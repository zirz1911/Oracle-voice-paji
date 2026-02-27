/// Claude Code Session Watcher
/// Watches ~/.claude/projects/**/*.jsonl for assistant completions.
/// When Claude finishes a response (stop_reason: end_turn), queues a voice notification.
/// No hooks, no subprocess, no window flash.
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::state::{AppState, VoiceEntry};

pub fn start_session_watcher(state: Arc<AppState>) {
    std::thread::spawn(move || {
        let Some(projects_dir) = find_claude_projects_dir() else {
            println!("[watcher] ~/.claude/projects not found — session watcher disabled");
            return;
        };
        println!("[watcher] Watching: {}", projects_dir.display());

        let mut file_positions: HashMap<PathBuf, u64> = HashMap::new();
        let mut last_notify: Option<Instant> = None;

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

        for result in rx {
            let event = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                continue;
            }

            for path in &event.paths {
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    let found = check_new_completion_lines(path, &mut file_positions);
                    if found {
                        // Debounce: skip if another notification fired within 2s
                        let should_notify = last_notify
                            .map(|t| t.elapsed() > Duration::from_secs(2))
                            .unwrap_or(true);

                        if should_notify {
                            last_notify = Some(Instant::now());
                            queue_completion_voice(&state);
                        }
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
/// Returns true if an assistant completion (stop_reason: end_turn) was found.
fn check_new_completion_lines(
    path: &PathBuf,
    positions: &mut HashMap<PathBuf, u64>,
) -> bool {
    let Ok(mut file) = File::open(path) else { return false };
    let Ok(metadata) = file.metadata() else { return false };
    let file_size = metadata.len();

    // First time seeing this file — skip history, start tracking from current end
    let pos = positions.entry(path.clone()).or_insert(file_size);

    // File was truncated or rotated — reset
    if file_size < *pos {
        *pos = 0;
    }

    if file_size == *pos {
        return false;
    }

    let _ = file.seek(SeekFrom::Start(*pos));
    let mut new_content = String::new();
    let _ = file.read_to_string(&mut new_content);
    *pos = file_size;

    for line in new_content.lines() {
        if line.is_empty() {
            continue;
        }
        // Fast pre-check before full JSON parse
        if !line.contains("end_turn") {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            let is_done = json.get("type").and_then(|t| t.as_str()) == Some("assistant")
                && json.pointer("/message/stop_reason").and_then(|s| s.as_str())
                    == Some("end_turn");
            if is_done {
                return true;
            }
        }
    }
    false
}

fn queue_completion_voice(state: &Arc<AppState>) {
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
            text: "Task complete".to_string(),
            voice: "Samantha".to_string(),
            rate: 220,
            agent: Some("claude".to_string()),
            status: "queued".to_string(),
        });
        while timeline.len() > 100 {
            timeline.pop_front();
        }
    }
    println!("[watcher] Voice queued: Task complete");
}
