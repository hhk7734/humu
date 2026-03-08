from __future__ import annotations

import logging
from typing import AsyncIterator

from humu.db.repositories import Repository
from humu.engine.room_graph import RoomEngine
from humu.engine.state import RoomState
from humu.protocol import ServerMessage
from humu.server.ws import WebSocketManager

logger = logging.getLogger(__name__)


class RoomExecutor:
    def __init__(
        self,
        repo: Repository,
        engine: RoomEngine,
        ws_manager: WebSocketManager,
    ) -> None:
        self._repo = repo
        self._engine = engine
        self._ws_manager = ws_manager

    async def execute(
        self, workspace: str, room_name: str, user_message: str
    ) -> AsyncIterator[dict]:
        room = await self._repo.get_room(workspace, room_name)
        if not room:
            yield ServerMessage.error(f"Room '{room_name}' not found")
            return

        leader = await self._repo.get_agent(workspace, room_name, room.leader)
        if not leader:
            yield ServerMessage.error(f"Leader '{room.leader}' not found")
            return

        # Save user message
        await self._repo.append_message(
            workspace, room_name, {"sender": "user", "text": user_message}
        )
        yield ServerMessage.message_added(workspace, room_name, "user", user_message)

        # Build agent configs
        agent_configs = {}
        for agent in await self._repo.list_agents(workspace, room_name):
            if agent.name != room.leader:
                agent_configs[agent.name] = agent.model_dump()

        # Build initial state
        state = RoomState(
            workspace=workspace,
            room=room_name,
            user_message=user_message,
            leader_config=leader.model_dump(),
            agent_configs=agent_configs,
        )

        # Build and compile graph
        graph = self._engine.build_graph(state)
        compiled = graph.compile()

        # Execute
        yield ServerMessage.agent_status(
            workspace, room_name, room.leader, "started"
        )

        try:
            result = await compiled.ainvoke(state.model_dump())
            final_state = RoomState.model_validate(result)

            if final_state.final_response:
                await self._repo.append_message(
                    workspace,
                    room_name,
                    {"sender": room.leader, "text": final_state.final_response},
                )
                yield ServerMessage.message_added(
                    workspace, room_name, room.leader, final_state.final_response
                )

            # Emit all accumulated events
            for event in final_state.events:
                if event.get("type") == "agent_done":
                    agent_name = event["agent"]
                    await self._repo.append_message(
                        workspace,
                        room_name,
                        {"sender": agent_name, "text": event["text"]},
                    )
                    yield ServerMessage.message_added(
                        workspace, room_name, agent_name, event["text"]
                    )

            yield ServerMessage.agent_status(
                workspace, room_name, room.leader, "completed"
            )

        except Exception as e:
            logger.exception("Room execution failed")
            yield ServerMessage.agent_status(
                workspace, room_name, room.leader, "error", error=str(e)
            )
            yield ServerMessage.error(str(e))
