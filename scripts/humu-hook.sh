#!/bin/bash
# Claude Code hook script for humu integration.
# Merges workspace/room into the hook JSON as flat top-level fields.
#
# Install: Add to ~/.claude/settings.json hooks configuration
# Requires: jq, socat

if [ -n "$HUMU_SOCKET" ] && command -v socat &> /dev/null && command -v jq &> /dev/null; then
  INPUT=$(cat)
  echo "$INPUT" | jq -c \
    --arg ws "$HUMU_WORKSPACE" \
    --arg rm "$HUMU_ROOM" \
    --arg ht "$CLAUDE_HOOK_TYPE" \
    '. + {workspace: $ws, room: $rm, hook_type: $ht}' \
    | socat - UNIX-CONNECT:"$HUMU_SOCKET" 2>/dev/null || true
fi
