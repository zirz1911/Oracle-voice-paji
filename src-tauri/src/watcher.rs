/// Claude Code Session Watcher
/// Watches ~/.claude/projects/**/*.jsonl for assistant completions.
/// When Claude finishes a response (stop_reason: end_turn), queues a voice notification.
/// When Claude needs tool approval, detects via timeout:
///   tool_use written → no tool_result within 3s → user needs to approve.
/// No hooks, no subprocess, no window flash.
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::state::{AppState, VoiceEntry};

#[derive(Debug, PartialEq)]
enum LineEvent {
    None,
    Completion,   // stop_reason: end_turn
    ToolUse,      // stop_reason: tool_use — Claude is requesting a tool
    ToolResult,   // user message with tool_result — tool was executed (approved/denied)
}

/// How long to wait after tool_use before assuming user needs to approve.
/// Auto-approved tools produce tool_result within milliseconds.
const APPROVAL_TIMEOUT_SECS: u64 = 3;

pub fn start_session_watcher(state: Arc<AppState>) {
    std::thread::spawn(move || {
        let Some(projects_dir) = find_claude_projects_dir() else {
            println!("[watcher] ~/.claude/projects not found — session watcher disabled");
            return;
        };
        println!("[watcher] Watching: {}", projects_dir.display());

        let mut file_positions: HashMap<PathBuf, u64> = HashMap::new();
        let mut last_completion_notify: Option<Instant> = None;
        let mut last_approval_notify: Option<Instant> = None;

        // Tracks when we last saw tool_use without a subsequent tool_result
        let mut pending_tool_use: Option<Instant> = None;

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                println!("[watcher] Failed to create watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&projects_dir, RecursiveMode::Recursive) {
            println!("[watcher] Failed to watch directory: {}", e);
            return;
        }

        loop {
            // Use recv_timeout so we can check pending_tool_use expiry
            // even when the file isn't being written (user is looking at permission prompt)
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        continue;
                    }

                    for path in &event.paths {
                        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                            match check_new_lines(path, &mut file_positions) {
                                LineEvent::Completion => {
                                    pending_tool_use = None;
                                    let should_notify = last_completion_notify
                                        .map(|t| t.elapsed() > Duration::from_secs(2))
                                        .unwrap_or(true);
                                    if should_notify {
                                        last_completion_notify = Some(Instant::now());
                                        queue_voice(&state, "Task complete", 220);
                                    }
                                }
                                LineEvent::ToolUse => {
                                    // Start the approval timer — if no tool_result arrives
                                    // within APPROVAL_TIMEOUT_SECS, the user needs to act
                                    if pending_tool_use.is_none() {
                                        pending_tool_use = Some(Instant::now());
                                    }
                                }
                                LineEvent::ToolResult => {
                                    // Tool was auto-approved or executed — cancel pending
                                    pending_tool_use = None;
                                }
                                LineEvent::None => {}
                            }
                        }
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // No file event — check if pending tool_use has timed out
                }
            }

            // Check approval timeout (runs on every loop iteration)
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
    });
}

fn find_claude_projects_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join(".claude").join("projects");
    if path.exists() { Some(path) } else { None }
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
                    result = LineEvent::ToolUse;
                }
                _ => {}
            }
        }
    }
    result
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
