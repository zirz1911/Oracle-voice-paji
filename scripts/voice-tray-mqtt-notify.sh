#!/bin/bash
# Voice Tray v2 MQTT Notification Hook
# Usage: voice-tray-mqtt-notify.sh "message" [voice] [agent] [rate]

MESSAGE="${1:-}"
VOICE="${2:-Samantha}"
AGENT="${3:-}"
RATE="${4:-220}"

if [ -z "$MESSAGE" ]; then
    echo "Usage: $0 \"message\" [voice] [agent] [rate]"
    exit 1
fi

# Escape JSON special characters
MESSAGE_ESCAPED=$(echo "$MESSAGE" | jq -Rs '.')

# Build JSON payload
if [ -n "$AGENT" ]; then
    PAYLOAD="{\"text\":${MESSAGE_ESCAPED},\"voice\":\"$VOICE\",\"agent\":\"$AGENT\",\"rate\":$RATE}"
else
    PAYLOAD="{\"text\":${MESSAGE_ESCAPED},\"voice\":\"$VOICE\",\"rate\":$RATE}"
fi

# Send via MQTT
mosquitto_pub -t voice/speak -m "$PAYLOAD"
