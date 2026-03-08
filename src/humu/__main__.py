import sys


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "serve":
        from humu.server.app import run_server
        run_server()
    else:
        from humu.client.app import run_client
        run_client()


if __name__ == "__main__":
    main()
