"""Adapt the existing Write hook without copying its scanner or exemptions.

The hook historically returns zero on PARSEFAIL and undeclared locales. Shell
trace of its scanner-result assignment makes those cases distinguishable from
an evaluated CLEAN result. Trace stays in memory and is never reported as prose.
The existing hook's declared quotation/path/ownership exemptions remain intact.
"""
import json
import os
import re
from common import Refusal, run

RULES = {"dialect": "blocking"}


def scan(root, path, text, *, entity=None):
    engine = root / "richos/engine"
    env = os.environ.copy()
    env.update(RICHOS_ENGINE_ROOT=str(engine), RICHOS_ENTITY_ROOT=str(entity or engine), PS4="+ ")
    payload = {"tool_name": "Write", "cwd": str(entity or engine),
               "tool_input": {"file_path": str(root / path), "content": text}}
    result, _ = run(["bash", "-x", engine / "scripts/hooks/dialect-guard.sh"],
                    cwd=root, env=env, input=json.dumps(payload), timeout=30)
    verdict = re.search(r"^\+ RESULT=['\"]?(CLEAN|FOUND|REPORTONLY|PARSEFAIL|NODICT)\b", result.stderr, re.M)
    if not verdict or verdict[1] in {"PARSEFAIL", "NODICT"}:
        raise Refusal(f"dialect scan did not evaluate {path}")
    if result.returncode == 0:
        # FOUND with exit zero means the hook established third-party ownership.
        return []
    if result.returncode != 2 or "\n=== Dialect check BLOCKED ===\n" not in result.stderr:
        raise Refusal(f"dialect hook failed for {path}")
    labels = re.findall(r"^    - (.+)$", result.stderr, re.M)
    if not labels:
        raise Refusal(f"dialect hook refused without findings for {path}")
    return labels
