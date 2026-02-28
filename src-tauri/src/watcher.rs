/// Claude Code Session Watcher
/// Watches ~/.claude/projects/**/*.jsonl for assistant completions.
/// Reads ~/.claude/settings.json to detect current permission mode and gate alerts.
///
/// Permission modes:
///   SkipAll        — skipDangerousModePermissionPrompt:true  → no approval alerts
///   AutoAcceptEdits — autoAcceptEdits:true                   → alert only for non-edit tools
///   Normal         — default                                  → 3s timer alerts
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::state::{AppState, VoiceEntry};

#[derive(Debug, Clone, PartialEq)]
enum PermissionMode {
    /// --dangerously-skip-permissions: all tools auto-approved, no alerts
    SkipAll,
    /// "Accept edits on": file edit tools auto-approved, bash/etc still need approval
    AutoAcceptEdits,
    /// Default: all tools require explicit approval
    Normal,
}

fn read_permission_mode(home: &PathBuf) -> PermissionMode {
    let path = home.join(".claude").join("settings.json");
    let Ok(content) = fs::read_to_string(&path) else {
        return PermissionMode::Normal;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return PermissionMode::Normal;
    };

    if json.get("skipDangerousModePermissionPrompt")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return PermissionMode::SkipAll;
    }

    // "Accept edits on" mode — Claude Code may use one of these field names
    let auto_accept = json.get("autoAcceptEdits")
        .or_else(|| json.get("acceptEdits"))
        .or_else(|| json.get("autoApproveEdits"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if auto_accept {
        return PermissionMode::AutoAcceptEdits;
    }

    PermissionMode::Normal
}

#[derive(Debug, PartialEq)]
enum LineEvent {
    None,
    Completion,              // stop_reason: end_turn → "Claude Stop"
    ToolUse(bool),           // stop_reason: tool_use — bool: needs_approval (Bash only)
    SubagentSpawn(String),   // tool_use name=Task → announce description immediately
    ToolResult,              // user message with tool_result — tool was executed
}

/// Tools that may require explicit user approval in Normal mode.
/// Read-only and safe tools (Read, Glob, Grep, WebFetch, WebSearch, Agent/Task)
/// are auto-approved and should not trigger the approval timer.
const APPROVAL_TOOLS: &[&str] = &["Bash", "Edit", "Write", "MultiEdit", "NotebookEdit"];

/// How long to wait after tool_use before assuming user needs to approve.
/// 15s to avoid false positives from slow-running auto-approved tools.
const APPROVAL_TIMEOUT_SECS: u64 = 15;

pub fn start_session_watcher(state: Arc<AppState>) {
    std::thread::spawn(move || {
        let Some(home) = dirs::home_dir() else {
            println!("[watcher] home dir not found — session watcher disabled");
            return;
        };

        let projects_dir = home.join(".claude").join("projects");
        if !projects_dir.exists() {
            println!("[watcher] ~/.claude/projects not found — session watcher disabled");
            return;
        }

        let settings_file = home.join(".claude").join("settings.json");
        println!("[watcher] Watching: {}", projects_dir.display());

        let mut file_positions: HashMap<PathBuf, u64> = HashMap::new();
        let mut last_completion_notify: Option<Instant> = None;
        let mut last_approval_notify: Option<Instant> = None;
        let mut pending_tool_use: Option<Instant> = None;
        let mut perm_mode = read_permission_mode(&home);
        println!("[watcher] Permission mode: {:?}", perm_mode);

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                println!("[watcher] Failed to create watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&projects_dir, RecursiveMode::Recursive) {
            println!("[watcher] Failed to watch projects dir: {}", e);
            return;
        }
        // Also watch settings.json so mode changes take effect immediately
        if settings_file.exists() {
            let _ = watcher.watch(&settings_file, RecursiveMode::NonRecursive);
        }

        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        continue;
                    }

                    for path in &event.paths {
                        // Re-read mode when settings.json changes
                        if path == &settings_file {
                            perm_mode = read_permission_mode(&home);
                            println!("[watcher] Permission mode updated: {:?}", perm_mode);
                            // If mode changed to skip-all, clear any pending alert
                            if perm_mode == PermissionMode::SkipAll {
                                pending_tool_use = None;
                            }
                            continue;
                        }

                        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                            match check_new_lines(path, &mut file_positions) {
                                LineEvent::Completion => {
                                    pending_tool_use = None;
                                    let should_notify = last_completion_notify
                                        .map(|t| t.elapsed() > Duration::from_secs(2))
                                        .unwrap_or(true);
                                    if should_notify {
                                        last_completion_notify = Some(Instant::now());
                                        queue_voice(&state, "Claude Stop", 220);
                                    }
                                }
                                LineEvent::SubagentSpawn(desc) => {
                                    queue_voice(&state, &format!("Spawning {}", desc), 230);
                                }
                                LineEvent::ToolUse(needs_approval) => {
                                    // Only start timer when mode requires approval AND tool is approval-gated
                                    if perm_mode == PermissionMode::Normal
                                        && needs_approval
                                        && pending_tool_use.is_none()
                                    {
                                        pending_tool_use = Some(Instant::now());
                                    }
                                }
                                LineEvent::ToolResult => {
                                    pending_tool_use = None;
                                }
                                LineEvent::None => {}
                            }
                        }
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            // Check approval timeout — only in Normal mode
            if perm_mode == PermissionMode::Normal {
                if let Some(t) = pending_tool_use {
                    if t.elapsed() > Duration::from_secs(APPROVAL_TIMEOUT_SECS) {
                        pending_tool_use = None;
                        let should_notify = last_approval_notify
                            .map(|t| t.elapsed() > Duration::from_secs(10))
                            .unwrap_or(true);
                        if should_notify {
                            last_approval_notify = Some(Instant::now());
                            queue_voice(&state, "Action needed, please approve", 240);
                        }
                    }
                }
            }
        }
    });
}

/// Read new lines appended to a .jsonl file since last check.
/// Returns the most significant event found in the new lines.
/// Priority: ToolResult > ToolUse > Completion > None
fn check_new_lines(
    path: &PathBuf,
    positions: &mut HashMap<PathBuf, u64>,
) -> LineEvent {
    let Ok(mut file) = File::open(path) else { return LineEvent::None };
    let Ok(metadata) = file.metadata() else { return LineEvent::None };
    let file_size = metadata.len();

    // First time seeing this file — skip history, start tracking from current end
    let pos = positions.entry(path.clone()).or_insert(file_size);

    // File was truncated or rotated — reset
    if file_size < *pos {
        *pos = 0;
    }

    if file_size == *pos {
        return LineEvent::None;
    }

    let _ = file.seek(SeekFrom::Start(*pos));
    let mut new_content = String::new();
    let _ = file.read_to_string(&mut new_content);
    *pos = file_size;

    let mut result = LineEvent::None;

    for line in new_content.lines() {
        if line.is_empty() {
            continue;
        }

        // Detect tool_result (user message after tool execution)
        // This fires whether the tool was auto-approved or user-approved
        if line.contains("tool_result") && line.contains("\"type\":\"user\"") {
            return LineEvent::ToolResult;
        }

        // Detect assistant stop reasons
        if !line.contains("stop_reason") {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                continue;
            }
            match json.pointer("/message/stop_reason").and_then(|s| s.as_str()) {
                Some("end_turn") => {
                    result = LineEvent::Completion;
                }
                Some("tool_use") => {
                    // Check if any tool in the content array is a Task (subagent spawn)
                    if let Some(spawn) = extract_task_spawn(&json) {
                        return LineEvent::SubagentSpawn(spawn);
                    }
                    let needs_approval = extract_tool_names(&json)
                        .iter()
                        .any(|name| APPROVAL_TOOLS.contains(&name.as_str()));
                    result = LineEvent::ToolUse(needs_approval);
                }
                _ => {}
            }
        }
    }
    result
}

/// If the assistant message contains a Task tool_use, return its description.
/// Falls back to subagent_type if description is absent.
fn extract_task_spawn(json: &serde_json::Value) -> Option<String> {
    let content = json.pointer("/message/content")?.as_array()?;
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            continue;
        }
        if item.get("name").and_then(|n| n.as_str()) != Some("Task") {
            continue;
        }
        // Prefer description, fall back to subagent_type
        let desc = item.pointer("/input/description")
            .and_then(|d| d.as_str())
            .or_else(|| item.pointer("/input/subagent_type").and_then(|t| t.as_str()))
            .unwrap_or("agent");
        return Some(desc.to_string());
    }
    None
}

/// Return all tool names used in this assistant message.
fn extract_tool_names(json: &serde_json::Value) -> Vec<String> {
    let Some(content) = json.pointer("/message/content").and_then(|c| c.as_array()) else {
        return vec![];
    };
    content.iter()
        .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .filter_map(|item| item.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .collect()
}

fn queue_voice(state: &Arc<AppState>, text: &str, rate: u32) {
    let id = state
        .next_id
        .lock()
        .map(|mut n| {
            let i = *n;
            *n += 1;
            i
        })
        .unwrap_or(0);

    if let Ok(mut timeline) = state.timeline.lock() {
        timeline.push_back(VoiceEntry {
            id,
            timestamp: Utc::now(),
            text: text.to_string(),
            voice: "Samantha".to_string(),
            rate,
            agent: Some("claude".to_string()),
            status: "queued".to_string(),
        });
        while timeline.len() > 100 {
            timeline.pop_front();
        }
    }
    println!("[watcher] Voice queued: {}", text);
}
