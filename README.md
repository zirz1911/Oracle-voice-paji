# Voice Tray

Central voice notification system for Claude Code. A Tauri app that receives voice requests via HTTP, queues them, speaks using macOS `say`, and displays a timeline UI.

## Features

- **HTTP API** - Hooks can POST voice requests to `http://127.0.0.1:37779/speak`
- **Voice Queue** - Messages are queued and spoken one at a time (no overlap)
- **Timeline UI** - Click tray icon to see voice history with timestamps
- **Per-Agent Voices** - Different voices for different agents (configurable via TOML)

## Installation

```bash
# Build the app
npm install
npm run tauri build

# Copy to Applications
cp -r src-tauri/target/release/bundle/macos/voice-tray.app /Applications/

# Launch (runs in menu bar)
open /Applications/voice-tray.app
```

## HTTP API

### POST /speak
Queue a voice message.

```bash
curl -X POST http://127.0.0.1:37779/speak \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello world","voice":"Samantha","agent":"Main"}'
```

Response:
```json
{"id":1,"status":"queued"}
```

### GET /timeline
Get all voice entries.

```bash
curl http://127.0.0.1:37779/timeline
```

### GET /status
Get current status.

```bash
curl http://127.0.0.1:37779/status
```

Response:
```json
{"total":5,"queued":0,"is_speaking":false}
```

## Hook Integration

Use `scripts/voice-tray-notify.sh` as a Claude Code hook:

```json
{
  "hooks": {
    "SubagentStop": [{
      "hooks": [{
        "type": "command",
        "command": "/path/to/voice-tray/scripts/voice-tray-notify.sh"
      }]
    }]
  }
}
```

## Voice Configuration

Edit `scripts/agent-voices.toml`:

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

List available voices:
```bash
say -v '?'
```

## Architecture

```
Claude Code Hook
      |
voice-tray-notify.sh
      | POST /speak
Voice Tray App (Tauri)
      |
macOS say -v [voice]
      |
Timeline UI (WebView)
```

## Development

```bash
npm run tauri dev
```

## License

MIT
