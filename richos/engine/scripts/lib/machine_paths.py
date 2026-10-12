"""Known literal machine paths and reviewed, line-local text exceptions."""
import ast
import re
from pathlib import PurePosixPath
from collections import Counter

PLACEHOLDERS = frozenset({'you', 'example', 'x', 'jordan', 'user', 'someone', 'Shared'})
# machine-path-exempt: the checker must declare the literal path patterns it rejects
PATTERN = re.compile(r'/Users/([A-Za-z0-9_.-]+)(?![A-Za-z0-9_.-])|(?<!/System)/Volumes/([A-Za-z0-9_.-]+)(?![A-Za-z0-9_.-])|(?<![\w])~/ab(?![A-Za-z0-9_.-])|\$HOME/ab(?![\w.-])|\$\{HOME\}/ab(?![\w.-])')
MARKER = re.compile(r'machine-path-exempt:\s*(\S[^\r\n]*)')
CODE = ('.py', '.sh', '.rs', '.js', '.mjs', '.ts', '.swift', '.kt', '.css', '.toml', '.config')


def matches(text, path=''):
    """Counts by actual matched prefix. Exceptions never apply to binary blobs."""
    text = runtime_text(text, path)
    hits = Counter()
    previous = ''
    for line in text.splitlines():
        exempt = MARKER.search(line) or MARKER.search(previous)
        stripped = line.lstrip()
        comment = path.endswith(CODE) and (stripped.startswith(('#', '//')) or
                   (stripped.startswith('/*') and (
                       '*/' not in stripped or not stripped.partition('*/')[2].strip())))
        comment_end = line.find('*/') + 2 if path.endswith(CODE) and stripped.startswith('/*') else 0
        for match in PATTERN.finditer(line):
            if match.group(1) in PLACEHOLDERS:
                continue
            if (comment or (comment_end > 1 and match.end() <= comment_end)):
                continue
            if not exempt:
                hits[match.group(0)] += 1
        previous = line
    return hits


def runtime_text(text, path):
    """Remove inert Python prose and source comments, keeping assigned path strings."""
    if path.endswith('.py'):
        try:
            tree = ast.parse(text)
        except SyntaxError:
            return text  # malformed source is never an exemption
        lines = text.splitlines(keepends=True)
        for node in reversed(list(ast.walk(tree))):
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
                for row in range(node.lineno - 1, node.end_lineno):
                    raw = lines[row].encode('utf-8')
                    start = node.col_offset if row == node.lineno - 1 else 0
                    end = node.end_col_offset if row == node.end_lineno - 1 else len(raw.rstrip(b'\r\n'))
                    lines[row] = (raw[:start] + b' ' * (end - start) + raw[end:]).decode('utf-8')
        return ''.join(lines)
    if path.endswith(('.css', '.js', '.mjs', '.ts', '.swift', '.kt')):
        return re.sub(r'/\*.*?\*/', lambda m: '\n' * m.group().count('\n'), text, flags=re.S)
    return text


def in_scope(path):
    parts = PurePosixPath(path).parts
    name = parts[-1] if parts else ''
    if name in {'proof-inputs.json', 'verification-dependencies.json', 'verification-input-qualifications.json'}:
        return False  # development proof identities, never customer storage defaults
    # Rust has its existing lexer-based compile-time check. Test fixtures and
    # archived prose do not prescribe paths on a customer's computer.
    if name.endswith('.rs') or 'docs' in parts or 'validation' in parts:
        return False
    if any(p.lower() in {'test', 'tests', 'testvm', 'operator-probes', 'fixtures', 'examples', 'audits'} for p in parts):
        return False
    if '.test.' in name or '.mutation.' in name or name.endswith(('.md', '.patch', '.log')):
        return '/skills/' in path or '/agents/' in path
    return True
