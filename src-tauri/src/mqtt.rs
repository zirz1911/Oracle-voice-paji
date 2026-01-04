use rumqttc::{AsyncClient, MqttOptions, QoS, Event, Packet};
use std::sync::Arc;
use std::time::Duration;
use crate::{AppState, SpeakRequest, VoiceEntry};
use chrono::Utc;

const MQTT_BROKER: &str = "127.0.0.1";
const MQTT_PORT: u16 = 1883;
const TOPIC_SPEAK: &str = "voice/speak";
const TOPIC_STATUS: &str = "voice/status";

pub async fn start_mqtt_client(state: Arc<AppState>) {
    // Update MQTT status to connecting
    {
        let mut mqtt_status = state.mqtt_status.lock().unwrap();
        *mqtt_status = "connecting".to_string();
    }

    let mut mqttoptions = MqttOptions::new("voice-tray-v2", MQTT_BROKER, MQTT_PORT);
    mqttoptions.set_keep_alive(Duration::from_secs(30));
    mqttoptions.set_clean_session(false); // Persist session for offline messages

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Subscribe to voice/speak topic
    if let Err(e) = client.subscribe(TOPIC_SPEAK, QoS::AtLeastOnce).await {
        eprintln!("MQTT subscribe error: {:?}", e);
        let mut mqtt_status = state.mqtt_status.lock().unwrap();
        *mqtt_status = format!("error: {:?}", e);
        return;
    }
    println!("MQTT: Subscribed to {}", TOPIC_SPEAK);

    // Update MQTT status to connected
    {
        let mut mqtt_status = state.mqtt_status.lock().unwrap();
        *mqtt_status = "connected".to_string();
    }

    // Publish online status (retained)
    let status_json = serde_json::json!({
        "status": "online",
        "version": "0.2.0",
        "timestamp": Utc::now().to_rfc3339()
    });
    let _ = client.publish(
        TOPIC_STATUS,
        QoS::AtLeastOnce,
        true, // retained
        status_json.to_string()
    ).await;

    // Store client for status publishing
    let client_clone = client.clone();

    // Event loop
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                if publish.topic == TOPIC_SPEAK {
                    match serde_json::from_slice::<SpeakRequest>(&publish.payload) {
                        Ok(req) => {
                            let id = {
                                let mut next_id = state.next_id.lock().unwrap();
                                let id = *next_id;
                                *next_id += 1;
                                id
                            };

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

                            {
                                let mut timeline = state.timeline.lock().unwrap();
                                timeline.push_back(entry);
                                while timeline.len() > 100 {
                                    timeline.pop_front();
                                }
                            }

                            println!("MQTT: Queued voice message #{}: {}", id, req.text);

                            // Publish per-agent status if agent specified
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
                                    true, // retained
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
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                println!("MQTT: Connected");
                let mut mqtt_status = state.mqtt_status.lock().unwrap();
                *mqtt_status = "connected".to_string();
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("MQTT connection error: {:?}", e);
                {
                    let mut mqtt_status = state.mqtt_status.lock().unwrap();
                    *mqtt_status = "disconnected".to_string();
                }
                // Wait before reconnect
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
