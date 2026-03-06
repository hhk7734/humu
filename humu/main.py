import importlib
import sys

RELOAD_SENTINEL = "reload"


def main() -> None:
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
