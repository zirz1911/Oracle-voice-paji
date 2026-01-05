use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
    image::Image,
    Manager, AppHandle, PhysicalPosition,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use chrono::Utc;

mod config;
mod state;
mod mqtt;
mod http;
mod tray;

pub use config::{MqttConfig, load_mqtt_config, save_mqtt_config_to_file};
pub use state::{AppState, VoiceEntry, SpeakRequest, SpeakResponse};
pub use tray::update_tray_icon;

// Debounce for click events
static LAST_CLICK: Mutex<Option<Instant>> = Mutex::new(None);

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
    if let Ok(mut last_click) = LAST_CLICK.lock() {
        if let Some(last) = *last_click {
            if last.elapsed() < Duration::from_millis(300) {
                return;
            }
        }
        *last_click = Some(Instant::now());
    } else {
        return;
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
    state.timeline.lock()
        .map(|timeline| timeline.iter().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
fn get_status(state: tauri::State<'_, Arc<AppState>>) -> serde_json::Value {
    let (total, queued_count) = state.timeline.lock()
        .map(|t| (t.len(), t.iter().filter(|e| e.status == "queued").count()))
        .unwrap_or((0, 0));
    let is_speaking = state.is_speaking.lock().map(|g| *g).unwrap_or(false);
    let mqtt_status = state.mqtt_status.lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    serde_json::json!({
        "total": total,
        "queued": queued_count,
        "is_speaking": is_speaking,
        "server_port": http::VOICE_SERVER_PORT,
        "mqtt_status": mqtt_status
    })
}

#[tauri::command]
fn clear_timeline(state: tauri::State<'_, Arc<AppState>>) {
    if let Ok(mut timeline) = state.timeline.lock() {
        timeline.retain(|e| e.status != "done");
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn test_voice(state: tauri::State<'_, Arc<AppState>>) {
    if let Ok(mut timeline) = state.timeline.lock() {
        let id = timeline.len() as u64 + 1;
        timeline.push_back(VoiceEntry {
            id,
            timestamp: Utc::now(),
            text: "Hello! Voice Tray is working.".to_string(),
            voice: "Samantha".to_string(),
            rate: 175,
            agent: Some("Test".to_string()),
            status: "queued".to_string(),
        });
    }
}

#[tauri::command]
fn get_mqtt_config() -> MqttConfig {
    load_mqtt_config()
}

#[tauri::command]
fn save_mqtt_config(config: MqttConfig, state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    // Check if config actually changed
    let current = load_mqtt_config();
    let changed = config.broker != current.broker
        || config.port != current.port
        || config.topic_speak != current.topic_speak
        || config.topic_status != current.topic_status
        || config.username != current.username
        || config.password != current.password;

    save_mqtt_config_to_file(&config)?;

    if changed {
        // Set status to disconnected immediately so UI shows the transition
        if let Ok(mut status) = state.mqtt_status.lock() {
            *status = "disconnected".to_string();
        }
        // Update tray icon to disconnected
        update_tray_icon(&state, false);
        // Signal MQTT to reconnect
        if let Ok(mut reconnect) = state.mqtt_reconnect.lock() {
            *reconnect = true;
        }
        Ok("Settings saved. Reconnecting...".to_string())
    } else {
        Ok("Settings saved.".to_string())
    }
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
    tray::process_queue(state_queue);

    // Start HTTP server in background
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(http::start_http_server(state_http));
    });

    // Load MQTT config and start client in background
    let mqtt_config = load_mqtt_config();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(mqtt::start_mqtt_client(state_mqtt, mqtt_config));
    });

    let state_setup = state.clone();

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Load custom icons (embedded at compile time)
            let idle_bytes = include_bytes!("../icons/idle.png");
            let speaking_bytes = include_bytes!("../icons/speaking.png");
            let disconnected_bytes = include_bytes!("../icons/disconnected.png");

            let idle_icon = image::load_from_memory(idle_bytes).ok().map(|img| {
                let rgba = img.to_rgba8();
                println!("Loaded idle icon: {}x{}", rgba.width(), rgba.height());
                Image::new_owned(rgba.to_vec(), rgba.width(), rgba.height())
            });

            let speaking_icon = image::load_from_memory(speaking_bytes).ok().map(|img| {
                let rgba = img.to_rgba8();
                println!("Loaded speaking icon: {}x{}", rgba.width(), rgba.height());
                Image::new_owned(rgba.to_vec(), rgba.width(), rgba.height())
            });

            let disconnected_icon = image::load_from_memory(disconnected_bytes).ok().map(|img| {
                let rgba = img.to_rgba8();
                println!("Loaded disconnected icon: {}x{}", rgba.width(), rgba.height());
                Image::new_owned(rgba.to_vec(), rgba.width(), rgba.height())
            });

            // Store icons in state
            *state_setup.idle_icon.lock().unwrap() = idle_icon.clone();
            *state_setup.speaking_icon.lock().unwrap() = speaking_icon;
            *state_setup.disconnected_icon.lock().unwrap() = disconnected_icon.clone();

            // Create right-click menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit Oracle Voice Tray", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            // Use disconnected icon initially (MQTT not connected yet)
            let initial_icon = disconnected_icon
                .or(idle_icon)
                .unwrap_or_else(|| app.default_window_icon().unwrap().clone());

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
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event {
                        toggle_popup(tray.app_handle(), position.x, position.y);
                    }
                })
                .build(app)?;
            println!("Tray icon created successfully!");

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
        .invoke_handler(tauri::generate_handler![
            get_timeline, get_status, clear_timeline, quit_app,
            test_voice, get_mqtt_config, save_mqtt_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
