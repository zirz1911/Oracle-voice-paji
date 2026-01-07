# Mission 01: Oracle Voice Tray Setup

> **Squad Team First Mission**: Get Oracle Voice Tray running on your machine

---

## Project History (From Oracle Memory)

This project evolved through several iterations, with bugs fixed and lessons learned along the way:

### Timeline

| Date | Version | Event |
|------|---------|-------|
| 2025-12-30 | v0.2.5 | **Oracle Status Tray v1** - First version, showed Oracle MCP status |
| 2025-12-30 | v0.2.6 | **Double Window Bug Fixed** - Root cause: dual tray initialization (config + code) |
| 2026-01-02 | v0.2.0 | **Voice Tray v2** - Rewritten with MQTT + HTTP, voice queue |
| 2026-01-03 | - | **DMG Polish** - Fixed Applications folder icon (symlink → alias) |
| 2026-01-05 | v0.2.0 | **Bug Fixes** - MQTT clean_session infinite loop, voice hook improvements |
| 2026-01-07 | v0.2.1 | **DMG Polish Script** - Automated professional DMG creation |

### Key Bugs We Solved (Learn From Our Pain)

#### 1. Double Window Bug (Tauri 2.0)
```
Problem: Two windows appear - correct popup + mystery white square
Root Cause: trayIcon defined in BOTH tauri.conf.json AND Rust code
Fix: Only define tray in code, remove from config
Lesson: Tauri 2.0 creates duplicate if both config + code define tray
```

#### 2. MQTT Infinite Loop
```
Problem: App speaks same message infinitely
Root Cause: clean_session=false + persistent subscription
Fix: Set clean_session=true in MQTT client
Lesson: Retained messages + persistent session = infinite replay
```

#### 3. DMG Broken Applications Icon
```
Problem: Applications folder shows empty/broken icon
Root Cause: Symlinks don't inherit icons, only aliases do
Fix: Create alias via AppleScript + apply icon with fileicon
Lesson: "Symlinks don't inherit icons. Aliases do."
```

---

## Project Structure

```
oracle-voice-tray/
├── src-tauri/                    # Rust backend (Tauri 2.0)
│   ├── src/
│   │   ├── main.rs              # Entry point
│   │   ├── lib.rs               # Main app logic, Tauri commands
│   │   ├── http.rs              # Axum HTTP server (:37779)
│   │   ├── mqtt.rs              # rumqttc MQTT client
│   │   ├── tray.rs              # Tray icon, voice queue, macOS say
│   │   ├── state.rs             # Shared state (timeline, icons)
│   │   └── config.rs            # MQTT config persistence
│   ├── icons/                   # Tray icons (idle, speaking, disconnected)
│   ├── Cargo.toml               # Rust dependencies
│   └── tauri.conf.json          # Tauri config
│
├── src/                          # Frontend (WebView)
│   ├── index.html               # Timeline + Settings UI
│   └── main.js                  # Frontend logic
│
├── scripts/
│   ├── voice-tray-notify.sh     # Claude Code hook (HTTP)
│   ├── voice-tray-mqtt-notify.sh # Claude Code hook (MQTT)
│   ├── agent-voices.toml        # Voice mapping per agent
│   └── polish-dmg.sh            # DMG post-build polish
│
├── docs/
│   ├── MISSION-01-SETUP.md      # This file
│   └── blog-dmg-polish.md       # Blog post about DMG fix
│
└── README.md                    # Main documentation
```

---

## How It Works

```
┌─────────────────────────────────────────────────────────────┐
│                    Claude Code                               │
│                        │                                     │
│         ┌──────────────┼──────────────┐                     │
│         ▼              ▼              ▼                     │
│    SessionStart   SubagentStop      Stop                    │
│         │              │              │                     │
│         └──────────────┴──────────────┘                     │
│                        │                                     │
│              voice-tray-notify.sh                           │
│                        │                                     │
│         ┌──────────────┴──────────────┐                     │
│         ▼                             ▼                     │
│   HTTP POST :37779              MQTT publish                │
└─────────┬─────────────────────────────┬─────────────────────┘
          │                             │
          ▼                             ▼
┌─────────────────────────────────────────────────────────────┐
│                 Oracle Voice Tray                            │
│  ┌─────────────┐              ┌─────────────┐               │
│  │ HTTP Server │              │ MQTT Client │               │
│  │   :37779    │              │  :1883      │               │
│  └──────┬──────┘              └──────┬──────┘               │
│         └────────────┬───────────────┘                      │
│                      ▼                                       │
│              ┌─────────────┐                                │
│              │ Voice Queue │                                │
│              └──────┬──────┘                                │
│                     ▼                                        │
│              ┌─────────────┐                                │
│              │ macOS `say` │  ← Samantha, Daniel, Karen...  │
│              └──────┬──────┘                                │
│                     ▼                                        │
│              ┌─────────────┐                                │
│              │ Timeline UI │  ← Click tray to see           │
│              └─────────────┘                                │
└─────────────────────────────────────────────────────────────┘
```

---

## Prompt for AI Assistant

Copy this prompt to Claude Code or any AI assistant:

---

```
I want to set up Oracle Voice Tray on my machine. This is a Tauri 2.0 menu bar app that provides text-to-speech for Claude Code agents.

Repository: https://github.com/Soul-Brews-Studio/oracle-voice-tray

Please help me:
1. Clone the repository
2. Install prerequisites (Rust, Node.js, platform-specific dependencies)
3. Build and run the app
4. Test the voice functionality
5. Integrate with my Claude Code setup

My operating system is: [macOS / Windows / Linux]

Guide me step by step and verify each step works before moving to the next.
```

---

## Prerequisites by Platform

### macOS

```bash
# 1. Install Homebrew (if not installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Install Node.js (LTS)
brew install node

# 4. Install fileicon (for DMG polish - optional)
brew install fileicon

# 5. Voice works out of the box (uses `say` command)
say "Hello world"  # Test it
```

### Windows

```powershell
# 1. Install Rust
# Download from: https://rustup.rs

# 2. Install Node.js
# Download from: https://nodejs.org

# 3. Install Visual Studio Build Tools
# Download from: https://visualstudio.microsoft.com/visual-cpp-build-tools/
# Select "Desktop development with C++"

# 4. Voice uses Windows SAPI (built-in)
# Note: Code changes needed for Windows TTS (currently macOS only)
```

### Linux (Ubuntu/Debian)

```bash
# 1. Install system dependencies
sudo apt update
sudo apt install -y build-essential curl wget file \
    libssl-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libwebkit2gtk-4.1-dev espeak

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Install Node.js
curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
sudo apt install -y nodejs

# 4. Voice uses espeak (code changes needed)
sudo apt install espeak
espeak "Hello world"  # Test it
```

---

## Quick Start

```bash
# Clone
git clone https://github.com/Soul-Brews-Studio/oracle-voice-tray.git
cd oracle-voice-tray

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

---

## Testing Guide

### HTTP Testing (No Extra Setup)

HTTP works immediately after starting the app.

```bash
# 1. Check if app is running
curl http://127.0.0.1:37779/status
# Expected: {"total":0,"queued":0,"is_speaking":false,...}

# 2. Send a voice message
curl -X POST http://127.0.0.1:37779/speak \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello from HTTP!","voice":"Samantha","agent":"Test"}'
# Expected: {"success":true,"id":1}

# 3. Check timeline
curl http://127.0.0.1:37779/timeline
# Shows all voice entries

# 4. Try different voices (macOS)
curl -X POST http://127.0.0.1:37779/speak \
  -d '{"text":"I am Daniel","voice":"Daniel"}'

curl -X POST http://127.0.0.1:37779/speak \
  -d '{"text":"I am Karen","voice":"Karen"}'
```

### MQTT Testing (Requires Mosquitto)

#### Prerequisites
```bash
# macOS
brew install mosquitto
brew services start mosquitto

# Ubuntu/Debian
sudo apt install mosquitto mosquitto-clients
sudo systemctl start mosquitto

# Windows
# Download from: https://mosquitto.org/download/
```

#### Test Commands
```bash
# 1. Configure Voice Tray
# Click tray icon → Settings
# Set Broker: 127.0.0.1, Port: 1883
# Save and check status shows "connected"

# 2. Send voice via MQTT
mosquitto_pub -h 127.0.0.1 -t "voice/speak" \
  -m '{"text":"Hello from MQTT!","agent":"MQTT-Test"}'

# 3. Monitor MQTT traffic (debug)
mosquitto_sub -h 127.0.0.1 -t "voice/#" -v

# 4. Test with different topics
mosquitto_pub -t "voice/speak" \
  -m '{"text":"Custom topic works!","voice":"Daniel"}'
```

#### MQTT Payload Schema
```json
{
  "text": "Required message",
  "voice": "Samantha",    // Optional: Samantha, Daniel, Karen, Alex, Victoria
  "rate": 220,            // Optional: Words per minute (150-300)
  "agent": "agent-name"   // Optional: Shows in timeline
}
```

---

## Success Criteria

You've completed the mission when:

- [ ] App appears in system tray/menu bar
- [ ] Clicking tray icon shows popup window
- [ ] "Test Voice" button speaks "Hello! Voice Tray is working."
- [ ] HTTP endpoint responds: `curl http://127.0.0.1:37779/health`
- [ ] Can send voice via curl:
  ```bash
  curl -X POST http://127.0.0.1:37779/speak \
    -H "Content-Type: application/json" \
    -d '{"text":"Mission complete!","agent":"Squad"}'
  ```

---

## Bonus Challenges

### Level 1: Custom Voice
- Change the default voice in the code
- macOS: Try "Samantha", "Alex", "Zarvox", "Daniel"
- Test different speech rates (150-300)
- Edit `scripts/agent-voices.toml`

### Level 2: Claude Code Integration
- Set up the voice notification hook in `~/.claude/settings.json`
- Make Claude Code announce when tasks complete
- Test with: start Claude Code, run a task, hear announcement

### Level 3: MQTT Connection
- Install Mosquitto broker: `brew install mosquitto`
- Start broker: `brew services start mosquitto`
- Connect Voice Tray to MQTT (Settings → MQTT)
- Send voice via MQTT:
  ```bash
  mosquitto_pub -t voice/speak -m '{"text":"Hello MQTT!","agent":"test"}'
  ```

### Level 4: DMG Polish (macOS only)
- Run `./scripts/polish-dmg.sh` after build
- Understand why symlinks vs aliases matter
- Read `docs/blog-dmg-polish.md`
- Create your own polished DMG

### Level 5: Cross-Platform Support
- **Challenge**: Make voice work on Windows or Linux
- Windows: Use Windows SAPI instead of `say`
- Linux: Use `espeak` or `festival`
- Submit a PR with your changes!

---

## Troubleshooting

### "tauri: command not found"
```bash
npm install  # Install dependencies first
```

### Build fails on Linux
```bash
# Install ALL webkit dependencies
sudo apt install libwebkit2gtk-4.1-dev
```

### No sound on Linux
```bash
# Install and test espeak
sudo apt install espeak
espeak "Hello world"
```

### Port 37779 already in use
```bash
# Find and kill existing process
lsof -i :37779
kill -9 <PID>
```

### Double window appears (macOS)
```
This is the bug we fixed! Check that tauri.conf.json does NOT have
a "trayIcon" section. Tray should only be defined in Rust code.
```

### MQTT keeps replaying messages
```
Set clean_session=true in mqtt.rs to avoid retained message replay.
```

---

## Report Your Success

After completing, share in the group:
1. Screenshot of the app running
2. Your OS and any issues you encountered
3. One thing you learned
4. (Bonus) Blog post about your experience

---

## Resources

- [Tauri 2.0 Prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Rust Installation](https://rustup.rs)
- [Project README](../README.md)
- [DMG Polish Blog Post](./blog-dmg-polish.md)

---

## Oracle Wisdom

Key lessons from our development journey:

> "Symlinks don't inherit icons. Aliases do." - DMG Polish Session

> "If tray is defined in both config AND code, you get two trays." - Double Window Bug

> "clean_session=false + retained messages = infinite loop" - MQTT Bug

---

**Mission Difficulty**: ⭐⭐ (Beginner-Intermediate)
**Estimated Time**: 30-60 minutes
**Skills Gained**: Rust toolchain, Tauri 2.0, Cross-platform development, System tray apps

---

*This mission document is part of the "Level Up with AI" program.*
*Created with Claude Code + Oracle Memory System.*
