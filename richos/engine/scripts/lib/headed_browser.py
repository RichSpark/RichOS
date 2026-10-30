#!/usr/bin/env python3
"""headed_browser.py: browser automation on the host Mac runs headless. Refuse an agent's command or
MCP tool call that would open a visible browser window on the person's screen.

2026-10-28: a reviewer ran a Playwright script with `chromium.launch({ headless: false })` on the
CEO's Mac, and a Chrome window showing test pages appeared in front of him. Nothing stopped it.
ceo-decisions §65: nothing appears on the host's screen; the test VM is the screen. The app itself
enforces it, so no agent instruction is needed: a check that needs a real window runs in the VM.

TWO SURFACES, one rule.

BASH (`check`), wherever the command sits (after && ; | & newline, in ( ), { }, $( ), backticks,
sh -c scripts, behind sudo/env/nohup/exec/time/timeout/xargs), refuses:
  1. a headed flag on any command: --headed, --headless=false, --no-headless
  2. a Playwright command that always opens a window: open, codegen, cr, ff, wk, show-trace,
     show-report, and `test --ui` / `test --debug` (playwright, npx/pnpm/yarn/bunx playwright)
  3. PWDEBUG=<anything> or HEADED=1 in front of a command or behind `env` (Playwright's debug
     mode is headed)
  4. a browser binary started without --headless (Google Chrome, Chromium, Edge, Firefox, Brave),
     and `open -a <browser> --args ...` (an automation launch through Launch Services)
  5. inline code (node -e, python -c, deno eval, a heredoc fed to an interpreter) that sets
     `headless: false` / `headless=False`, or starts a Selenium driver with no headless option
  6. a script the command runs (node x.mjs, python3 x.py, npx tsx x.ts, ...) whose source sets
     `headless: false`, and `playwright test` whose playwright.config, or a spec file named on
     its command line, sets it; `npm test`,
     `npm run <script>`, `pnpm <script>`, `yarn <script>` are followed one level into
     package.json. Paths resolve from the payload's cwd and any `cd` earlier in the command.

MCP (`check-mcp`, PreToolUse on mcp__*): a tool of a browser-automation server (Playwright,
Puppeteer, Chrome DevTools, Selenium, browser-use) is refused unless the server's own launch
configuration makes it headless (`--headless` among its args, or a PUPPETEER_LAUNCH_OPTIONS /
HEADLESS setting) or it attaches to a browser already running (--cdp-endpoint, --browserUrl,
--wsEndpoint; launching that browser is a Bash call, judged above). All three common servers
open a visible window by default, so a server whose configuration cannot be found is refused.
The configuration is read from the payload cwd's .mcp.json and from Claude Code's own config
file (CLAUDE_CONFIG_DIR/.claude.json or ~/.claude.json: top level and the cwd's project). The
Claude in Chrome extension (mcp__claude-in-chrome__*) is the user's own browser, opened by the
user, and is not judged here.

HEADLESS IS NOT ENOUGH FOR THE FULL CHROME APP (2026-10-29). A suite started the full Chrome for
Testing app with --headless=new, and macOS put "Google Chrome for Testing" and "... Helper (Alerts)"
notification permission prompts on the CEO's screen: new-headless Chrome is the whole app bundle,
and its macOS notification bridge checks in with Notification Center at startup. So, on Bash:
  7. a Chromium-family binary (Chrome, Chrome for Testing, Chromium, Edge, Brave) started with
     --headless but without --disable-features=NativeNotifications,SystemNotifications (in the LAST
     --disable-features, the only one Chrome keeps), --disable-notifications, or a --user-data-dir
     that is not a real browser profile (Library/Application Support/...) is refused;
  8. code that starts the full app without those switches: Playwright with `channel: 'chromium'`
     (or a beta/dev/canary channel) or an executablePath to Chrome for Testing / Chromium,
     Puppeteer's launch unless `headless: 'shell'`, and Selenium's Chrome driver;
  9. `playwright screenshot|pdf --channel chromium` (and beta/dev/canary channels).
And on MCP: a launching Puppeteer server (Puppeteer starts Chrome for Testing) unless its launch
options use the headless shell or carry SystemNotifications, and a Playwright or DevTools server
given an executable path or a beta/dev/canary/chromium channel without that evidence.
Playwright's default Chromium (the headless shell, a plain binary with no app bundle) and the
user's own installed Chrome or Edge channel are not judged by this part. The same rule for suites
is richos/app/ui/tests/lib/quiet-browser.js.

What passes: headless runs (Playwright launches headless by default), `--headless` Chrome with the
notification switches and a throwaway profile, a mention (echo, grep, a commit message, a comment),
`open <url>` (the user's own browser), and anything sent to the test VM: a segment whose command
word is ssh/scp/sftp or a script under a `testvm/` path runs in the guest.

IT IS A TEXT MATCH AND LEAKS (a config value computed at run time, a script that imports the
launcher from another file, eval). Its job is to stop the habitual command. Any error in the
check is a pass. Suite: scripts/hooks/guard-headed-browser.test.sh.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from unguarded_rm import tokenize  # noqa: E402
from host_sleep import _strip_prefix, _unquote, _ENV_ASSIGN, SHELLS, REMOTE, \
    WRAPPERS, _OPTS_WITH_ARG, _POSITIONAL_VALUE  # noqa: E402

MAX_SOURCE_BYTES = 1_000_000

HEADED_FLAG = re.compile(r"^(?:--headed(?:=(?:true|1|yes))?|--headless=(?:false|0|no)|--no-headless)$", re.I)
PLAYWRIGHT_WINDOW_SUBCOMMANDS = {"open", "codegen", "cr", "ff", "wk", "show-trace", "show-report"}
PACKAGE_RUNNERS = {"npx", "pnpx", "bunx", "pnpm", "yarn", "bun", "npm"}
BROWSER_BINARIES = re.compile(
    r"^(?:google chrome(?: canary| beta| dev| for testing)?|chromium(?:-browser)?|chrome|"
    r"google-chrome(?:-stable|-beta|-unstable)?|microsoft edge|msedge|firefox(?:-bin)?|"
    r"brave browser|brave)$", re.I)
BROWSER_APPS = re.compile(r"chrome|chromium|firefox|edge|brave|safari|arc|vivaldi|opera", re.I)
INTERPRETERS = re.compile(r"^(?:node|nodejs|python[0-9.]*|deno|bun|tsx|ts-node|ruby)$")
INLINE_FLAGS = {"-c", "-e", "--eval", "-p", "--print", "eval"}
SCRIPT_EXT = re.compile(r"\.(?:js|mjs|cjs|ts|mts|cts|py)$")
# `headless: false`, `headless=False`, `"headless": false`, `headless: 'false'`, `headless = 0`.
HEADED_CODE = re.compile(r"""\bheadless['"]?\s*[:=]\s*['"]?(?:false|False|0|no)\b""")
SELENIUM_DRIVER = re.compile(r"\bwebdriver\.(?:Chrome|Firefox|Edge|Safari)\s*\(")
PLAYWRIGHT_CONFIGS = ("playwright.config.ts", "playwright.config.js", "playwright.config.mjs",
                      "playwright.config.cjs", "playwright.config.mts")
COMMENT_LINE = re.compile(r"^\s*(?://|#|\*|/\*)")

# --- the full Chrome app and macOS Notification Center (2026-10-29) ---------------------------
CHROMIUM_BINARIES = re.compile(
    r"^(?:google chrome(?: canary| beta| dev| for testing)?|chromium(?:-browser)?|chrome|"
    r"google-chrome(?:-stable|-beta|-unstable)?|microsoft edge|msedge|brave browser|brave)$", re.I)
NOTIFICATION_FEATURES = ("NativeNotifications", "SystemNotifications")
REAL_PROFILE = re.compile(r"Library/Application Support/|\.config/(?:google-chrome|chromium)", re.I)
# Channels that start a Chrome the user has likely never run: Chrome for Testing ("chromium") and
# the beta/dev/canary builds. The stable `chrome` / `msedge` channel is the user's own browser.
FULL_APP_CHANNEL = r"(?:chromium[\w-]*|chrome-(?:beta|dev|canary)|msedge-(?:beta|dev|canary))"
# Quotes are optional throughout: inline code reaches this check with its quotes removed.
CODE_FULL_CHANNEL = re.compile(r"""\bchannel['"]?\s*[:=]\s*['"]?%s(?![\w-])""" % FULL_APP_CHANNEL)
CODE_FULL_EXECUTABLE = re.compile(r"""executable_?[Pp]ath['"]?\s*[:=]\s*['"]?[^,)}\n'"]*(?:Chrome for Testing|Chromium)""")
PUPPETEER_IMPORT = re.compile(r"""(?:require\(\s*['"]?puppeteer['"]?\s*\)|from\s+['"]?puppeteer\b|import\s+puppeteer\b|\bpyppeteer\b)""")
PUPPETEER_SHELL = re.compile(r"""headless['"]?\s*:\s*['"]?shell\b""")
SELENIUM_CHROME = re.compile(r"""\bwebdriver\.Chrome\s*\(|forBrowser\(\s*['"]?chrome\b|\bChromeOptions\s*\(""")
NOTIFY_ADVICE = ("which registers the app with macOS Notification Center, and macOS asks the user to "
                 "allow its notifications. Use Playwright's default Chromium (the headless shell), or "
                 "give the full app --disable-features=NativeNotifications,SystemNotifications, "
                 "--disable-notifications and a --user-data-dir in $TMPDIR")


def _notification_problems(args):
    """What a headless Chromium-family launch with `args` lacks to stay away from Notification Center."""
    out = []
    features = None
    for a in args:
        if a.startswith("--disable-features="):
            features = a.split("=", 1)[1]
    missing = [f for f in NOTIFICATION_FEATURES if f not in (features or "").split(",")]
    if missing:
        out.append("starts the full browser app without --disable-features=%s (in its last "
                   "--disable-features, the only one Chrome keeps), %s" % (",".join(NOTIFICATION_FEATURES), NOTIFY_ADVICE))
    elif "--disable-notifications" not in args:
        out.append("starts the full browser app without --disable-notifications")
    profile = None
    for a in args:
        if a.startswith("--user-data-dir="):
            profile = a.split("=", 1)[1]
    if profile is None:
        out.append("starts the full browser app on the user's real profile (add --user-data-dir=<a new folder in $TMPDIR>)")
    elif REAL_PROFILE.search(profile):
        out.append("starts the full browser app on a real browser profile (%s); use a new folder in $TMPDIR" % profile)
    return out


def _config_has_notification_switches(cwd, args):
    """True when a --config file named among `args` (Playwright MCP / playwright-cli) carries the
    notification switches in its launch options."""
    for k, a in enumerate(args):
        path = a.split("=", 1)[1] if a.startswith("--config=") else \
            (args[k + 1] if a == "--config" and k + 1 < len(args) else None)
        if path:
            full = _resolve(cwd, _unquote(path))
            text = _read(full) if full else None
            if text and "SystemNotifications" in text and "NativeNotifications" in text:
                return True
    return False


def _full_app_code_findings(code, where):
    """Code that starts the full Chrome app without the notification switches."""
    if "SystemNotifications" in code:
        return []
    m = CODE_FULL_CHANNEL.search(code)
    if m:
        return ["%s launches `%s`, the full Chrome app, %s" % (where, m.group(0), NOTIFY_ADVICE)]
    m = CODE_FULL_EXECUTABLE.search(code)
    if m:
        return ["%s launches the full Chrome app by executablePath, %s" % (where, NOTIFY_ADVICE)]
    if PUPPETEER_IMPORT.search(code) and re.search(r"\.launch\s*\(", code) and not PUPPETEER_SHELL.search(code):
        return ["%s starts Puppeteer's browser, the full Chrome for Testing app unless `headless: 'shell'`, %s"
                % (where, NOTIFY_ADVICE)]
    if SELENIUM_CHROME.search(code):
        return ["%s starts Chrome through Selenium (the full app, or Chrome for Testing that Selenium "
                "Manager downloads), %s" % (where, NOTIFY_ADVICE)]
    return []


def _code_findings(text, where):
    """Reasons for source text that opens a visible window: a headless: false outside a comment,
    or a Selenium driver in code that never mentions headless; then code that starts the full
    Chrome app without the notification switches."""
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if COMMENT_LINE.match(line):
            continue
        if HEADED_CODE.search(line):
            out.append("%s line %d sets `%s`" % (where, n, HEADED_CODE.search(line).group(0)))
            break
    if not out and SELENIUM_DRIVER.search(text) and "headless" not in text:
        out.append("%s starts a Selenium browser with no headless option" % where)
    if not out:
        code = "\n".join(line for line in text.splitlines() if not COMMENT_LINE.match(line))
        out.extend(_full_app_code_findings(code, where))
    return out


def _read(path):
    try:
        if os.path.getsize(path) > MAX_SOURCE_BYTES:
            return None
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return None


def _resolve(cwd, path):
    path = os.path.expanduser(path)
    if not os.path.isabs(path):
        if not cwd:
            return None
        path = os.path.join(cwd, path)
    return os.path.normpath(path)


class _Ctx:
    def __init__(self, cwd):
        self.cwd = cwd
        self.findings = []
        self.seen_files = set()

    def add(self, reason):
        if reason not in self.findings:
            self.findings.append(reason)

    def scan_file(self, path, label):
        full = _resolve(self.cwd, path)
        if not full or full in self.seen_files or not os.path.isfile(full):
            return
        self.seen_files.add(full)
        text = _read(full)
        if text is not None:
            for reason in _code_findings(text, label or path):
                self.add(reason)


def _assignments(words):
    """The NAME=value settings the command runs with: leading ones and those after `env` and the
    other wrappers (review 2026-10-29: `env PWDEBUG=1 npx playwright test` escaped the debug check)."""
    out = {}
    i = 0
    while i < len(words):
        w = _unquote(words[i])
        if _ENV_ASSIGN.match(w):
            k, _, v = w.partition("=")
            out[k] = v
            i += 1
            continue
        base = os.path.basename(w)
        if base not in WRAPPERS:
            break
        i += 1
        opts = _OPTS_WITH_ARG.get(base, set())
        while i < len(words):
            t = _unquote(words[i])
            if base == "env" and _ENV_ASSIGN.match(t):
                k, _, v = t.partition("=")
                out[k] = v
                i += 1
                continue
            if t == "--":
                i += 1
                break
            if t.startswith("-") and len(t) > 1:
                i += 2 if (t in opts and "=" not in t) else 1
                continue
            break
        if base in _POSITIONAL_VALUE and i < len(words):
            i += 1
    return out


def _package_name(word):
    """A package word without its version: `playwright@latest` -> `playwright`,
    `@playwright/test@1.49.0` -> `@playwright/test` (review: `npx playwright@latest codegen` passed)."""
    return re.sub(r"(?<=.)@[^/@]*$", "", word)


def _playwright_tool(argv):
    """(tool, argv after it) for a Playwright CLI call, tool being `playwright` or `playwright-cli`;
    (None, None) otherwise."""
    name = _package_name(os.path.basename(argv[0]))
    if name in ("playwright", "playwright-cli"):
        return name, argv[1:]
    if name in PACKAGE_RUNNERS:
        rest = [a for a in argv[1:]]
        while rest and (rest[0].startswith("-") or rest[0] in ("exec", "dlx", "x")):
            rest = rest[1:]
        pkg = _package_name(rest[0]) if rest else ""
        if pkg in ("playwright", "@playwright/test"):
            return "playwright", rest[1:]
        if pkg in ("playwright-cli", "@playwright/cli"):
            return "playwright-cli", rest[1:]
    return None, None


def _playwright_args(argv):
    """argv after the `playwright` word, or None when this is not a Playwright CLI call."""
    return _playwright_tool(argv)[1]


def _package_script(ctx, argv, depth):
    """`npm test`, `npm run x`, `pnpm x`, `yarn x`: the script text from package.json, analyzed."""
    name = os.path.basename(argv[0])
    args = [a for a in argv[1:] if not a.startswith("-")]
    if not args:
        return
    script = None
    if name == "npm":
        if args[0] in ("test", "t", "start"):
            script = "test" if args[0] in ("test", "t") else "start"
        elif args[0] in ("run", "run-script") and len(args) > 1:
            script = args[1]
    elif name in ("pnpm", "yarn", "bun"):
        script = args[1] if args[0] == "run" and len(args) > 1 else args[0]
    if not script:
        return
    pkg = _resolve(ctx.cwd, "package.json")
    text = _read(pkg) if pkg else None
    if not text:
        return
    try:
        scripts = json.loads(text).get("scripts") or {}
    except (ValueError, AttributeError):
        return
    body = scripts.get(script)
    if isinstance(body, str):
        _scan_text(ctx, body, depth + 1)


def judge(ctx, words, depth):
    env = _assignments(words)
    argv = _strip_prefix(words)
    if not argv:
        return
    name = os.path.basename(argv[0])
    shown = " ".join(argv)[:120]
    if name == "cd":
        if len(argv) > 1 and not argv[1].startswith("-"):
            target = _resolve(ctx.cwd, argv[1])
            if target:
                ctx.cwd = target
        return
    if name in SHELLS:
        for k, a in enumerate(argv[1:], 1):
            if re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a) and k + 1 < len(argv):
                _scan_text(ctx, argv[k + 1], depth + 1)
                break
        return
    if name in REMOTE or re.search(r"(?:^|/)testvm/", argv[0]):
        return
    if any(k == "PWDEBUG" for k in env):
        ctx.add("`PWDEBUG=%s %s`: Playwright's debug mode opens a visible browser and inspector"
                % (env["PWDEBUG"], shown))
    if str(env.get("HEADED", "")).lower() in ("1", "true", "yes"):
        ctx.add("`HEADED=%s %s`: asks the script for a visible browser" % (env["HEADED"], shown))
    for a in argv[1:]:
        if HEADED_FLAG.match(a):
            ctx.add("`%s`: %s opens a visible browser window" % (shown, a))
            break
    tool, pw = _playwright_tool(argv)
    if pw is not None:
        positional = [a for a in pw if not a.startswith("-")]
        if positional and positional[0] in PLAYWRIGHT_WINDOW_SUBCOMMANDS and tool != "playwright-cli":
            ctx.add("`%s`: `playwright %s` always opens a window" % (shown, positional[0]))
        # `playwright screenshot --channel chromium`, `playwright-cli open --browser=chromium`: both
        # start the full Chrome for Testing app (Playwright's MCP config maps --browser chromium to
        # channel "chromium"). playwright-cli can carry the switches in its --config file.
        option = "--channel" if tool == "playwright" else "--browser"
        for k, a in enumerate(pw):
            value = a.split("=", 1)[1] if a.startswith(option + "=") else \
                (pw[k + 1] if a == option and k + 1 < len(pw) else None)
            if value and re.fullmatch(FULL_APP_CHANNEL, _unquote(value)) and \
                    not _config_has_notification_switches(ctx.cwd, pw):
                ctx.add("`%s`: %s %s starts the full Chrome app, %s" % (shown, option, value, NOTIFY_ADVICE))
                break
        if positional and positional[0] == "test":
            for flag in ("--ui", "--debug"):
                if flag in pw:
                    ctx.add("`%s`: `playwright test %s` opens a visible browser" % (shown, flag))
            config = None
            for k, a in enumerate(pw):
                if a in ("-c", "--config") and k + 1 < len(pw):
                    config = pw[k + 1]
                elif a.startswith("--config="):
                    config = a.split("=", 1)[1]
            if config:
                ctx.scan_file(config, config)
            else:
                for candidate in PLAYWRIGHT_CONFIGS:
                    ctx.scan_file(candidate, candidate)
            # A spec file named on the command line is read too (review 2026-10-29: `npx playwright
            # test headed.spec.js` ran a spec with headless: false). A positional that is not an
            # existing script file is a name filter, and scan_file skips it.
            for a in positional[1:]:
                if SCRIPT_EXT.search(a):
                    ctx.scan_file(a, a)
        return
    if BROWSER_BINARIES.match(name):
        if not any(a.startswith("--headless") and not HEADED_FLAG.match(a) for a in argv[1:]):
            ctx.add("`%s`: starts the browser with a window (add --headless)" % shown)
        elif CHROMIUM_BINARIES.match(name):
            for reason in _notification_problems([_unquote(a) for a in argv[1:]]):
                ctx.add("`%s`: %s" % (shown, reason))
        return
    if name == "open":
        args = argv[1:]
        for k, a in enumerate(args):
            if re.fullmatch(r"-[A-Za-z]*[ab]", a) and k + 1 < len(args) and BROWSER_APPS.search(args[k + 1]) \
                    and "--args" in args:
                ctx.add("`%s`: launches the browser for automation with a window" % shown)
                break
        return
    if name in PACKAGE_RUNNERS:
        _package_script(ctx, argv, depth)
        rest = [a for a in argv[1:] if not a.startswith("-")]
        if name in ("npx", "pnpx", "bunx") and rest and rest[0] in ("tsx", "ts-node", "node"):
            argv = rest
            name = rest[0]
        else:
            return
    if INTERPRETERS.match(name) or name in ("tsx", "ts-node"):
        args = argv[1:]
        if "-m" in args and args.index("-m") + 1 < len(args) and args[args.index("-m") + 1] == "playwright":
            judge(ctx, ["playwright"] + args[args.index("-m") + 2:], depth + 1)
            return
        for k, a in enumerate(args):
            if a in INLINE_FLAGS and k + 1 < len(args):
                for reason in _code_findings(args[k + 1], "the inline code of `%s %s`" % (name, a)):
                    ctx.add(reason)
                return
        for a in args:
            if a.startswith("-") or (name in ("deno", "bun") and a == "run"):
                continue
            if SCRIPT_EXT.search(a):
                ctx.scan_file(a, a)
            break


def _heredoc_bodies(text):
    """(command line, body) for each heredoc, so a heredoc fed to an interpreter is read as code."""
    out = []
    for m in re.finditer(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1[^\n]*\n", text):
        start = m.end()
        end = re.search(r"^\s*%s\s*$" % re.escape(m.group(2)), text[start:], re.M)
        body = text[start:start + end.start()] if end else text[start:]
        line_start = text.rfind("\n", 0, m.start()) + 1
        out.append((text[line_start:m.start()], body))
    return out


def _scan_text(ctx, text, depth=0):
    if depth > 6:
        return
    cmds, nested = tokenize(text)
    for words in cmds:
        judge(ctx, words, depth)
    for body in nested:
        _scan_text(ctx, body, depth + 1)
    # A heredoc is code when an interpreter reads it, or when the same command writes it to a
    # file and then runs an interpreter or Playwright (the file does not exist yet at check time).
    runs_code = any(_runs_code(_strip_prefix(words)) for words in cmds)
    for head, body in _heredoc_bodies(text):
        head_cmds, _ = tokenize(head)
        fed = [_strip_prefix(words) for words in head_cmds]
        if any(argv and os.path.basename(argv[0]) in REMOTE for argv in fed):
            continue
        if any(_runs_code(argv) for argv in fed) or runs_code:
            for reason in _code_findings(body, "the heredoc in this command"):
                ctx.add(reason)


def _runs_code(argv):
    if not argv:
        return False
    name = os.path.basename(argv[0])
    return bool(INTERPRETERS.match(name)) or name in ("tsx", "ts-node", "playwright") or \
        (name in PACKAGE_RUNNERS and len(argv) > 1)


def verdict(command, cwd=None):
    ctx = _Ctx(cwd)
    _scan_text(ctx, command)
    return ctx.findings


TRIGGER = re.compile(r"headless|headed|HEADED|PWDEBUG|playwright|puppeteer|webdriver|selenium|"
                     r"chrome|chromium|firefox|msedge|edge|brave|npm|pnpm|yarn|npx|bunx|node|"
                     r"python|tsx|ts-node|deno|bun", re.I)

ADVICE = """
Nothing an agent or a test runs shows anything on this Mac's screen: not a browser window, and not
a notification or a notification permission prompt. Browser automation here runs headless, and a
full Chrome app runs with its notification bridge off.

What to do instead:
  * Run it headless: drop --headed and `headless: false` (Playwright launches headless by default;
    Chrome takes --headless=new). Screenshots, traces and videos work the same.
  * Prefer Playwright's default Chromium (the headless shell: no app bundle, no notifications).
    A full Chrome, Chrome for Testing or Chromium app also needs
    --disable-features=NativeNotifications,SystemNotifications --disable-notifications
    and a --user-data-dir in $TMPDIR; Puppeteer takes headless: 'shell'.
  * A check that needs a real window: run it in the test VM (ssh <vm> '...' or a testvm/ script),
    whose screen is the test screen.
"""

REFUSAL = ("REFUSED (guard-headed-browser): this command would put a browser window or notification "
           "prompt on this Mac's screen. Nothing ran.\n  - %s\n" + ADVICE)

MCP_REFUSAL = ("REFUSED (guard-headed-browser): `%s` drives a browser that would put a window or "
               "notification prompt on this Mac's screen. Nothing ran.\n  - %s\n" + ADVICE +
               "  * For this MCP server: add --headless (and, for a full Chrome app, the switches "
               "above) to its launch configuration and restart the session.\n")


def check(payload_text):
    """PreToolUse[Bash] (and an mcp__ payload, handed to check_mcp): 2 with the refusal on stderr,
    else 0. Unreadable input passes."""
    try:
        payload = json.loads(payload_text)
    except ValueError:
        return 0
    if isinstance(payload, dict) and str(payload.get("tool_name") or "").startswith("mcp__"):
        return check_mcp(payload_text)
    if not isinstance(payload, dict) or payload.get("tool_name") not in (None, "Bash"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str) or not TRIGGER.search(command):
        return 0
    try:
        reasons = verdict(command, cwd=payload.get("cwd"))
    except Exception:  # any error in the check is a pass
        return 0
    if not reasons:
        return 0
    sys.stderr.write(REFUSAL % "\n  - ".join(reasons))
    return 2


# --- MCP ---------------------------------------------------------------------------------------
BROWSER_SERVER = re.compile(r"playwright|puppeteer|chrome[-_]?devtools|selenium|webdriver|"
                            r"browser[-_]?use|browsermcp|browserbase|chromium", re.I)
BROWSER_PACKAGE = re.compile(r"@playwright/mcp|playwright-mcp|chrome-devtools-mcp|server-puppeteer|"
                             r"puppeteer-mcp|mcp-selenium|browser-use|@browsermcp", re.I)
NOT_JUDGED = re.compile(r"claude[-_]in[-_]chrome", re.I)
ATTACH_FLAGS = re.compile(r"^--(?:cdp-endpoint|browserUrl|browser-url|wsEndpoint|ws-endpoint|"
                          r"connect|remote-debugging)", re.I)


def _server_of(tool_name):
    m = re.match(r"^mcp__(.+?)__", tool_name or "")
    return m.group(1) if m else None


def _config_files(cwd):
    files = []
    if cwd:
        files.append(os.path.join(cwd, ".mcp.json"))
    base = os.environ.get("CLAUDE_CONFIG_DIR") or ""
    if base:
        files.append(os.path.join(base, ".claude.json"))
    files.append(os.path.join(os.path.expanduser("~"), ".claude.json"))
    return files


def _server_config(server, cwd):
    """The server's launch entry ({command, args, env}) or None."""
    for path in _config_files(cwd):
        text = _read(path)
        if not text:
            continue
        try:
            doc = json.loads(text)
        except ValueError:
            continue
        pools = [doc.get("mcpServers") or {}]
        projects = doc.get("projects") or {}
        if cwd and isinstance(projects, dict):
            p = projects.get(cwd) or {}
            pools.insert(0, p.get("mcpServers") or {})
        for pool in pools:
            if isinstance(pool, dict) and isinstance(pool.get(server), dict):
                return pool[server]
    return None


def mcp_verdict(tool_name, cwd):
    server = _server_of(tool_name)
    if not server or NOT_JUDGED.search(server):
        return None
    config = _server_config(server, cwd)
    launch = ""
    if config:
        launch = " ".join([str(config.get("command") or "")] +
                          [str(a) for a in (config.get("args") or [])] + [str(config.get("url") or "")])
    if not (BROWSER_SERVER.search(server) or BROWSER_PACKAGE.search(launch)):
        return None
    if config is None:
        return ("the browser server `%s` launches a visible browser by default, and its configuration "
                "(.mcp.json or Claude Code's config) does not show --headless" % server)
    args = [str(a) for a in (config.get("args") or [])]
    env = config.get("env") or {}
    if any(ATTACH_FLAGS.match(a) for a in args):
        return None
    if config.get("url") or config.get("type") in ("http", "sse"):
        return None  # a remote server: its browser is not on this Mac
    headless = (
        any(a.lower() in ("--headless", "--headless=true", "--headless=new", "--headless=shell") for a in args)
        or re.search(r'"headless"\s*:\s*(?:true|"new"|"shell")', str(env.get("PUPPETEER_LAUNCH_OPTIONS", "")))
        or str(env.get("HEADLESS", "")).lower() in ("1", "true", "yes"))
    if not headless:
        return ("the browser server `%s` is configured without --headless, so it opens a visible window"
                % server)
    return _mcp_notification_problem(server, launch, args, env, cwd)


def _mcp_notification_problem(server, launch, args, env, cwd):
    """A headless launching browser server that starts the full Chrome app without the notification
    switches (2026-10-29): Puppeteer (Chrome for Testing unless the headless shell), or a server
    given an executable path or a chromium/beta/dev/canary browser or channel."""
    evidence = " ".join([launch] + [str(v) for v in env.values()])
    if "SystemNotifications" in evidence and "NativeNotifications" in evidence:
        return None
    if _config_has_notification_switches(cwd, args):
        return None
    why = None
    if re.search(r"puppeteer", server + " " + launch, re.I) and \
            not re.search(r'"headless"\s*:\s*"shell"', str(env.get("PUPPETEER_LAUNCH_OPTIONS", ""))):
        why = "launches Puppeteer's Chrome for Testing, the full app"
    for k, a in enumerate(args):
        if re.match(r"^--(?:executable-path|executablePath)(?:=|$)", a):
            why = why or "launches the browser at an executable path"
        for option in ("--browser", "--channel"):
            value = a.split("=", 1)[1] if a.startswith(option + "=") else \
                (args[k + 1] if a == option and k + 1 < len(args) else None)
            if value and re.fullmatch(FULL_APP_CHANNEL, value):
                why = why or "launches the `%s` build, the full Chrome app" % value
    if not why:
        return None
    return ("the browser server `%s` %s, %s (for this server: in its launch options or config file)"
            % (server, why, NOTIFY_ADVICE))


def check_mcp(payload_text):
    try:
        payload = json.loads(payload_text)
    except ValueError:
        return 0
    if not isinstance(payload, dict):
        return 0
    tool = payload.get("tool_name") or ""
    if not isinstance(tool, str) or not tool.startswith("mcp__"):
        return 0
    try:
        reason = mcp_verdict(tool, payload.get("cwd"))
    except Exception:
        return 0
    if not reason:
        return 0
    sys.stderr.write(MCP_REFUSAL % (tool, reason))
    return 2


def main(argv):
    if argv and argv[0] == "check":
        return check(sys.stdin.read())
    if argv and argv[0] == "check-mcp":
        return check_mcp(sys.stdin.read())
    sys.stderr.write("usage: headed_browser.py check|check-mcp < payload.json\n")
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
