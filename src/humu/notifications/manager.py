from __future__ import annotations

import logging

from humu.notifications.base import Notification, NotificationProvider

logger = logging.getLogger(__name__)


class NotificationManager:
    def __init__(self) -> None:
        self._providers: dict[str, NotificationProvider] = {}
        self._enabled: set[str] = set()

    def register(self, name: str, provider: NotificationProvider) -> None:
        self._providers[name] = provider

    def enable(self, name: str) -> None:
        self._enabled.add(name)

    def disable(self, name: str) -> None:
        self._enabled.discard(name)

    async def notify(self, notification: Notification) -> None:
        for name in self._enabled:
            provider = self._providers.get(name)
            if provider:
                try:
                    await provider.send(notification)
                except Exception:
                    logger.exception("Notification provider '%s' failed", name)
