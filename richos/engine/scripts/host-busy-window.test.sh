#!/usr/bin/env bash
# host-busy-window.test.sh — the busy-window sampler answers with sane numbers.
#
# Two seconds of sampling: the shares add up to 100, nothing is negative, and a
# second with a known burst of process creation counts at least that burst.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

OUT="$(python3 "$HERE/host-busy-window.py" 2)" || bad "the sampler exits 0"
if printf '%s' "$OUT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["seconds"] >= 1.9, d
assert abs(d["user_percent"] + d["system_percent"] + d["idle_percent"] - 100) < 0.5, d
assert all(d[k] >= 0 for k in ("busy_percent", "user_percent", "system_percent", "idle_percent",
                               "processes_created_per_second")), d
assert abs(d["busy_percent"] - d["user_percent"] - d["system_percent"]) < 0.2, d
'; then
    ok "two seconds of sampling: the shares add up and nothing is negative"
else
    bad "two seconds of sampling: the shares add up and nothing is negative ($OUT)"
fi

# 40 short processes inside the window must be counted.
OUT="$(python3 - "$HERE/host-busy-window.py" <<'PY'
import subprocess, sys, time
p = subprocess.Popen([sys.executable, sys.argv[1], "2"], stdout=subprocess.PIPE, text=True)
time.sleep(0.5)
for _ in range(40):
    subprocess.run(["/usr/bin/true"])
print(p.communicate()[0].strip())
PY
)"
if printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["processes_created_per_second"] * d["seconds"] >= 40, d'; then
    ok "a burst of 40 processes inside the window is counted"
else
    bad "a burst of 40 processes inside the window is counted ($OUT)"
fi

printf '  %s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
