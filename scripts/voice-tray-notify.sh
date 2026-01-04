#!/bin/bash
# Voice Tray Notification Hook
# Posts to voice-tray HTTP server instead of calling say directly

set -euo pipefail

VOICE_TRAY_URL="http://127.0.0.1:37779"

# Get script directory for sourcing config
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOML_FILE="$SCRIPT_DIR/agent-voices.toml"

# Parse TOML value using yq (primary) or stoml (fallback)
get_toml() {
    local section="$1"
    local key="$2"
    local default="$3"

    if [ ! -f "$TOML_FILE" ]; then
        echo "$default"
        return
    fi

    local value=""
    if command -v yq &> /dev/null; then
        value=$(yq -p toml -oy ".${section}.${key}" "$TOML_FILE" 2>/dev/null | grep -v "null")
    elif command -v stoml &> /dev/null; then
        value=$(stoml "$TOML_FILE" "${section}.${key}" 2>/dev/null)
    fi

    echo "${value:-$default}"
}

# Get voice for agent
get_voice() {
    local agent_name="$1"
    case "$agent_name" in
        "Main") get_toml "voices" "main" "Samantha" ;;
        "Agent 1") get_toml "voices" "agent_1" "Daniel" ;;
        "Agent 2") get_toml "voices" "agent_2" "Karen" ;;
        "Agent 3") get_toml "voices" "agent_3" "Rishi" ;;
        "Subagent") get_toml "voices" "subagent" "Daniel" ;;
        *) get_toml "voices" "default" "Samantha" ;;
    esac
}

# Get speech rate for agent
get_rate() {
    local agent_name="$1"
    case "$agent_name" in
        "Main") get_toml "rate" "main" "190" ;;
        "Agent 1") get_toml "rate" "agent_1" "220" ;;
        "Agent 2") get_toml "rate" "agent_2" "220" ;;
        "Agent 3") get_toml "rate" "agent_3" "220" ;;
        "Subagent") get_toml "rate" "subagent" "220" ;;
        *) get_toml "rate" "default" "220" ;;
    esac
}

# Read JSON input from stdin
INPUT=$(cat)

# Parse relevant fields
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
HOOK_EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // "unknown"')
CWD=$(echo "$INPUT" | jq -r '.cwd // ""')
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // ""')
AGENT_TRANSCRIPT=$(echo "$INPUT" | jq -r '.agent_transcript_path // ""')

# Determine if this is a subagent (SubagentStop) or MAW agent (Stop)
IS_SUBAGENT=false
if [ "$HOOK_EVENT" = "SubagentStop" ]; then
    IS_SUBAGENT=true
fi

# Try to get last message from transcript
LAST_MESSAGE=""
TRANSCRIPT_TO_READ="$TRANSCRIPT_PATH"
if [ "$IS_SUBAGENT" = true ] && [ -n "$AGENT_TRANSCRIPT" ] && [ -f "$AGENT_TRANSCRIPT" ]; then
    TRANSCRIPT_TO_READ="$AGENT_TRANSCRIPT"
fi

if [ -n "$TRANSCRIPT_TO_READ" ] && [ -f "$TRANSCRIPT_TO_READ" ]; then
    LAST_MESSAGE=$(tail -20 "$TRANSCRIPT_TO_READ" | grep -o '"text":"[^"]*"' | tail -1 | sed 's/"text":"//;s/"$//' | head -c 100)
fi

# Determine agent identifier
AGENT_NAME=""
if [ "$IS_SUBAGENT" = true ]; then
    SLUG=""
    if [ -n "$AGENT_TRANSCRIPT" ] && [ -f "$AGENT_TRANSCRIPT" ]; then
        SLUG=$(head -1 "$AGENT_TRANSCRIPT" | grep -o '"slug":"[^"]*"' | sed 's/"slug":"//;s/"$//' | head -1)
    fi
    if [ -n "$SLUG" ]; then
        AGENT_NAME=$(echo "$SLUG" | tr '-' ' ')
    else
        AGENT_NAME="Subagent"
    fi
else
    if [[ "$CWD" =~ agents/([0-9]+) ]]; then
        AGENT_NAME="Agent ${BASH_REMATCH[1]}"
    else
        AGENT_NAME="Main"
    fi
fi

# Get voice and rate for this agent
VOICE=$(get_voice "$AGENT_NAME")
RATE=$(get_rate "$AGENT_NAME")

# Build message
if [ -n "$LAST_MESSAGE" ]; then
    MESSAGE="$AGENT_NAME says: $LAST_MESSAGE"
else
    MESSAGE="$AGENT_NAME completed"
fi

# Escape message for JSON
MESSAGE_ESCAPED=$(echo "$MESSAGE" | jq -Rs '.')

# Post to voice-tray (non-blocking, don't wait for response)
curl -s -X POST "$VOICE_TRAY_URL/speak" \
    -H "Content-Type: application/json" \
    -d "{\"text\":${MESSAGE_ESCAPED},\"voice\":\"$VOICE\",\"agent\":\"$AGENT_NAME\",\"rate\":$RATE}" \
    --connect-timeout 1 \
    --max-time 2 \
    > /dev/null 2>&1 &

# Output JSON for Claude Code
cat << EOF
{
  "decision": "approve",
  "reason": "$AGENT_NAME notification sent to voice-tray"
}
EOF

exit 0
