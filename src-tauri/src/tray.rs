use std::sync::Arc;
use std::time::Duration;
use std::process::Command;

use crate::state::AppState;

/// Update tray icon based on speaking state and MQTT connection
/// Uses a specific lock order to prevent deadlocks: mqtt_status -> icons -> tray_icon
pub fn update_tray_icon(state: &Arc<AppState>, speaking: bool) {
    let mqtt_status = match state.mqtt_status.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return,
    };

    let icon = if mqtt_status != "connected" {
        match state.disconnected_icon.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return,
        }
    } else if speaking {
        match state.speaking_icon.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return,
        }
    } else {
        match state.idle_icon.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return,
        }
    };

    if let Ok(tray_guard) = state.tray_icon.lock() {
        if let Some(ref tray) = *tray_guard {
            if let Some(img) = icon {
                let _ = tray.set_icon(Some(img));
            }
        }
    }
}

/// Map voice name to Windows SAPI voice (David=male, Zira=female)
#[cfg(target_os = "windows")]
fn map_voice_windows(voice: &str) -> &'static str {
    match voice.to_lowercase().as_str() {
        "samantha" | "karen" | "victoria" | "fiona" | "moira" => "Microsoft Zira Desktop",
        "daniel" | "alex" | "rishi" | "tom" => "Microsoft David Desktop",
        _ => "Microsoft David Desktop",
    }
}

/// Convert words-per-minute (150-300) to SAPI rate (-10 to 10)
#[cfg(target_os = "windows")]
fn wpm_to_sapi_rate(wpm: u32) -> i32 {
    // 220 wpm ≈ rate 0 (default), scale ±10
    let delta = wpm as i32 - 220;
    (delta / 15).clamp(-10, 10)
}

/// Speak text using Windows SAPI via PowerShell (hidden — CREATE_NO_WINDOW)
#[cfg(target_os = "windows")]
pub fn speak_text(text: &str, voice: &str, rate: u32) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let sapi_voice = map_voice_windows(voice);
    let sapi_rate = wpm_to_sapi_rate(rate);
    // Escape single quotes in text to avoid PS injection
    let safe_text = text.replace('\'', " ");
    let ps_script = format!(
        "Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $s.SelectVoice('{}'); \
         $s.Rate = {}; \
         $s.Speak('{}')",
        sapi_voice, sapi_rate, safe_text
    );
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .and_then(|mut child| child.wait());
}

/// Speak text using macOS say command with rate
#[cfg(target_os = "macos")]
pub fn speak_text(text: &str, voice: &str, rate: u32) {
    let _ = Command::new("say")
        .args(["-v", voice, "-r", &rate.to_string(), text])
        .spawn()
        .and_then(|mut child| child.wait());
}

/// Speak text using espeak on Linux
#[cfg(target_os = "linux")]
pub fn speak_text(text: &str, _voice: &str, rate: u32) {
    let _ = Command::new("espeak")
        .args(["-s", &rate.to_string(), text])
        .spawn()
        .and_then(|mut child| child.wait());
}

/// Process voice queue in a background thread
pub fn process_queue(state: Arc<AppState>) {
    std::thread::spawn(move || {
        loop {
            let entry_opt = {
                let Ok(mut timeline) = state.timeline.lock() else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                if let Some(e) = timeline.iter_mut().find(|e| e.status == "queued") {
                    e.status = "speaking".to_string();
                    Some(e.clone())
                } else {
                    None
                }
            };

            if let Some(entry) = entry_opt {
                if let Ok(mut is_speaking) = state.is_speaking.lock() {
                    *is_speaking = true;
                }
                update_tray_icon(&state, true);

                speak_text(&entry.text, &entry.voice, entry.rate);

                if let Ok(mut timeline) = state.timeline.lock() {
                    if let Some(e) = timeline.iter_mut().find(|e| e.id == entry.id) {
                        e.status = "done".to_string();
                    }
                }
                if let Ok(mut is_speaking) = state.is_speaking.lock() {
                    *is_speaking = false;
                }
                update_tray_icon(&state, false);
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    });
}
