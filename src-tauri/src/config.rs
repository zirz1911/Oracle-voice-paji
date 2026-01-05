use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// MQTT Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub broker: String,
    pub port: u16,
    pub topic_speak: String,
    pub topic_status: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker: "127.0.0.1".to_string(),
            port: 1883,
            topic_speak: "voice/speak".to_string(),
            topic_status: "voice/status".to_string(),
            username: None,
            password: None,
        }
    }
}

/// Get config file path
pub fn get_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".oracle-voice-tray").join("config.json")
}

/// Load MQTT config from file or return defaults
pub fn load_mqtt_config() -> MqttConfig {
    let path = get_config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Failed to parse config: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to read config: {}", e),
        }
    }
    MqttConfig::default()
}

/// Save MQTT config to file
pub fn save_mqtt_config_to_file(config: &MqttConfig) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_mqtt_config_default() {
        let config = MqttConfig::default();
        assert_eq!(config.broker, "127.0.0.1");
        assert_eq!(config.port, 1883);
        assert_eq!(config.topic_speak, "voice/speak");
        assert_eq!(config.topic_status, "voice/status");
    }

    #[test]
    fn test_mqtt_config_serialization() {
        let config = MqttConfig {
            broker: "mqtt.example.com".to_string(),
            port: 8883,
            topic_speak: "custom/speak".to_string(),
            topic_status: "custom/status".to_string(),
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: MqttConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.broker, config.broker);
        assert_eq!(parsed.port, config.port);
        assert_eq!(parsed.topic_speak, config.topic_speak);
        assert_eq!(parsed.topic_status, config.topic_status);
    }

    #[test]
    fn test_config_persistence() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config_dir = temp_dir.path().join(".oracle-voice-tray");
        let config_path = config_dir.join("config.json");

        fs::create_dir_all(&config_dir).expect("create dir");

        let config = MqttConfig {
            broker: "test.broker.com".to_string(),
            port: 9999,
            topic_speak: "test/speak".to_string(),
            topic_status: "test/status".to_string(),
        };
        let json = serde_json::to_string_pretty(&config).expect("serialize");
        fs::write(&config_path, &json).expect("write config");

        let content = fs::read_to_string(&config_path).expect("read");
        let loaded: MqttConfig = serde_json::from_str(&content).expect("parse");

        assert_eq!(loaded.broker, "test.broker.com");
        assert_eq!(loaded.port, 9999);
    }
}
