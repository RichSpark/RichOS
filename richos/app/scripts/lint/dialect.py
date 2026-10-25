"""The dialect rule, on the engine's own fixer rather than a copy of it.

Counts every non-American spelling the engine's Write hook (dialect-guard.sh) would fix or
refuse in a file's text: the same module (richos/engine/scripts/lib/dialect/dialect-fix.py),
the same table, the same file classes and exemptions. Until 2026-10-24 this ran the hook under
`bash -x` and parsed its trace and refusal banner; the hook now fixes most words instead of
refusing them, so the rule imports the module the hook runs.

What the hook takes from its jurisdiction is taken the same way here: the entity's
orchestration.config is sourced by bash (DIALECT_TARGET, DIALECT_SCAN_ALLOWLIST,
DIALECT_EXEMPT_PATHS), a locale other than en-US refuses (an unevaluated scan is never
clean), and a file recorded as third-party in `.richos/vendored-material` counts nothing,
asked through scripts/lib/vendored-material.sh only when the file has a finding.
"""
import importlib.util
import subprocess
import threading
from common import Refusal

RULES = {"dialect": "blocking"}

# THE VOCABULARY STAYS THE 173-WORD DICTIONARY UNTIL STREAM C, on purpose. The hook reads the
# generated 3,802-pair table; over this rule's inventory that table counts 149 findings
# (measured 2026-10-24) against the committed ceiling of 124, and ratchet.compare refuses a
# raised ceiling against main, so moving the vocabulary here would refuse every land. The
# plan puts the re-derived baseline in stream C, beside the inventory extension to ui/,
# web-app and the phone apps; that change is this one line. The mechanism (file classes,
# exemptions, scanner) is already the hook's own. dialect-table.test.py proves every line of
# this dictionary is in the generated table, so nothing the lint finds is unknown to the hook.
LINT_TABLE = "scripts/lib/dialect-en-US.dict"

_LOCK = threading.Lock()
_FIXERS = {}
_CONFIGS = {}

_CONFIG_SCRIPT = """
set -a
. "$1" >/dev/null 2>&1
printf '%s\\n%s\\n%s\\n' "${DIALECT_TARGET:-}" "${DIALECT_SCAN_ALLOWLIST:-}" "${DIALECT_EXEMPT_PATHS:-}"
"""

_OWNER_SCRIPT = """
. "$1"
rc=0
vm_load_upward "$(dirname "$2")" || rc=$?
case "$rc" in 0) ;; 1) exit 1 ;; *) exit 3 ;; esac
rel="${2#"$VM_ROOT"/}"
[ "$rel" != "$2" ] || exit 1
vm_is_third_party "$rel" && exit 0
exit 1
"""


def _fixer(engine):
    key = str(engine)
    with _LOCK:
        if key not in _FIXERS:
            path = engine / "scripts/lib/dialect/dialect-fix.py"
            spec = importlib.util.spec_from_file_location("dialect_fix", path)
            if spec is None or spec.loader is None:
                raise Refusal(f"dialect fixer missing: {path}")
            module = importlib.util.module_from_spec(spec)
            try:
                spec.loader.exec_module(module)
            except (OSError, SyntaxError) as exc:
                raise Refusal(f"dialect fixer unavailable: {exc}") from exc
            table = module.load_table(str(engine / LINT_TABLE))
            if not table:
                raise Refusal("dialect table yielded no rules")
            _FIXERS[key] = (module, table)
        return _FIXERS[key]


def _config(entity):
    key = str(entity)
    with _LOCK:
        if key not in _CONFIGS:
            result = subprocess.run(["bash", "-s", "--", str(entity / "orchestration.config")],
                                    input=_CONFIG_SCRIPT, capture_output=True, text=True, timeout=30)
            lines = (result.stdout.split("\n") + ["", "", ""])[:3]
            _CONFIGS[key] = (lines[0], lines[1].split(), lines[2].split())
        return _CONFIGS[key]


def _third_party(engine, file_path):
    result = subprocess.run(["bash", "-s", "--", str(engine / "scripts/lib/vendored-material.sh"), str(file_path)],
                            input=_OWNER_SCRIPT, capture_output=True, text=True, timeout=30)
    if result.returncode == 3:
        raise Refusal(f"vendored-material registry unreadable for {file_path}")
    return result.returncode == 0


def scan(root, path, text, *, entity=None):
    engine = root / "richos/engine"
    target, allowlist, exempt = _config(entity or engine)
    if target != "en-US":
        raise Refusal(f"dialect scan did not evaluate {path}: DIALECT_TARGET is {target or 'blank'}")
    module, table = _fixer(engine)
    labels = module.findings_for_file(str(root / path), text, table, allowlist,
                                      segment=str(path), extra_exempt=exempt)
    if labels and _third_party(engine, root / path):
        return []
    return labels
