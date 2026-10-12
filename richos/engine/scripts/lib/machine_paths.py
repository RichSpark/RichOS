"""Known literal machine paths and reviewed, line-local text exceptions."""
import re
from collections import Counter

PLACEHOLDERS = frozenset({'you', 'example', 'x', 'jordan', 'user', 'someone'})
PATTERN = re.compile(r'/Users/([A-Za-z0-9_.-]+)(?=/)|/Volumes/([A-Za-z0-9_.-]+)(?![A-Za-z0-9_.-])|(?<![\w])~/ab(?=/)|\$HOME/ab(?![\w.-])|\$\{HOME\}/ab(?![\w.-])')
MARKER = re.compile(r'machine-path-exempt:\s*(\S[^\r\n]*)')
CODE = ('.py', '.sh', '.rs', '.js', '.mjs', '.ts', '.swift', '.kt', '.css', '.toml', '.config')


def matches(text, path=''):
    """Counts by actual matched prefix. Exceptions never apply to binary blobs."""
    hits = Counter()
    previous = ''
    for line in text.splitlines():
        exempt = MARKER.search(line) or MARKER.search(previous)
        comment = path.endswith(CODE) and line.lstrip().startswith(('#', '//', '/*', '*'))
        for match in PATTERN.finditer(line):
            if match.group(1) in PLACEHOLDERS:
                continue
            if comment and match.group(2) is None:
                continue
            if not exempt:
                hits[match.group(0)] += 1
        previous = line
    return hits


def in_scope(path):
    return (not path.startswith('docs/') or path.startswith('docs/development/')) and not path.startswith('richos/app/validation/')
