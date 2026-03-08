from __future__ import annotations

import sys

from humu.notifications.base import Notification


class SoundProvider:
    async def send(self, notification: Notification) -> None:
        sys.stdout.write("\a")
        sys.stdout.flush()
