"""Check release source for changes without treating Finder metadata as source."""
from pathlib import Path
import subprocess
import sys


STATUS_ARGS = ("status", "--porcelain", "-z", "--untracked-files=all")


def is_finder_metadata(path):
    return path.name == ".DS_Store" and not path.is_symlink() and path.is_file()


def has_source_changes(root, status):
    """Keep the clean-tree guard read-only. Only untracked regular .DS_Store files are incidental."""
    for entry in status.split("\0"):
        if not entry:
            continue
        if not entry.startswith("?? "):
            return True
        if not is_finder_metadata(root / entry[3:]):
            return True
    return False


if __name__ == "__main__":
    root = Path(sys.argv[1])
    command = ["git", "-C", str(root)]
    scope = ["--", *sys.argv[2:]]
    status = subprocess.check_output([*command, *STATUS_ARGS, *scope], text=True, errors="surrogateescape")
    if has_source_changes(root, status):
        subprocess.run([*command, "status", "--porcelain", *scope], check=True)
