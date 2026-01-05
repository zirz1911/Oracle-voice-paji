# Oracle Voice Tray

Centralized text-to-speech for Claude Code agents. A macOS menu bar app (Tauri 2.0) that receives voice requests via **HTTP** or **MQTT**, queues them, speaks using macOS `say`, and displays a timeline UI.

## For AI Assistants

```
ORACLE VOICE TRAY - AI CONTEXT

HTTP API (http://127.0.0.1:37779):
  POST /speak    - Queue voice message
  GET  /timeline - Get all entries
  GET  /status   - Get status + MQTT state
  GET  /         - API docs page

Send voice (HTTP):
  curl -X POST http://127.0.0.1:37779/speak \
    -H "Content-Type: application/json" \
    -d '{"text":"Hello","agent":"Claude"}'

Send voice (MQTT):
  mosquitto_pub -t voice/speak -m '{"text":"Hello","agent":"Claude"}'

Payload: {"text":"required", "voice":"Samantha", "rate":220, "agent":"name"}
Voices: Samantha (default), Daniel, Karen, Rishi, Alex, Victoria
Ports: 37779 (HTTP) | 1883 (MQTT default, configurable)

Key files:
  src-tauri/src/lib.rs    - Main app, Tauri commands
  src-tauri/src/http.rs   - HTTP server
  src-tauri/src/mqtt.rs   - MQTT client
  src-tauri/src/tray.rs   - Tray icon, voice queue

Build: bun tauri dev | bun tauri build
```

## Features

- **Dual Protocol** - HTTP API + MQTT subscriber for maximum flexibility
- **Voice Queue** - Messages queued and spoken one at a time (no overlap)
- **Timeline UI** - Click tray icon to see voice history with timestamps
- **Settings UI** - Configure MQTT broker, port, topics, and authentication
- **Live Status** - Tray icon shows connection state (connected/disconnected)
- **Per-Agent Voices** - Different voices for different agents (via hook scripts)

## Installation

```bash
# Build the app
bun install
bun tauri build

# Copy to Applications
cp -r "src-tauri/target/release/bundle/macos/Oracle Voice Tray.app" /Applications/

# Launch (runs in menu bar)
open "/Applications/Oracle Voice Tray.app"
```

## Usage

### HTTP API

**POST /speak** - Queue a voice message
```bash
curl -X POST http://127.0.0.1:37779/speak \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello world","voice":"Samantha","agent":"Main"}'
```

**GET /timeline** - Get all voice entries
```bash
curl http://127.0.0.1:37779/timeline
```

**GET /status** - Get current status
```bash
curl http://127.0.0.1:37779/status
```

Response:
```json
{
  "total": 5,
  "queued": 0,
  "is_speaking": false,
  "mqtt_status": "connected",
  "mqtt_broker": "127.0.0.1:1883"
}
```

### MQTT

Subscribe to configurable topics (default: `voice/speak`). Requires an MQTT broker like [Mosquitto](https://mosquitto.org/).

```bash
# Install mosquitto (macOS)
brew install mosquitto
brew services start mosquitto

# Send a voice message
mosquitto_pub -t voice/speak \
  -m '{"text":"Hello from MQTT!","agent":"my-agent"}'
```

Configure broker, port, topics, and authentication in the tray app settings (click tray icon → Settings).

### Payload Schema

```json
{
  "text": "Hello!",        // required
  "voice": "Samantha",     // optional (default: Samantha)
  "rate": 220,             // optional (words per minute, default: 220)
  "agent": "my-agent"      // optional (shows in timeline)
}
```

## Hook Integration

### HTTP Hook

Use `scripts/voice-tray-notify.sh` as a Claude Code hook:

```json
{
  "hooks": {
    "SubagentStop": [{
      "type": "command",
      "command": "/path/to/voice-tray-v2/scripts/voice-tray-notify.sh"
    }]
  }
}
```

### MQTT Hook

Use `scripts/voice-tray-mqtt-notify.sh` for MQTT-based notifications:

```bash
# Usage: voice-tray-mqtt-notify.sh "message" [voice] [agent] [rate]
./scripts/voice-tray-mqtt-notify.sh "Task completed" "Daniel" "Agent-1" 220
```

## Voice Configuration

The HTTP hook script (`voice-tray-notify.sh`) reads voice settings from `scripts/agent-voices.toml`:

```toml
[voices]
main = "Samantha"
agent_1 = "Daniel"
agent_2 = "Karen"
agent_3 = "Rishi"
default = "Samantha"

[rate]
main = 190
agent_1 = 220
default = 220
```

List available macOS voices:
```bash
say -v '?'
```

## Architecture

```
Claude Code Hook
      │
      ├── voice-tray-notify.sh ──► HTTP POST /speak
      │                                    │
      └── voice-tray-mqtt-notify.sh ──► MQTT publish
                                           │
                              ┌────────────┴────────────┐
                              │   Oracle Voice Tray     │
                              │   (Tauri macOS App)     │
                              │                         │
                              │  ┌─────────┐ ┌───────┐  │
                              │  │ HTTP    │ │ MQTT  │  │
                              │  │ :37779  │ │Client │  │
                              │  └────┬────┘ └───┬───┘  │
                              │       └─────┬────┘      │
                              │         Voice Queue     │
                              │             │           │
                              │      macOS say -v       │
                              │             │           │
                              │       Timeline UI       │
                              └─────────────────────────┘
```

## Development

```bash
# Run in dev mode with hot reload
bun tauri dev

# Build release
bun tauri build
```

## Requirements

- macOS 10.15+
- For MQTT: Mosquitto or compatible MQTT broker

## License

MIT
