use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
    image::Image,
    Manager, AppHandle, PhysicalPosition,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::process::Command;
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

mod mqtt;

// HTTP server port
const VOICE_SERVER_PORT: u16 = 37779;

// Debounce for click events
static LAST_CLICK: Mutex<Option<Instant>> = Mutex::new(None);

// Voice entry for timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceEntry {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub text: String,
    pub voice: String,
    pub rate: u32,  // Speech rate in wpm
    pub agent: Option<String>,
    pub status: String, // "queued", "speaking", "done"
}

// Shared state
pub struct AppState {
    pub timeline: Mutex<VecDeque<VoiceEntry>>,
    pub next_id: Mutex<u64>,
    pub is_speaking: Mutex<bool>,
    pub mqtt_status: Mutex<String>,
    pub tray_icon: Mutex<Option<TrayIcon>>,
    pub idle_icon: Mutex<Option<Image<'static>>>,
    pub speaking_icon: Mutex<Option<Image<'static>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            timeline: Mutex::new(VecDeque::with_capacity(100)),
            next_id: Mutex::new(1),
            is_speaking: Mutex::new(false),
            mqtt_status: Mutex::new("disconnected".to_string()),
            tray_icon: Mutex::new(None),
            idle_icon: Mutex::new(None),
            speaking_icon: Mutex::new(None),
        }
    }
}

// Request to speak
#[derive(Debug, Deserialize)]
pub struct SpeakRequest {
    pub text: String,
    pub voice: Option<String>,
    pub agent: Option<String>,
    pub rate: Option<u32>,  // Speech rate in words per minute (default 220)
}

// Response from speak endpoint
#[derive(Debug, Serialize)]
pub struct SpeakResponse {
    pub id: u64,
    pub status: String,
}

/// Speak text using macOS say command with rate
fn speak_text(text: &str, voice: &str, rate: u32) {
    let _ = Command::new("say")
        .args(["-v", voice, "-r", &rate.to_string(), text])
        .spawn()
        .and_then(|mut child| child.wait());
}

/// Update tray icon based on speaking state
fn update_tray_icon(state: &Arc<AppState>, speaking: bool) {
    let tray_guard = state.tray_icon.lock().unwrap();
    if let Some(ref tray) = *tray_guard {
        let icon = if speaking {
            state.speaking_icon.lock().unwrap().clone()
        } else {
            state.idle_icon.lock().unwrap().clone()
        };
        if let Some(img) = icon {
            let _ = tray.set_icon(Some(img));
        }
    }
}

/// Process voice queue
fn process_queue(state: Arc<AppState>) {
    std::thread::spawn(move || {
        loop {
            let entry_opt = {
                let mut timeline = state.timeline.lock().unwrap();
                // Find first queued entry and mark as speaking immediately (prevents re-processing)
                if let Some(e) = timeline.iter_mut().find(|e| e.status == "queued") {
                    e.status = "speaking".to_string();
                    Some(e.clone())
                } else {
                    None
                }
            };

            if let Some(entry) = entry_opt {
                // Update speaking state
                {
                    *state.is_speaking.lock().unwrap() = true;
                }
                // Update tray icon to speaking
                update_tray_icon(&state, true);

                // Speak
                speak_text(&entry.text, &entry.voice, entry.rate);

                // Mark as done
                {
                    let mut timeline = state.timeline.lock().unwrap();
                    if let Some(e) = timeline.iter_mut().find(|e| e.id == entry.id) {
                        e.status = "done".to_string();
                    }
                    *state.is_speaking.lock().unwrap() = false;
                }
                // Update tray icon to idle
                update_tray_icon(&state, false);
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Show popup window near tray icon
fn show_popup(app: &AppHandle, x: f64, y: f64) {
    if let Some(window) = app.get_webview_window("main") {
        let pos = PhysicalPosition::new((x - 200.0) as i32, (y + 30.0) as i32);
        let _ = window.set_position(pos);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Hide popup window
fn hide_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// Toggle popup visibility with debounce
fn toggle_popup(app: &AppHandle, x: f64, y: f64) {
    {
        let mut last_click = LAST_CLICK.lock().unwrap();
        if let Some(last) = *last_click {
            if last.elapsed() < Duration::from_millis(300) {
                return;
            }
        }
        *last_click = Some(Instant::now());
    }

    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_popup(app, x, y);
        }
    }
}

// Tauri commands
#[tauri::command]
fn get_timeline(state: tauri::State<'_, Arc<AppState>>) -> Vec<VoiceEntry> {
    let timeline = state.timeline.lock().unwrap();
    timeline.iter().cloned().collect()
}

#[tauri::command]
fn get_status(state: tauri::State<'_, Arc<AppState>>) -> serde_json::Value {
    let timeline = state.timeline.lock().unwrap();
    let is_speaking = *state.is_speaking.lock().unwrap();
    let mqtt_status = state.mqtt_status.lock().unwrap().clone();
    let queued_count = timeline.iter().filter(|e| e.status == "queued").count();

    serde_json::json!({
        "total": timeline.len(),
        "queued": queued_count,
        "is_speaking": is_speaking,
        "server_port": VOICE_SERVER_PORT,
        "mqtt_status": mqtt_status
    })
}

#[tauri::command]
fn clear_timeline(state: tauri::State<'_, Arc<AppState>>) {
    let mut timeline = state.timeline.lock().unwrap();
    timeline.retain(|e| e.status != "done");
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn test_voice(state: tauri::State<'_, Arc<AppState>>) {
    let mut timeline = state.timeline.lock().unwrap();
    let id = timeline.len() as u64 + 1;
    timeline.push_back(VoiceEntry {
        id,
        timestamp: chrono::Utc::now(),
        text: "Hello! Voice Tray is working.".to_string(),
        voice: "Samantha".to_string(),
        rate: 175,
        agent: Some("Test".to_string()),
        status: "queued".to_string(),
    });
}

/// Start HTTP server for receiving voice requests
async fn start_http_server(state: Arc<AppState>) {
    use axum::{
        routing::{get, post},
        Json, Router,
        extract::State,
    };

    let app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(r#"<!DOCTYPE html>
<html><head><title>Voice Tray API</title>
<style>body{font-family:system-ui;max-width:600px;margin:40px auto;padding:20px;background:#1a1a2e;color:#eee}
h1{color:#0f9}code{background:#333;padding:2px 6px;border-radius:4px}
pre{background:#222;padding:15px;border-radius:8px;overflow-x:auto}</style></head>
<body><h1>🎙️ Voice Tray API</h1>
<p>Endpoints:</p>
<ul>
<li><code>POST /speak</code> - Queue text for speech</li>
<li><code>GET /timeline</code> - Get speech queue</li>
<li><code>GET /status</code> - Get server status</li>
</ul>
<h3>Example:</h3>
<pre>curl -X POST http://127.0.0.1:37779/speak \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello!","voice":"Samantha"}'</pre>
</body></html>"#)
        }))
        .route("/speak", post(|State(state): State<Arc<AppState>>, Json(req): Json<SpeakRequest>| async move {
            let id = {
                let mut next_id = state.next_id.lock().unwrap();
                let id = *next_id;
                *next_id += 1;
                id
            };

            let voice = req.voice.unwrap_or_else(|| "Samantha".to_string());
            let rate = req.rate.unwrap_or(220);  // Default 220 wpm (fast)

            let entry = VoiceEntry {
                id,
                timestamp: Utc::now(),
                text: req.text,
                voice: voice.clone(),
                rate,
                agent: req.agent,
                status: "queued".to_string(),
            };

            {
                let mut timeline = state.timeline.lock().unwrap();
                timeline.push_back(entry);
                // Keep only last 100 entries
                while timeline.len() > 100 {
                    timeline.pop_front();
                }
            }

            Json(SpeakResponse { id, status: "queued".to_string() })
        }))
        .route("/timeline", get(|State(state): State<Arc<AppState>>| async move {
            let timeline = state.timeline.lock().unwrap();
            Json(timeline.iter().cloned().collect::<Vec<_>>())
        }))
        .route("/status", get(|State(state): State<Arc<AppState>>| async move {
            let timeline = state.timeline.lock().unwrap();
            let is_speaking = *state.is_speaking.lock().unwrap();
            let mqtt_status = state.mqtt_status.lock().unwrap().clone();
            Json(serde_json::json!({
                "total": timeline.len(),
                "queued": timeline.iter().filter(|e| e.status == "queued").count(),
                "is_speaking": is_speaking,
                "mqtt_status": mqtt_status
            }))
        }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", VOICE_SERVER_PORT))
        .await
        .expect("Failed to bind HTTP server");

    println!("Voice HTTP server listening on http://127.0.0.1:{}", VOICE_SERVER_PORT);
    axum::serve(listener, app).await.unwrap();
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("Oracle Voice Tray v{} starting...", VERSION);

    let state = Arc::new(AppState::default());
    let state_queue = state.clone();
    let state_http = state.clone();
    let state_mqtt = state.clone();

    // Start voice queue processor
    process_queue(state_queue);

    // Start HTTP server in background
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(start_http_server(state_http));
    });

    // Start MQTT client in background
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(mqtt::start_mqtt_client(state_mqtt));
    });

    let state_setup = state.clone();

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Load custom icons for lips (embedded at compile time)
            let idle_bytes = include_bytes!("../icons/idle.png");
            let speaking_bytes = include_bytes!("../icons/speaking.png");

            // Decode PNGs to RGBA using image crate
            let idle_icon = match image::load_from_memory(idle_bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    println!("Loaded idle icon: {}x{}", rgba.width(), rgba.height());
                    Some(Image::new_owned(rgba.to_vec(), rgba.width(), rgba.height()))
                }
                Err(e) => {
                    println!("Failed to load idle icon: {}", e);
                    None
                }
            };
            let speaking_icon = match image::load_from_memory(speaking_bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    println!("Loaded speaking icon: {}x{}", rgba.width(), rgba.height());
                    Some(Image::new_owned(rgba.to_vec(), rgba.width(), rgba.height()))
                }
                Err(e) => {
                    println!("Failed to load speaking icon: {}", e);
                    None
                }
            };

            // Store icons in state
            *state_setup.idle_icon.lock().unwrap() = idle_icon.clone();
            *state_setup.speaking_icon.lock().unwrap() = speaking_icon;

            // Create right-click menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit Oracle Voice Tray", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            // Use idle lips icon or fall back to default
            let initial_icon = match idle_icon {
                Some(icon) => {
                    println!("Using custom lips icon for tray");
                    icon
                }
                None => {
                    println!("Falling back to default window icon");
                    app.default_window_icon().unwrap().clone()
                }
            };

            // Build tray icon
            println!("Building tray icon...");
            let tray = TrayIconBuilder::new()
                .icon(initial_icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Oracle Voice Tray - MQTT + HTTP")
                .on_menu_event(move |app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } => {
                            let app = tray.app_handle();
                            toggle_popup(app, position.x, position.y);
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            println!("Tray icon created successfully!");

            // Store tray reference in state
            *state_setup.tray_icon.lock().unwrap() = Some(tray);

            // Hide popup when it loses focus
            let app_handle_blur = app_handle.clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        hide_popup(&app_handle_blur);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_timeline, get_status, clear_timeline, quit_app, test_voice])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
