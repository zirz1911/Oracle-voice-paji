/// Claude Code Session Watcher
/// Watches ~/.claude/projects/**/*.jsonl for assistant completions and subagent spawns.
/// Approval alerts are handled by PreToolUse hooks in ~/.claude/settings.json.
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
    Completion,            // stop_reason: end_turn → "Claude Stop"
    SubagentSpawn(String), // tool_use name=Task → "Spawning <desc>"
}

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

        println!("[watcher] Watching: {}", projects_dir.display());

        let mut file_positions: HashMap<PathBuf, u64> = HashMap::new();
        let mut last_completion_notify: Option<Instant> = None;

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

        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        continue;
                    }

                    for path in &event.paths {
                        if !path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                            continue;
                        }

                        match check_new_lines(path, &mut file_positions) {
                            LineEvent::Completion => {
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
                            LineEvent::None => {}
                        }
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    });
}

/// Read new lines appended to a .jsonl file since last check.
fn check_new_lines(
    path: &PathBuf,
    positions: &mut HashMap<PathBuf, u64>,
) -> LineEvent {
    let Ok(mut file) = File::open(path) else { return LineEvent::None };
    let Ok(metadata) = file.metadata() else { return LineEvent::None };
    let file_size = metadata.len();

    // First time seeing this file — skip history, start tracking from current end
    let pos = positions.entry(path.clone()).or_insert(file_size);

    if file_size < *pos {
        *pos = 0; // file truncated/rotated
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
        if line.is_empty() || !line.contains("stop_reason") {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if json.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        match json.pointer("/message/stop_reason").and_then(|s| s.as_str()) {
            Some("end_turn") => {
                result = LineEvent::Completion;
            }
            Some("tool_use") => {
                if let Some(spawn) = extract_task_spawn(&json) {
                    return LineEvent::SubagentSpawn(spawn);
                }
                // Non-Task tool_use: approval handled by PreToolUse hook
            }
            _ => {}
        }
    }
    result
}

/// If the assistant message contains a Task tool_use, return its description.
fn extract_task_spawn(json: &serde_json::Value) -> Option<String> {
    let content = json.pointer("/message/content")?.as_array()?;
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            continue;
        }
        if item.get("name").and_then(|n| n.as_str()) != Some("Task") {
            continue;
        }
        let desc = item.pointer("/input/description")
            .and_then(|d| d.as_str())
            .or_else(|| item.pointer("/input/subagent_type").and_then(|t| t.as_str()))
            .unwrap_or("agent");
        return Some(desc.to_string());
    }
    None
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
