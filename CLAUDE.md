# Project Instructions

## Project Context

Oracle Voice Tray is a Tauri 2.0 macOS menu bar app that provides centralized text-to-speech for Claude Code agents.

- **Tech**: Tauri 2.0 (Rust + WebView), Axum HTTP, rumqttc MQTT
- **Port**: HTTP server on 37779
- **Voice**: macOS `say` command

## Development

```bash
npm run tauri dev    # Dev mode
npm run tauri build  # Release build
```

See README.md for full documentation.
