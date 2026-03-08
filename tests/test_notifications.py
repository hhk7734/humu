import pytest
from unittest.mock import AsyncMock

from humu.notifications.base import Notification, NotificationProvider
from humu.notifications.manager import NotificationManager


@pytest.mark.asyncio
async def test_notification_manager_sends_to_enabled():
    mock_provider = AsyncMock(spec=NotificationProvider)
    manager = NotificationManager()
    manager.register("test", mock_provider)
    manager.enable("test")

    notification = Notification(
        title="Test",
        body="Hello",
        room="dev",
        workspace="ws",
        severity="info",
    )
    await manager.notify(notification)
    mock_provider.send.assert_called_once_with(notification)


@pytest.mark.asyncio
async def test_notification_manager_skips_disabled():
    mock_provider = AsyncMock(spec=NotificationProvider)
    manager = NotificationManager()
    manager.register("test", mock_provider)
    # Not enabled

    notification = Notification(
        title="Test", body="Hi", room="dev", workspace="ws", severity="info"
    )
    await manager.notify(notification)
    mock_provider.send.assert_not_called()
