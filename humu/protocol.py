"""Shared protocol definitions for client-server communication.

All messages are JSON dicts with a ``type`` field.
"""

from __future__ import annotations

# ---------------------------------------------------------------------------
# Client → Server commands
# ---------------------------------------------------------------------------

# submit_message:  workspace, room, text
# cancel_processing:  workspace, room
# create_workspace:  name, root_path
# delete_workspace:  name
# create_room:  workspace, room_name
# delete_room:  workspace, room_name
# invite_agent:  workspace, room, agent_name
# kick_agent:  workspace, room, agent_name
# create_agent:  agent (dict)
# compact:  workspace, room, instructions
# list_workspaces
# list_rooms:  workspace
# list_agents
# get_agent:  name
# get_chat_history:  workspace, room
# get_skills
# subscribe_room:  workspace, room
# unsubscribe_room:  workspace, room

# ---------------------------------------------------------------------------
# Server → Client events
# ---------------------------------------------------------------------------

# message_added:  workspace, room, sender, text, is_system, raw, steps
# stream_chunk:  workspace, room, sender, text
# processing_started:  workspace, room, sender
# processing_done:  workspace, room
# processing_cancelled:  workspace, room
# live_step:  workspace, room, step
# workspace_list:  workspaces (list of dicts)
# room_list:  workspace, rooms (list of dicts)
# agent_list:  agents (list of dicts)
# agent_info:  agent (dict or null)
# chat_history:  workspace, room, messages
# skills_list:  skills (list of dicts)
# queue_updated:  workspace, room, pending_count, pending_messages
# system_event:  workspace, room, agent, text
# error:  message, request_type (optional)
