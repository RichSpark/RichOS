#!/usr/bin/env bash
#
# guard-headed-browser.test.sh: the rule that keeps browser automation on the host headless
# (scripts/hooks/guard-headed-browser.sh: a module of dispatch-pretooluse.sh on Bash, registered
# directly on mcp__* tools; the rule is scripts/lib/headed_browser.py).
#
# 2026-10-28: a Playwright script with `chromium.launch({ headless: false })`, run on the CEO's Mac,
# put a Chrome window in front of him. Nothing refused it. ceo-decisions §65: the test VM is the
# screen.
#
#   P1   POSITIVE CONTROL, the 2026-10-28 shape: a script file with headless: false, run by node;
#        refused, and the refusal says what to do instead (headless, or the test VM)
#   P2   flags: --headed on playwright test / playwright-cli open / pytest, --headless=false
#   P3   Playwright commands that always open a window: open, codegen, show-trace, show-report,
#        test --ui, test --debug, PWDEBUG=1, python -m playwright codegen
#   P4   inline and heredoc code: node -e, python -c (headless=False), a heredoc fed to node,
#        a heredoc written to a file and run in the same command, Selenium with no headless
#   P5   files the command runs: python3 x.py, npx tsx x.ts, `cd dir && node x.mjs`,
#        playwright test with a playwright.config that sets headless: false, npm test and
#        npm run <script> followed into package.json
#   P6   a browser binary with no --headless; open -a "Google Chrome" --args ...
#   P7   MCP: a Playwright / Chrome DevTools server configured without --headless, and one whose
#        configuration cannot be found, are refused
#   N1   headless runs pass: a script with headless: true, a script with no option, Chrome with
#        --headless=new, playwright test with a headless config, npm test of a headless script
#   N2   a mention is not a launch: echo, grep, a commit message, a comment, a heredoc to cat
#        with no interpreter in the command, a commented-out headless: false in a script
#   N3   the test VM: ssh <vm> 'npx playwright test --headed', a testvm/ script
#   N4   MCP: --headless servers, an attach-only server, a non-browser server, Claude in Chrome
#   N5   an unreadable payload and a command naming nothing of this pass
#   N7   only /usr/bin/python3 on PATH: the hook passes instead of running Apple's stub
#   P8   2026-10-29: the full Chrome app with --headless but without the notification switches
#        (--disable-features=NativeNotifications,SystemNotifications in the LAST such switch,
#        --disable-notifications) or on a real profile; code that starts it (Playwright channel
#        chromium, Puppeteer's launch, Selenium's Chrome); playwright --channel chromium,
#        playwright-cli --browser=chromium; MCP servers that launch it (Puppeteer, --browser chromium)
#   P9   review 2026-10-29: version-qualified Playwright packages (npx playwright@latest codegen)
#   P10  review 2026-10-29: PWDEBUG / HEADED set behind env (env PWDEBUG=1 npx playwright test)
#   P11  review 2026-10-29: a spec file named to playwright test is read (headless: false in it)
#   P12  review 2026-10-30: with no spec named, the specs playwright test discovers under the
#        config's testDir are read (node_modules skipped), and its name filters narrow them
#   N6   the quiet full app (headless, both switches, a $TMPDIR profile), chrome-headless-shell,
#        Puppeteer headless: 'shell', Playwright's default Chromium, playwright-cli with a config
#        that carries the switches, npx playwright-cli@latest open (headless by default)
#   D1   through the real dispatcher: Bash refused, and with only this rule named (the app lease)
#
# Usage: scripts/hooks/guard-headed-browser.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/guard-headed-browser.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -8; FAIL=$((FAIL + 1)); return 0; }

[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 2; }

FX="$(mktemp -d "${TMPDIR:-/tmp}/headed-browser-test.XXXXXX")"
trap 'rm -rf "$FX"' EXIT
FX="$(cd "$FX" && pwd -P)"
mkdir -p "$FX/home" "$FX/proj/sub" "$FX/proj-headless" "$FX/proj-mcp" "$FX/proj-specs/tests/e2e" "$FX/proj-specs/node_modules/pkg"
unset CLAUDE_CONFIG_DIR

# The 2026-10-28 shape: a review script that launches a visible Chromium.
cat > "$FX/proj/review.mjs" <<'EOF'
import { chromium } from 'playwright';
const browser = await chromium.launch({ headless: false });
const page = await browser.newPage();
await page.goto('http://localhost:4173/avatars');
EOF
cat > "$FX/proj/check.py" <<'EOF'
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    browser = p.chromium.launch(headless=False)
EOF
cat > "$FX/proj/sub/walk.ts" <<'EOF'
const b = await chromium.launch({ "headless": false, slowMo: 50 });
EOF
cat > "$FX/proj/sub/review.mjs" <<'EOF'
const browser = await chromium.launch({ headless: false });
EOF
cat > "$FX/proj/selenium_check.py" <<'EOF'
from selenium import webdriver
driver = webdriver.Chrome()
EOF
cat > "$FX/proj/playwright.config.ts" <<'EOF'
export default { use: { headless: false, viewport: { width: 1280, height: 720 } } };
EOF
cat > "$FX/proj/package.json" <<'EOF'
{ "scripts": { "test": "node review.mjs", "shots": "node review.mjs --shots", "lint": "eslint ." } }
EOF
cat > "$FX/proj-headless/shot.mjs" <<'EOF'
import { chromium } from 'playwright';
// headless: false would put a window on the screen; never on the host.
const browser = await chromium.launch({ headless: true });
EOF
# A project whose specs Playwright discovers itself (testDir below), one of them headed.
cat > "$FX/proj-specs/playwright.config.ts" <<'EOF'
export default { testDir: './tests', use: { headless: true } };
EOF
cat > "$FX/proj-specs/tests/e2e/headed.spec.js" <<'EOF'
const { test, chromium } = require('@playwright/test');
test('walk', async () => { const b = await chromium.launch({ headless: false }); });
EOF
cat > "$FX/proj-specs/tests/quiet.spec.js" <<'EOF'
const { test } = require('@playwright/test');
test('walk', async ({ page }) => { await page.goto('about:blank'); });
EOF
cat > "$FX/proj-specs/node_modules/pkg/vendored.spec.js" <<'EOF'
const b = await chromium.launch({ headless: false });
EOF
cat > "$FX/proj-specs/package.json" <<'EOF'
{ "scripts": { "test": "playwright test" } }
EOF
cp "$FX/proj-specs/tests/quiet.spec.js" "$FX/proj-headless/quiet.spec.js"
cat > "$FX/proj-headless/plain.mjs" <<'EOF'
const browser = await chromium.launch();
EOF
cat > "$FX/proj-headless/selenium_ok.py" <<'EOF'
from selenium import webdriver
opts = webdriver.ChromeOptions(); opts.add_argument("--headless=new")
opts.add_argument("--disable-features=NativeNotifications,SystemNotifications")
opts.add_argument("--disable-notifications")
driver = webdriver.Chrome(options=opts)
EOF
cat > "$FX/proj/selenium_cft.py" <<'EOF'
from selenium import webdriver
opts = webdriver.ChromeOptions(); opts.add_argument("--headless=new")
driver = webdriver.Chrome(options=opts)
EOF
cat > "$FX/proj/pptr.mjs" <<'EOF'
import puppeteer from 'puppeteer';
const browser = await puppeteer.launch();
EOF
cat > "$FX/proj-headless/pptr-shell.mjs" <<'EOF'
import puppeteer from 'puppeteer';
const browser = await puppeteer.launch({ headless: 'shell' });
EOF
cat > "$FX/proj/cft.mjs" <<'EOF'
import { chromium } from 'playwright';
const browser = await chromium.launch({ channel: 'chromium' });
EOF
cat > "$FX/proj-headless/quiet-cli.json" <<'EOF'
{ "browser": { "launchOptions": { "args": ["--disable-features=NativeNotifications,SystemNotifications", "--disable-notifications"] } } }
EOF
cat > "$FX/proj-headless/playwright.config.ts" <<'EOF'
export default { use: { headless: true } };
EOF
cat > "$FX/proj-headless/package.json" <<'EOF'
{ "scripts": { "test": "node shot.mjs" } }
EOF
cat > "$FX/proj-mcp/.mcp.json" <<'EOF'
{ "mcpServers": {
    "playwright": { "command": "npx", "args": ["@playwright/mcp@latest"] },
    "pw-headless": { "command": "npx", "args": ["@playwright/mcp@latest", "--headless"] },
    "devtools": { "command": "npx", "args": ["chrome-devtools-mcp@latest"] },
    "devtools-attach": { "command": "npx", "args": ["chrome-devtools-mcp@latest", "--browserUrl", "http://127.0.0.1:9222"] },
    "pw-cft": { "command": "npx", "args": ["@playwright/mcp@latest", "--headless", "--browser", "chromium"] },
    "tracker": { "command": "npx", "args": ["some-issue-tracker-mcp"] }
} }
EOF
cat > "$FX/home/.claude.json" <<'EOF'
{ "mcpServers": { "puppeteer": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-puppeteer"],
                                 "env": { "PUPPETEER_LAUNCH_OPTIONS": "{\"headless\": \"shell\"}" } },
                   "puppeteer-cft": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-puppeteer"],
                                 "env": { "PUPPETEER_LAUNCH_OPTIONS": "{\"headless\": true}" } } } }
EOF

payload() { # <command> [cwd] [tool]
    python3 -c 'import json,sys; print(json.dumps({"tool_name":sys.argv[3],"tool_input":{"command":sys.argv[1]},"cwd":sys.argv[2],"session_id":"test"}))' \
        "$1" "${2:-$FX/proj}" "${3:-Bash}"
}
check() { # <expected rc> <id> <description> <command> [cwd] [tool]
    local out rc
    out="$(payload "$4" "${5:-$FX/proj}" "${6:-Bash}" | HOME="$FX/home" bash "$HOOK" 2>&1)"; rc=$?
    if [ "$rc" = "$1" ]; then ok "$2 $3"; else bad "$2 $3" "rc=$rc (expected $1) $out"; fi
    LAST_OUT="$out"
}

echo "=== guard-headed-browser: refused ==="
# shellcheck disable=SC2016  # the $-words are the literal command text under test
{
check 2 P1 "the 2026-10-28 shape: node runs a script with headless: false" 'node review.mjs'
for want in "Nothing ran" "review.mjs line 2" "headless" "test VM"; do
    if printf '%s' "$LAST_OUT" | grep -qF "$want"; then ok "P1 the refusal carries '$want'"; else bad "P1 the refusal does not carry '$want'" "$LAST_OUT"; fi
done
check 2 P1 'the same script by absolute path from elsewhere' "node $FX/proj/review.mjs" /tmp
check 2 P2 'playwright test --headed' 'npx playwright test --headed'
check 2 P2 'playwright-cli open --headed (the bundled skill used to say this)' 'playwright-cli open --headed --browser=chrome'
check 2 P2 'pytest --headed' 'python3 -m pytest tests/ui --headed'
check 2 P2 '--headless=false' 'node shot.mjs --headless=false' "$FX/proj-headless"
check 2 P3 'playwright open' 'npx playwright open http://localhost:4173'
check 2 P3 'playwright codegen' 'pnpm exec playwright codegen localhost:3000'
check 2 P3 'playwright show-trace' 'npx playwright show-trace trace.zip'
check 2 P3 'playwright show-report' 'npx playwright show-report'
check 2 P3 'playwright test --ui' 'npx playwright test --ui' "$FX/proj-headless"
check 2 P3 'playwright test --debug' 'npx playwright test --debug login.spec.ts' "$FX/proj-headless"
check 2 P3 'PWDEBUG=1' 'PWDEBUG=1 npx playwright test' "$FX/proj-headless"
check 2 P10 'env PWDEBUG=1 (the setting behind env)' 'env PWDEBUG=1 npx playwright test' "$FX/proj-headless"
check 2 P10 'env -u X PWDEBUG=console behind nohup' 'nohup env -u CI PWDEBUG=console npx playwright test' "$FX/proj-headless"
check 2 P10 'env HEADED=1 node' 'env HEADED=1 node plain.mjs' "$FX/proj-headless"
check 2 P3 'python -m playwright codegen' 'python3 -m playwright codegen example.com'
check 2 P4 'node -e with headless: false' "node -e 'const b = await chromium.launch({headless: false})'"
check 2 P4 'python -c with headless=False' "python3 -c 'p.chromium.launch(headless=False)'"
check 2 P4 'a heredoc fed to node' "node --input-type=module <<'EOF'
import { chromium } from 'playwright';
await chromium.launch({ headless: false });
EOF"
check 2 P4 'a heredoc written and run in one command' "cat > /tmp/x.mjs <<'EOF'
await chromium.launch({ headless: false });
EOF
node /tmp/x.mjs"
check 2 P4 'Selenium with no headless option' 'python3 selenium_check.py'
check 2 P5 'python3 runs a script with headless=False' 'python3 check.py'
check 2 P5 'npx tsx runs a script with "headless": false' 'npx tsx sub/walk.ts'
check 2 P5 'cd into a folder, then node' 'cd sub && node review.mjs'
check 2 P5 'playwright test with a headed config' 'npx playwright test'
check 2 P11 'playwright test naming a spec that launches headed' 'npx playwright test tests/e2e/headed.spec.js' "$FX/proj-specs"
check 2 P11 'playwright test with the spec after options' 'npx playwright test --project=chromium ./tests/e2e/headed.spec.js' "$FX/proj-specs"
check 2 P12 'plain playwright test discovers the headed spec under testDir' 'npx playwright test' "$FX/proj-specs"
check 2 P12 'npm test running playwright test' 'npm test' "$FX/proj-specs"
check 2 P12 'a name filter that selects the headed spec' 'npx playwright test e2e' "$FX/proj-specs"
check 2 P5 'playwright test --config naming a headed config' "npx playwright test --config $FX/proj/playwright.config.ts" "$FX/proj-headless"
check 2 P5 'npm test into package.json' 'npm test'
check 2 P5 'npm run <script> into package.json' 'npm run shots'
check 2 P6 'Chrome binary with no --headless' '"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --remote-debugging-port=9222 about:blank'
check 2 P6 'chromium with no --headless' 'chromium --user-data-dir=/tmp/p http://localhost:3000'
check 2 P6 'open -a Chrome --args' 'open -na "Google Chrome" --args --remote-debugging-port=9222'
check 2 P6 'behind nohup and &' 'nohup node review.mjs > /tmp/log 2>&1 &'
check 2 P7 'MCP: Playwright server without --headless' '' "$FX/proj-mcp" mcp__playwright__browser_navigate
if printf '%s' "$LAST_OUT" | grep -qF -- "--headless"; then ok "P7 the MCP refusal says to add --headless"; else bad "P7 the MCP refusal does not say to add --headless" "$LAST_OUT"; fi
check 2 P7 'MCP: Chrome DevTools server without --headless' '' "$FX/proj-mcp" mcp__devtools__new_page
check 2 P7 'MCP: a browser server whose configuration is not found' '' "$FX/proj" mcp__playwright__browser_snapshot
check 2 P8 'the 2026-10-29 shape: Chrome for Testing with only --headless=new' '"/x/chromium-1234/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" --headless=new --user-data-dir=/tmp/p --remote-debugging-port=0'
for want in "Notification Center" "SystemNotifications" "Nothing ran"; do
    if printf '%s' "$LAST_OUT" | grep -qF "$want"; then ok "P8 the refusal carries '$want'"; else bad "P8 the refusal does not carry '$want'" "$LAST_OUT"; fi
done
check 2 P8 'an earlier --disable-features does not count (Chrome keeps the last)' '"/x/chromium-1234/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" --headless=new --user-data-dir=/tmp/p --disable-features=NativeNotifications,SystemNotifications --disable-notifications --disable-features=MediaRouter'
check 2 P8 'no --disable-notifications' 'chromium --headless=new --user-data-dir=/tmp/p --disable-features=NativeNotifications,SystemNotifications'
check 2 P8 'the user profile' '"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-features=NativeNotifications,SystemNotifications --disable-notifications --user-data-dir="$HOME/Library/Application Support/Google/Chrome"'
check 2 P8 'no --user-data-dir (the user real profile)' '"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-features=NativeNotifications,SystemNotifications --disable-notifications'
check 2 P8 'a script launching Playwright channel chromium' 'node cft.mjs'
check 2 P8 'node -e with channel chromium' "node -e \"await chromium.launch({channel: 'chromium'})\""
check 2 P8 'Puppeteer launch (Chrome for Testing, the full app)' 'node pptr.mjs'
check 2 P8 'Selenium Chrome, headless, without the switches' 'python3 selenium_cft.py'
check 2 P8 'playwright screenshot --channel chromium' 'npx playwright screenshot --channel chromium https://example.com /tmp/s.png'
check 2 P8 'playwright-cli open --browser=chromium' 'playwright-cli open --browser=chromium https://example.com'
check 2 P8 'MCP: Puppeteer headless true (Chrome for Testing)' '' "$FX/proj" mcp__puppeteer-cft__puppeteer_navigate
check 2 P8 'MCP: Playwright --headless --browser chromium' '' "$FX/proj-mcp" mcp__pw-cft__browser_navigate
check 2 P9 'npx playwright@latest codegen' 'npx playwright@latest codegen localhost:3000'
check 2 P9 'npx @playwright/test@latest test --ui' 'npx @playwright/test@latest test --ui' "$FX/proj-headless"
check 2 P9 'pnpm dlx playwright@1.49.0 open' 'pnpm dlx playwright@1.49.0 open http://localhost:4173'
}

echo "=== guard-headed-browser: allowed ==="
# shellcheck disable=SC2016
{
check 0 N1 'a script with headless: true (and a commented-out false)' 'node shot.mjs' "$FX/proj-headless"
check 0 N1 'a script with no option (Playwright launches headless)' 'node plain.mjs' "$FX/proj-headless"
check 0 N1 'Selenium with --headless=new' 'python3 selenium_ok.py' "$FX/proj-headless"
check 0 N1 'Chrome with --headless=new, the notification switches and a temp profile' '"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-features=NativeNotifications,SystemNotifications --disable-notifications --user-data-dir="$TMPDIR/p" --screenshot=/tmp/s.png http://localhost:3000'
check 0 N1 'playwright test with a headless config' 'npx playwright test' "$FX/proj-headless"
check 0 N1 'playwright test naming a spec with no launch, and a name filter' 'npx playwright test quiet.spec.js walk' "$FX/proj-headless"
check 0 N1 'playwright test naming only the quiet spec where a headed one exists' 'npx playwright test tests/quiet.spec.js' "$FX/proj-specs"
check 0 N1 'a name filter that selects only the quiet spec' 'npx playwright test quiet' "$FX/proj-specs"
check 0 N1 'env CI=1 playwright test (a setting behind env that is not debug)' 'env CI=1 npx playwright test' "$FX/proj-headless"
check 0 N1 'npm test of a headless script' 'npm test' "$FX/proj-headless"
check 0 N1 'npm run of a script with no browser' 'npm run lint'
check 0 N1 'playwright-cli open, headless' 'playwright-cli open https://example.com'
check 0 N1 'playwright install' 'npx playwright install chromium'
check 0 N1 'an ordinary python script' 'python3 -m pytest tests/unit -q'
check 0 N2 'echo naming the flag' 'echo "never pass --headed on the host"'
check 0 N2 'grep for headless: false' 'grep -rn "headless: false" scripts'
check 0 N2 'a commit message' "git commit -m 'guard: refuse headless: false and --headed on the host'"
check 0 N2 'a comment' 'ls # npx playwright test --headed'
check 0 N2 'a heredoc to cat, nothing runs it' "cat > notes.md <<'EOF'
chromium.launch({ headless: false }) opened a window on 2026-10-28.
EOF"
check 0 N2 'open of a URL in the user browser' 'open http://localhost:4173'
check 0 N3 'ssh to the guest' "ssh admin@vm 'cd app && npx playwright test --headed'"
check 0 N3 'a testvm script' 'richos/app/scripts/testvm/run.sh vm npx playwright test --headed'
check 0 N4 'MCP: Playwright server with --headless' '' "$FX/proj-mcp" mcp__pw-headless__browser_navigate
check 0 N4 'MCP: DevTools server attached to a running browser' '' "$FX/proj-mcp" mcp__devtools-attach__new_page
check 0 N4 'MCP: Puppeteer with headless launch options (Claude config)' '' "$FX/proj" mcp__puppeteer__puppeteer_navigate
check 0 N4 'MCP: a server that is not a browser' '' "$FX/proj-mcp" mcp__tracker__list_issues
check 0 N4 'MCP: Claude in Chrome (the user own browser)' '' "$FX/proj" mcp__claude-in-chrome__navigate
check 0 N5 'a command naming nothing of this' 'git status --short'
check 0 N6 'Chrome for Testing, quiet (both switches merged, $TMPDIR profile)' '"/x/chromium-1234/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" --headless=new --user-data-dir=/var/folders/xx/T/richos-nat-1/p --disable-features=MediaRouter,NativeNotifications,SystemNotifications --disable-notifications'
check 0 N6 'chrome-headless-shell (no app bundle)' '/x/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell --headless --screenshot about:blank'
check 0 N6 'Puppeteer on the headless shell' 'node pptr-shell.mjs' "$FX/proj-headless"
check 0 N6 'playwright screenshot on the default Chromium (headless shell)' 'npx playwright screenshot -b chromium https://example.com /tmp/s.png'
check 0 N6 'playwright-cli --browser=chromium with a config carrying the switches' 'playwright-cli open --browser=chromium --config=quiet-cli.json https://example.com' "$FX/proj-headless"
check 0 N6 'npx playwright-cli@latest open (headless by default)' 'npx playwright-cli@latest open https://example.com'
check 0 N6 'npx playwright@latest install' 'npx playwright@latest install chromium'
}

echo "=== unreadable payload ==="
out="$(printf 'not json' | bash "$HOOK" 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then ok "N5 an unreadable payload is not refused"; else bad "N5 an unreadable payload was refused" "rc=$rc $out"; fi

echo "=== never the /usr/bin python3 stub ==="
# On a Mac without developer tools /usr/bin/python3 is Apple's stub: running it opens the install
# dialog. With no other python3 on PATH the hook passes rather than run it (a refusal here would
# mean it ran /usr/bin/python3). RICHOS_TOOLCHAIN_ROOT is an empty fake Mac, so the developer
# tools this host may have are not found behind the stub (scripts/lib/hook-python.sh step 3).
NODEV="$(mktemp -d -t ghb-nodev.XXXXXX)"
out="$(payload 'node review.mjs' | RICHOS_TOOLCHAIN_ROOT="$NODEV" PATH=/usr/bin:/bin bash "$HOOK" 2>&1)"; rc=$?
rm -rf "$NODEV"
if [ "$rc" = 0 ]; then ok "N7 with only /usr/bin/python3 on PATH the hook passes and never runs it"; else bad "N7 the hook ran /usr/bin/python3" "rc=$rc $out"; fi

echo "=== through the dispatcher ==="
out="$(payload 'node review.mjs' | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash 2>&1)"; rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -q "guard-headed-browser"; then
    ok "D1 dispatch-pretooluse.sh Bash runs this module and returns its refusal"
else
    bad "D1 dispatch-pretooluse.sh Bash runs this module and returns its refusal" "rc=$rc $out"
fi
out="$(payload 'npx playwright test --headed' | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash guard-headed-browser.sh 2>&1)"; rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -q "guard-headed-browser"; then
    ok "D1 the dispatcher given this rule by name (the app lease's form) refuses"
else
    bad "D1 the dispatcher given this rule by name (the app lease's form) refuses" "rc=$rc $out"
fi
out="$(payload 'node shot.mjs' "$FX/proj-headless" | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash guard-headed-browser.sh 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then ok "D1 the dispatcher passes a headless run"; else bad "D1 the dispatcher passes a headless run" "rc=$rc $out"; fi

echo "=== registration ==="
HJ="$SCRIPT_DIR/../../hooks/hooks.json"
if python3 - "$HJ" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
ok = any(g.get("matcher") == "mcp__.*" and any("guard-headed-browser.sh" in h.get("command", "") for h in g.get("hooks", []))
         for g in doc["hooks"]["PreToolUse"])
sys.exit(0 if ok else 1)
PY
then ok "R1 hooks.json registers the rule on mcp__.* tools"; else bad "R1 hooks.json does not register the rule on mcp__.* tools"; fi

printf 'guard-headed-browser: %d passed, %d FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
