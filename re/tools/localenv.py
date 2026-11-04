"""Local paths: process env first, then packaging/local/local.env (git-ignored KEY=VALUE lines)."""
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def get(key):
    if os.environ.get(key):
        return os.environ[key]
    try:
        for line in open(os.path.join(ROOT, "packaging", "local", "local.env"), encoding="utf-8"):
            k, _, v = line.strip().partition("=")
            if k == key and v:
                return v
    except OSError:
        pass
    raise SystemExit(f"{key} is not set (env or packaging/local/local.env)")


def game(name=""):
    return os.path.join(get("TH08_GAME_SRC"), name)
