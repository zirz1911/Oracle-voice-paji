use rumqttc::{AsyncClient, MqttOptions, QoS, Event, Packet};
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;

use crate::config::{MqttConfig, load_mqtt_config};
use crate::state::{AppState, SpeakRequest, VoiceEntry};
use crate::tray::update_tray_icon;

/// Run MQTT client with auto-reconnect on config change
pub async fn start_mqtt_client(state: Arc<AppState>, initial_config: MqttConfig) {
    let mut config = initial_config;

    loop {
        // Reset reconnect flag
        if let Ok(mut flag) = state.mqtt_reconnect.lock() {
            *flag = false;
        }

        // Run client until it needs to reconnect
        run_mqtt_session(&state, &config).await;

        // Check if we need to reconnect with new config
        let should_reconnect = state.mqtt_reconnect.lock()
            .map(|g| *g)
            .unwrap_or(false);
        if should_reconnect {
            println!("MQTT: Reconnecting with new config...");
            config = load_mqtt_config();
        } else {
            // Wait before auto-retry on error
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Single MQTT session - returns when disconnected or reconnect signaled
async fn run_mqtt_session(state: &Arc<AppState>, config: &MqttConfig) {
    // Update MQTT status to connecting
    if let Ok(mut mqtt_status) = state.mqtt_status.lock() {
        *mqtt_status = "connecting".to_string();
    }
    update_tray_icon(&state, false);

    println!("MQTT: Connecting to {}:{}", config.broker, config.port);
    let mut mqttoptions = MqttOptions::new("voice-tray-v2", &config.broker, config.port);
    mqttoptions.set_keep_alive(Duration::from_secs(30));
    mqttoptions.set_clean_session(true);

    // Set credentials if provided
    if let (Some(username), Some(password)) = (&config.username, &config.password) {
        if !username.is_empty() {
            println!("MQTT: Using authentication for user '{}'", username);
            mqttoptions.set_credentials(username, password);
        }
    }

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Subscribe to voice/speak topic (queues the request, doesn't wait for connection)
    if let Err(e) = client.subscribe(&config.topic_speak, QoS::AtLeastOnce).await {
        eprintln!("MQTT subscribe error: {:?}", e);
        if let Ok(mut mqtt_status) = state.mqtt_status.lock() {
            *mqtt_status = "disconnected".to_string();
        }
        update_tray_icon(&state, false);
        return;
    }
    println!("MQTT: Subscribe request sent to {}", config.topic_speak);

    // Note: "connected" status is set when we receive ConnAck in the event loop

    // Publish online status (retained) - will be sent when connected
    let status_json = serde_json::json!({
        "status": "online",
        "version": "0.2.0",
        "timestamp": Utc::now().to_rfc3339()
    });
    let _ = client.publish(
        &config.topic_status,
        QoS::AtLeastOnce,
        true,
        status_json.to_string()
    ).await;

    let client_clone = client.clone();

    // Event loop with reconnect check
    loop {
        // Check if reconnect requested
        let reconnect_requested = state.mqtt_reconnect.lock()
            .map(|g| *g)
            .unwrap_or(false);
        if reconnect_requested {
            println!("MQTT: Reconnect requested, closing session...");
            let _ = client.disconnect().await;
            return;
        }

        // Poll with timeout to allow checking reconnect flag
        match tokio::time::timeout(Duration::from_millis(100), eventloop.poll()).await {
            Ok(Ok(Event::Incoming(Packet::Publish(publish)))) => {
                if publish.topic == config.topic_speak {
                    match serde_json::from_slice::<SpeakRequest>(&publish.payload) {
                        Ok(req) => {
                            let id = state.next_id.lock()
                                .map(|mut next_id| {
                                    let id = *next_id;
                                    *next_id += 1;
                                    id
                                })
                                .unwrap_or(0);

                            let voice = req.voice.unwrap_or_else(|| "Samantha".to_string());
                            let rate = req.rate.unwrap_or(220);

                            let entry = VoiceEntry {
                                id,
                                timestamp: Utc::now(),
                                text: req.text.clone(),
                                voice: voice.clone(),
                                rate,
                                agent: req.agent.clone(),
                                status: "queued".to_string(),
                            };

                            if let Ok(mut timeline) = state.timeline.lock() {
                                timeline.push_back(entry);
                                while timeline.len() > 100 {
                                    timeline.pop_front();
                                }
                            }

                            println!("MQTT: Queued voice message #{}: {}", id, req.text);

                            if let Some(agent) = &req.agent {
                                let agent_topic = format!("voice/agent/{}/status", agent);
                                let agent_status = serde_json::json!({
                                    "last_message": req.text,
                                    "timestamp": Utc::now().to_rfc3339(),
                                    "id": id
                                });
                                let _ = client_clone.publish(
                                    agent_topic,
                                    QoS::AtLeastOnce,
                                    true,
                                    agent_status.to_string()
                                ).await;
                            }
                        }
                        Err(e) => {
                            eprintln!("MQTT: Failed to parse message: {:?}", e);
                        }
                    }
                }
            }
            Ok(Ok(Event::Incoming(Packet::ConnAck(_)))) => {
                println!("MQTT: Connected");
                if let Ok(mut mqtt_status) = state.mqtt_status.lock() {
                    *mqtt_status = "connected".to_string();
                }
                update_tray_icon(&state, false);
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                eprintln!("MQTT connection error: {:?}", e);
                if let Ok(mut mqtt_status) = state.mqtt_status.lock() {
                    *mqtt_status = "disconnected".to_string();
                }
                update_tray_icon(&state, false);
                return; // Exit session, will retry
            }
            Err(_) => {
                // Timeout - just continue to check reconnect flag
            }
        }
    }
}
