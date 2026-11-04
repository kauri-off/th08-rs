"""Local paths: process env first, then packaging/local/local.env (git-ignored KEY=VALUE lines)."""
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def _env_file():
    # A worktree has no local.env (gitignored): use the main checkout's.
    f = os.path.join(ROOT, "packaging", "local", "local.env")
    try:
        gd = open(os.path.join(ROOT, ".git")).read().strip()
    except OSError:
        return f
    if not os.path.exists(f) and gd.startswith("gitdir:") and "/.git/worktrees/" in gd:
        f = os.path.join(gd[7:].strip().split("/.git/worktrees/")[0], "packaging", "local", "local.env")
    return f


def get(key):
    if os.environ.get(key):
        return os.environ[key]
    try:
        for line in open(_env_file(), encoding="utf-8"):
            k, _, v = line.strip().partition("=")
            if k == key and v:
                return v
    except OSError:
        pass
    raise SystemExit(f"{key} is not set (env or packaging/local/local.env)")


def game(name=""):
    return os.path.join(get("TH08_GAME_SRC"), name)
