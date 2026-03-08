from humu.protocol import (
    ClientMessage,
    ServerMessage,
    UserMessageCmd,
    SubscribeRoomCmd,
    StreamChunkEvent,
    RoomStateSyncEvent,
)


def test_user_message_serialization():
    msg = UserMessageCmd(workspace="ws", room="dev", text="hello")
    data = msg.model_dump()
    assert data["type"] == "user_message"
    parsed = ClientMessage.parse(data)
    assert isinstance(parsed, UserMessageCmd)


def test_stream_chunk_serialization():
    event = StreamChunkEvent(workspace="ws", room="dev", sender="leader", text="hi")
    data = event.model_dump()
    assert data["type"] == "stream_chunk"


def test_subscribe_room():
    cmd = SubscribeRoomCmd(workspace="ws", room="dev")
    data = cmd.model_dump()
    assert data["type"] == "subscribe_room"
