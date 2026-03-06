import importlib
import subprocess
import sys
import time

RELOAD_SENTINEL = "reload"


def _is_server_running() -> bool:
    """Check if the Humu server socket exists and is connectable."""
    from humu.config import HUMU_HOME

    sock_path = HUMU_HOME / "humu.sock"
    if not sock_path.exists():
        return False
    # Quick connect test
    import socket

    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(str(sock_path))
        s.close()
        return True
    except (ConnectionRefusedError, FileNotFoundError, OSError):
        # Stale socket — clean up
        try:
            sock_path.unlink()
        except OSError:
            pass
        return False


def _start_server_daemon() -> None:
    """Spawn ``humu serve`` as a background daemon process."""
    subprocess.Popen(
        [sys.executable, "-m", "humu", "serve"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    # Wait for socket to appear
    from humu.config import HUMU_HOME

    sock_path = HUMU_HOME / "humu.sock"
    for _ in range(50):  # 5 seconds max
        if sock_path.exists():
            return
        time.sleep(0.1)


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "serve":
        # Run as server
        from humu.server.server import run_server

        run_server()
        return

    # Client mode — ensure server is running
    if not _is_server_running():
        _start_server_daemon()

    while True:
        from humu.tui.app import HumuApp

        app = HumuApp()
        result = app.run()
        if result != RELOAD_SENTINEL:
            break
        # Reload all humu modules so code changes take effect
        to_reload = [name for name in sys.modules if name.startswith("humu.")]
        for name in to_reload:
            try:
                importlib.reload(sys.modules[name])
            except Exception:
                pass


if __name__ == "__main__":
    main()
