#!/usr/bin/env python3
"""The operator probes, inside a disposable test VM guest (run by run-probes.py, never on a host).

Operator back-end spec r3 §5 (richos-hq docs/plans/2026-09-24-operator-back-end-spec-r3.md).
Each probe is one short `claude` session driven over stream-json the way the app will drive
his lead, with its CONTROL in the same run. Verdicts:

  PASS           the pass condition held AND the control behaved as its premise says
  FAIL           the control behaved, and the pass condition did not hold
  PREMISE-FALSE  the control did not behave as the premise says; nothing can be concluded
  NOT-RUN        something the probe needs is absent (named); nothing was concluded
  RECORDED       a measurement probe with no pass line (P11, P14): the numbers are the result
  RETIRED        kept in this file as the record of what it measured, and never run again (P6, r4 §1.1)
  ERROR          the harness itself failed; the traceback is recorded

Results: `<payload>/results/<probe>.json`, the redacted frames of every session
(`<probe>-<session>.jsonl`, account fields removed), and `summary.json`. Environments are
recorded as NAMES only; no value of any variable is written, except the few non-secret
facts a probe exists to read (CLAUDE_CODE_ENTRYPOINT, a pid, a path).

Python 3.9 compatible: this runs under the guest's /usr/bin/python3.
"""
import argparse
import fcntl
import glob
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import threading
import time
import traceback
import uuid
# `Lead.user` takes a parameter named `uuid` (the id to send), which shadows the module there.
uuid_module = uuid

GUEST_CLAUDE = '/Users/admin/.local/bin/claude'
# The spec's allowlist (r3 (i)), captured by `enable.sh` from his terminal.
ALLOWLIST = ['PATH', 'LANG', 'USER', 'LOGNAME', 'SHELL', 'JAVA_HOME', 'ANDROID_HOME', 'ANDROID_SDK_ROOT',
             'HOMEBREW_PREFIX', 'HOMEBREW_CELLAR', 'HOMEBREW_REPOSITORY', 'INFOPATH', 'GIT_EDITOR',
             'COREPACK_ENABLE_AUTO_PIN']
# The harness's one addition, stated: `claude` updates itself, and a lead that updated itself
# mid-probe would stop being the version this run records (testvm/lib.sh, TESTVM_CLAUDE_PIN).
HARNESS_PIN = {'DISABLE_AUTOUPDATER': '1'}
# The app's own additions (r3 (i)): the claim id, and the supervisor's pid variable; and r4
# §2.3's one CLAUDE_* exception, set by the app since P17 passed (operator_profile.rs ARTIFACT_ENV).
APP_NAMES = ['HOME', 'SSH_AUTH_SOCK', 'TMPDIR', 'RICHOS_OPERATOR_LEAD', 'CLAUDE_CODE_ARTIFACT']
ARTIFACT = ('CLAUDE_CODE_ARTIFACT', '1')
SUPERVISOR_NAMES = ['RICHOS_SESSION_PID']
# Names any Python adds to its own environment on macOS (operator_profile.rs,
# INTERPRETER_ADDED_NAMES, measured 2026-09-24).
INTERPRETER_NAMES = ['__CF_USER_TEXT_ENCODING', 'LC_CTYPE']
# Most probes measure the platform, not the model, so they run on the cheaper model. P7 runs
# on his own model at his own context size (r3 §5).
PROBE_MODEL = 'sonnet'
HIS_MODEL = 'claude-opus-5-5[1m]'
UNIQUE_CLAUDE_MD = 'The fixture watchword is PERIWINKLE-4417.'
UNIQUE_MEMORY = 'The fixture memory token is AMBER-2291.'
ACCOUNT_KEYS = {'account', 'email', 'emailAddress', 'organization', 'organizationName', 'organizationUuid',
                'accountUuid', 'subscriptionType', 'oauthAccount'}
HOOK_EVENTS = {'SessionStart', 'SessionEnd', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'PostToolUseFailure',
               'Stop', 'StopFailure', 'SubagentStart', 'SubagentStop', 'Notification', 'PreCompact', 'PostCompact',
               'TeammateIdle', 'TaskCompleted'}


# =============================================================================================
# small helpers
# =============================================================================================

def run(args, env=None, cwd=None, timeout=120, input_text=None):
    r = subprocess.run(args, env=env, cwd=cwd, timeout=timeout, input=input_text, text=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return r.returncode, r.stdout, r.stderr


def redact(value):
    """Drop account fields anywhere in a frame, and anything shaped like an email address."""
    if isinstance(value, dict):
        return {k: ('<removed>' if k in ACCOUNT_KEYS else redact(v)) for k, v in value.items()}
    if isinstance(value, list):
        return [redact(v) for v in value]
    if isinstance(value, str):
        return re.sub(r'[\w.+-]+@[\w-]+\.[\w.-]+', '<email>', value)
    return value


def slug(path):
    return re.sub(r'[^A-Za-z0-9]', '-', str(path))


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def children_of(pid):
    """Direct children, by parent pid: processes this harness's own child created."""
    code, out, _ = run(['ps', '-A', '-o', 'pid=,ppid='], timeout=30)
    kids = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1] == str(pid):
            kids.append(int(parts[0]))
    return kids


def pgid_of(pid):
    try:
        return os.getpgid(pid)
    except OSError:
        return None


def environ_names(pid):
    """The variable NAMES a running process was started with, from the kernel
    (sysctl CTL_KERN / KERN_PROCARGS2), without asking the process or its model anything.
    Values are read into memory and dropped; only names leave this function.

    macOS withholds the environment of Apple platform binaries (/bin/sleep, /bin/zsh) even
    from the same user: the kernel returns their arguments and no environment (measured
    2026-09-25 on the host). So None means "withheld or unreadable", never "empty"; read a
    non-Apple child instead (`claude` itself, or node)."""
    import ctypes
    import ctypes.util
    libc = ctypes.CDLL(ctypes.util.find_library('c'), use_errno=True)
    argmax = ctypes.c_int(0)
    size = ctypes.c_size_t(ctypes.sizeof(argmax))
    if libc.sysctl((ctypes.c_int * 2)(1, 8), 2, ctypes.byref(argmax), ctypes.byref(size), None, 0) != 0:
        return None
    buf = ctypes.create_string_buffer(argmax.value)
    size = ctypes.c_size_t(argmax.value)
    if libc.sysctl((ctypes.c_int * 3)(1, 49, pid), 3, buf, ctypes.byref(size), None, 0) != 0:
        return None
    data = buf.raw[:size.value]
    argc = int.from_bytes(data[:4], 'little')
    rest = data[4:]
    pos = rest.find(b'\0')
    while pos < len(rest) and rest[pos:pos + 1] == b'\0':
        pos += 1
    parts = rest[pos:].split(b'\0')
    names = []
    for item in parts[argc:]:
        if not item:
            break
        name = item.split(b'=', 1)[0].decode('utf-8', 'replace')
        if name:
            names.append(name)
    return sorted(set(names)) or None


def descendants(pid):
    """Every process below `pid`, by parent pid: processes our own child created."""
    code, out, _ = run(['ps', '-A', '-o', 'pid=,ppid=,command='], timeout=30)
    rows = []
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
            rows.append((int(parts[0]), int(parts[1]), parts[2] if len(parts) > 2 else ''))
    below, frontier = [], {pid}
    while frontier:
        nxt = set()
        for p, pp, cmd in rows:
            if pp in frontier:
                below.append((p, cmd))
                nxt.add(p)
        frontier = nxt
    return below


def names_file(path):
    try:
        return sorted(set(l.strip() for l in Path(path).read_text().splitlines() if l.strip()))
    except OSError:
        return None


# =============================================================================================
# the frame reading the app does (a port of operator_frames.rs and operator_profile.rs)
# =============================================================================================

def system_messages(stdout):
    def message(value):
        text = value.get('systemMessage') if isinstance(value, dict) else None
        return text if isinstance(text, str) and text.strip() else None
    try:
        value = json.loads(stdout.strip())
        m = message(value)
        return [m] if m else []
    except ValueError:
        pass
    found = []
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith('{'):
            try:
                m = message(json.loads(line))
            except ValueError:
                m = None
            if m:
                found.append(m)
    return found


def alarms_in(frame):
    if not isinstance(frame, dict) or frame.get('type') != 'system':
        return []
    sub = frame.get('subtype')
    if sub == 'hook_response':
        seen = []
        for text in system_messages(frame.get('stdout') or ''):
            if text not in seen:
                seen.append(text)
        return [{'text': t, 'frame': 'hook_response', 'hook_event': frame.get('hook_event'),
                 'hook_name': frame.get('hook_name')} for t in seen]
    if sub == 'informational':
        text = frame.get('content')
        if isinstance(text, str) and text.strip():
            return [{'text': text, 'frame': 'informational', 'level': frame.get('level')}]
    return []


def dedupe_key(text):
    text = text.strip()
    if ' says: ' in text:
        speaker, rest = text.split(' says: ', 1)
        if not re.search(r'\s', speaker) and speaker.split(':')[0] in HOOK_EVENTS:
            return rest.strip()
    return text


def parse_banner(text):
    """operator_profile::parse_banner: the person's short line first (what arrives as a
    systemMessage), then the model's long summary."""
    m = re.match(r'\s*RichOS engine (\S+): ENFORCEMENT ACTIVE for (.*?) \((\d+)/(\d+) guards, engine at (.*?), root via ',
                 text, re.S)
    if m:
        return {'version': m.group(1), 'engine_root': m.group(5), 'governing': m.group(2),
                'guard_count': int(m.group(3)), 'guard_expected': int(m.group(4))}
    m = re.match(r'\s*RichOS engine (\S+) ACTIVE\. Engine: (.*?)\. Governing: (.*?) \(resolved via .*?\)\. '
                 r'(?:.* )?(\d+)/(\d+) guards present', text, re.S)
    if not m:
        return None
    return {'version': m.group(1), 'engine_root': m.group(2), 'governing': m.group(3),
            'guard_count': int(m.group(4)), 'guard_expected': int(m.group(5))}


def init_check(init, banner, declared_engine):
    """The (b) init check, as operator_profile::init_check decides it (fences excluded: the
    engine has no fence status command yet, r3 §11 item 4)."""
    plugins = [x.get('name') for x in (init or {}).get('plugins') or []]
    if plugins.count('richos-engine') != 1:
        return 'refuse', 'richos-engine listed %d times' % plugins.count('richos-engine')
    if 'richos-app-engine' in plugins:
        return 'refuse', 'richos-app-engine loaded beside his'
    if 'mcp__richos_operator__report' not in ((init or {}).get('tools') or []):
        return 'refuse', 'no report tool'
    if not banner:
        return 'refuse', 'no ACTIVE banner'
    if '/.claude/plugins/cache/' in banner['engine_root'] or \
            os.path.realpath(banner['engine_root']) != os.path.realpath(declared_engine):
        return 'refuse', 'engine root %s' % banner['engine_root']
    if banner['guard_count'] != banner['guard_expected']:
        return 'refuse', 'guards %d/%d' % (banner['guard_count'], banner['guard_expected'])
    return 'open', None


# =============================================================================================
# the fixture: his entity, his memory, his engine, a repository with an SSH remote
# =============================================================================================

class Paths(object):
    def __init__(self, payload):
        self.payload = Path(payload)
        self.home = self.payload / 'home'
        self.claude_dir = self.home / '.claude'
        self.ab = self.home / 'ab'
        self.market = self.ab / 'richos'
        self.engine = self.market / 'richos' / 'engine'
        self.entity = self.ab / 'femcboost'
        self.remote = self.payload / 'remote.git'
        self.results = self.payload / 'results'
        self.work = self.payload / 'work'
        self.contract = self.work / 'operator-contract.md'
        self.stub_mcp = self.work / 'operator_stub_mcp.py'
        self.stub_calls = self.work / 'operator-stub-calls.jsonl'
        self.app_engine_plugin = self.work / 'app-engine-plugin'


STUB_MCP = r'''
import json, sys, time
LOG = sys.argv[1]
def reply(i, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": i, "result": result}) + "\n"); sys.stdout.flush()
for line in sys.stdin:
    try:
        m = json.loads(line)
    except Exception:
        continue
    i, method = m.get("id"), m.get("method")
    if i is None:
        continue
    if method == "initialize":
        reply(i, {"protocolVersion": m.get("params", {}).get("protocolVersion", "2025-06-18"),
                  "capabilities": {"tools": {}}, "serverInfo": {"name": "richos_operator", "version": "probe"}})
    elif method == "tools/list":
        reply(i, {"tools": [{"name": "report", "description": "Tell the CEO something.",
                             "inputSchema": {"type": "object", "properties": {"kind": {"type": "string"},
                                             "text": {"type": "string"}}, "required": ["kind", "text"]}}]})
    elif method == "tools/call":
        with open(LOG, "a") as f:
            f.write(json.dumps({"at": time.time(), "params": m.get("params")}) + "\n")
        reply(i, {"content": [{"type": "text", "text": "{\"recorded\": true}"}], "isError": False})
    else:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": i, "error": {"code": -32601, "message": "no"}}) + "\n")
        sys.stdout.flush()
'''


def login_environment():
    """The guest's own terminal environment: a login shell's, as enable.sh would capture it."""
    code, out, err = run(['/bin/zsh', '-lic', 'env -0'], timeout=60)
    if code != 0:
        raise RuntimeError('the guest login shell could not be read: ' + err)
    env = {}
    for item in out.split('\0'):
        if '=' in item:
            k, v = item.split('=', 1)
            env[k] = v
    return env


def stored_environment(login):
    """What enable.sh stores from his terminal: the allowlist, with `claude` findable on PATH.
    His terminal runs `claude` by name, so its PATH has the installer's folder, and his engine's
    hooks rely on that (Pierce runs `shutil.which('claude')`). The guest's login shell does not
    add it, so the harness does, first, unless it is already there."""
    stored = {k: login[k] for k in ALLOWLIST if k in login}
    stored.setdefault('LANG', 'en_US.UTF-8')
    folder = os.path.dirname(GUEST_CLAUDE)
    path = stored.get('PATH', '/usr/bin:/bin')
    stored['PATH'] = path if folder in path.split(':') else folder + ':' + path
    return stored


def git(cwd, *args):
    code, out, err = run(['git', '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgSign=false',
                          '-c', 'user.name=Probe', '-c', 'user.email=probe@example.invalid'] + list(args),
                         cwd=str(cwd), timeout=120)
    if code != 0:
        raise RuntimeError('git %s: %s' % (' '.join(args), err))
    return out.strip()


def setup_fixture(p, record):
    for d in (p.results, p.work, p.ab):
        d.mkdir(parents=True, exist_ok=True)
    if not p.engine.is_dir():
        p.market.mkdir(parents=True, exist_ok=True)
        code, _, err = run(['tar', '-xf', str(p.payload / 'probes' / 'engine.tar'), '-C', str(p.market)])
        if code != 0:
            raise RuntimeError('engine archive did not unpack: ' + err)
    p.entity.mkdir(parents=True, exist_ok=True)
    (p.entity / 'orchestration.config').write_text(
        '# Probe fixture entity.\nMODEL_TIERS="fable > opus > sonnet > haiku"\nMODEL_CEILING="opus"\n'
        'ALLOWED_MODELS="opus sonnet haiku"\n')
    (p.entity / 'CLAUDE.md').write_text('# Probe fixture\n\n' + UNIQUE_CLAUDE_MD + '\n')
    agents = p.entity / '.claude' / 'agents'
    agents.mkdir(parents=True, exist_ok=True)
    for name in ('alpha', 'beta'):
        (agents / (name + '.md')).write_text(
            '---\nname: %s\ndescription: Probe teammate %s. Runs exactly the shell command its brief names.\n'
            'model: sonnet\ntools: Bash\n---\nYou are a probe teammate. Run exactly the one Bash command your '
            'brief names, in the foreground, then reply with the single word finished.\n' % (name, name))
    # P16 (r4 §5): a teammate that can also message its lead, for the recorded half of P16.
    (agents / 'gamma.md').write_text(
        '---\nname: gamma\ndescription: Probe teammate gamma. Runs the shell command its brief names, then sends '
        'the message its brief names.\nmodel: sonnet\ntools: Bash, SendMessage\n---\nYou are a probe teammate. Do '
        'exactly what your brief says, in order, then reply with the single word finished.\n')
    (p.entity / '.claude' / 'settings.local.json').write_text(json.dumps(
        {'env': {'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS': '1'}}, indent=1) + '\n')
    (p.entity / '.gitignore').write_text('/.claude/worktrees/*\n')
    if not (p.entity / '.git').is_dir():
        git(p.entity, 'init', '-q', '-b', 'main')
        git(p.entity, 'add', '-A')
        git(p.entity, 'commit', '-q', '-m', 'Probe fixture')
    memory = p.claude_dir / 'projects' / slug(p.entity) / 'memory'
    memory.mkdir(parents=True, exist_ok=True)
    (memory / 'MEMORY.md').write_text('# Memory\n\n- ' + UNIQUE_MEMORY + '\n')
    settings = {
        'extraKnownMarketplaces': {'richos-local': {'source': {'source': 'directory', 'path': str(p.market)}}},
        'enabledPlugins': {'richos-engine@richos-local': True},
        'skipDangerousModePermissionPrompt': True,
    }
    (p.claude_dir / 'settings.json').write_text(json.dumps(settings, indent=1) + '\n')
    config_path = p.home / '.claude.json'
    config = json.loads(config_path.read_text()) if config_path.exists() else {}
    config.update({'hasCompletedOnboarding': True, 'theme': 'dark', 'bypassPermissionsModeAccepted': True})
    config.setdefault('projects', {})[str(p.entity)] = {'hasTrustDialogAccepted': True,
                                                         'hasCompletedProjectOnboarding': True}
    config_path.write_text(json.dumps(config, indent=1) + '\n')
    p.contract.write_text('You are the operator lead for this fixture. CLAUDE.md wins on any question of '
                          'substance. Use richos_operator.report to tell the CEO about a land, a file or a question.\n')
    p.stub_mcp.write_text(STUB_MCP)
    # A stand-in for a long build. Run 5: 2.1.282's Bash tool refuses a standalone `sleep 400`
    # ("Blocked: standalone sleep 400"), so a teammate briefed to sleep backgrounded it and
    # finished at once; this prints progress every ten seconds and is what it says it is.
    (p.work / 'long-task.py').write_text(
        'import sys, time\n'
        'n = int(sys.argv[1]); t0 = time.time()\n'
        'while time.time() - t0 < n:\n'
        '    print("long task: %d of %d seconds" % (int(time.time() - t0), n), flush=True)\n'
        '    time.sleep(min(10, max(0.1, n - (time.time() - t0))))\n'
        'print("long task: done", flush=True)\n')
    plugin = p.app_engine_plugin / '.claude-plugin'
    plugin.mkdir(parents=True, exist_ok=True)
    (plugin / 'plugin.json').write_text(json.dumps({'name': 'richos-app-engine', 'version': '1.2.0'}) + '\n')
    ssh = Path('/Users/admin/.ssh')
    ssh.mkdir(mode=0o700, exist_ok=True)
    key = ssh / 'id_probe'
    if not key.exists():
        code, _, err = run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(key), '-C', 'operator-probe'])
        if code != 0:
            raise RuntimeError('ssh-keygen: ' + err)
        with open(str(ssh / 'authorized_keys'), 'a') as f:
            f.write((ssh / 'id_probe.pub').read_text())
        os.chmod(str(ssh / 'authorized_keys'), 0o600)
        with open(str(ssh / 'config'), 'a') as f:
            f.write('\nHost probe-remote\n  HostName 127.0.0.1\n  User admin\n  IdentityFile %s\n'
                    '  StrictHostKeyChecking accept-new\n  UserKnownHostsFile %s\n' % (key, ssh / 'probe_known_hosts'))
    if not p.remote.is_dir():
        git(p.payload, 'init', '-q', '--bare', str(p.remote))
        git(p.entity, 'remote', 'add', 'origin', 'probe-remote:' + str(p.remote))
        git(p.entity, 'push', '-q', '-u', 'origin', 'main')
    env = dict(os.environ, HOME=str(p.home), **HARNESS_PIN)
    env.pop('CLAUDE_CONFIG_DIR', None)
    for args in (['plugin', 'marketplace', 'add', str(p.market)], ['plugin', 'install', 'richos-engine@richos-local']):
        code, out, err = run([GUEST_CLAUDE] + args, env=env, cwd=str(p.entity), timeout=180)
        record.setdefault('plugin_install', []).append({'args': args, 'exit': code, 'said': (out + err)[-600:]})


def derive_ssh_auth_sock():
    """r3 (i): `launchctl getenv SSH_AUTH_SOCK`. From an ssh session in the guest that is empty,
    because ssh runs outside the GUI login's launchd domain (measured in the survey run); the
    app runs inside it. So the GUI session's own agent socket is found by its launchd path,
    owned by this user, and the method is recorded with the value."""
    code, out, _ = run(['/bin/launchctl', 'getenv', 'SSH_AUTH_SOCK'], timeout=30)
    if code == 0 and out.strip().startswith('/'):
        return out.strip(), 'launchctl getenv'
    mine = [s for s in glob.glob('/private/tmp/com.apple.launchd.*/Listeners') if os.stat(s).st_uid == os.getuid()]
    if len(mine) == 1:
        return mine[0], 'the GUI session launchd socket path (launchctl getenv is empty over ssh)'
    return None, 'not derivable (%d candidate sockets)' % len(mine)


def python3_for_supervisor(path):
    """operator_profile::resolve_python: the first python3 on the stored PATH that is not the
    xcrun shim."""
    for d in path.split(':'):
        c = os.path.join(d, 'python3')
        if d.startswith('/') and os.access(c, os.X_OK) and c != '/usr/bin/python3':
            return c
    return '/usr/bin/python3'


class Context(object):
    def __init__(self, p, timeout):
        self.p = p
        self.timeout = timeout
        self.login = login_environment()
        self.stored = stored_environment(self.login)
        self.ssh_auth_sock, self.ssh_method = derive_ssh_auth_sock()
        code, out, _ = run(['getconf', 'DARWIN_USER_TEMP_DIR'])
        self.tmpdir = out.strip()
        self.python = python3_for_supervisor(self.stored['PATH'])
        code, out, _ = run(['/bin/zsh', '-lc', 'command -v node'], timeout=30)
        self.node = out.strip() or 'node'
        self.spawned = []
        # W2: the assignment handle each teammate was started under (the per-assignment Stop).
        self.spawn_handles = {}

    def clean_up_teammates(self):
        """Discard every teammate this probe started, through his engine's own registry, so the
        next probe's lead is not sent off by the engine's turn-end gates to deal with them
        (run 7: P13's lead spent its turns discarding P12's leftover). Their leads have ended,
        so they are no longer running. Then drop any worktree git still lists for them."""
        env = dict(self.stored, HOME=str(self.p.home), CLAUDE_PROJECT_DIR=str(self.p.entity))
        said = {}
        for name in self.spawned:
            code, out, err = run(['/bin/bash', str(self.p.engine / 'scripts' / 'workspaces.sh'), 'discard', name,
                                  '--reason', 'operator probe fixture cleanup',
                                  '--not-ceo-ordered', 'the probe harness removes the teammates it started'],
                                 env=env, cwd=str(self.p.entity), timeout=120)
            said[name] = {'exit': code, 'said': (out + err).strip()[-300:]}
        for row in self.worktrees():
            run(['git', '-C', str(self.p.entity), 'worktree', 'remove', '--force', row['worktree']], timeout=60)
        run(['git', '-C', str(self.p.entity), 'worktree', 'prune'], timeout=60)
        self.spawned = []
        return said

    def lead_env(self, claim='probe-claim'):
        """operator_profile::environment, built from empty, plus the harness pin."""
        env = dict(self.stored)
        env['HOME'] = str(self.p.home)
        if self.ssh_auth_sock:
            env['SSH_AUTH_SOCK'] = self.ssh_auth_sock
        env['TMPDIR'] = self.tmpdir
        env['RICHOS_OPERATOR_LEAD'] = claim
        env[ARTIFACT[0]] = ARTIFACT[1]
        env.update(HARNESS_PIN)
        return env

    def mcp_config(self):
        return json.dumps({'mcpServers': {'richos_operator': {
            'type': 'stdio', 'command': self.python, 'args': [str(self.p.stub_mcp), str(self.p.stub_calls)]}}})

    def lead_args(self, session_id=None, resume=None, model=PROBE_MODEL, sources='user,project,local', extra=None,
                  disallow=('AskUserQuestion',)):
        """operator_profile::child_args, in order, plus --model. `disallow` extends the one
        --disallowed-tools flag rather than adding a second one."""
        args = ['--print', '--input-format=stream-json', '--output-format=stream-json', '--include-partial-messages',
                '--verbose', '--include-hook-events', '--replay-user-messages', '--setting-sources', sources]
        args += ['--resume', resume] if resume else ['--session-id', session_id or str(uuid.uuid4())]
        args += ['--permission-prompt-tool', 'stdio', '--dangerously-skip-permissions',
                 '--disallowed-tools'] + list(disallow) + ['--model', model]
        args += list(extra or [])
        args += ['--append-system-prompt-file', str(self.p.contract), '--mcp-config', self.mcp_config()]
        return args

    def front_desk_args(self, session_id):
        """native.rs child_args: the product front desk's own invocation."""
        return ['--print', '--input-format=stream-json', '--output-format=stream-json', '--include-partial-messages',
                '--verbose', '--setting-sources', '', '--no-session-persistence', '--session-id', session_id,
                '--permission-prompt-tool', 'stdio', '--model', PROBE_MODEL]

    def supervisor(self):
        return [self.python, '-B', str(self.p.engine / 'scripts' / 'provider-supervisor.py')]

    def supervisor_reaps(self):
        text = (self.p.engine / 'scripts' / 'provider-supervisor.py').read_text()
        return '--reap-descendants' in text

    def liveness(self, target):
        """agent-liveness.sh from HIS engine (B9), with his environment, from outside the lead."""
        env = dict(self.stored, HOME=str(self.p.home), CLAUDE_PROJECT_DIR=str(self.p.entity))
        code, out, err = run(['/bin/bash', str(self.p.engine / 'scripts' / 'agent-liveness.sh'), '--entity',
                              str(self.p.entity), '--json', target], env=env, cwd=str(self.p.entity), timeout=60)
        verdict = {0: 'NOT-ALIVE', 10: 'ALIVE', 11: 'INDETERMINATE'}.get(code, 'exit %d' % code)
        try:
            detail = json.loads(out)
        except ValueError:
            detail = {'stdout': out[-800:], 'stderr': err[-800:]}
        return verdict, detail

    def worktrees(self):
        code, out, _ = run(['git', '-C', str(self.p.entity), 'worktree', 'list', '--porcelain'], timeout=30)
        rows, row = [], {}
        for line in out.splitlines() + ['']:
            if not line.strip():
                if row:
                    rows.append(row)
                row = {}
                continue
            key, _, value = line.partition(' ')
            row[key] = value if value else True
        return [r for r in rows if '/.claude/worktrees/' in r.get('worktree', '')]


# =============================================================================================
# a lead, driven over stream-json as the app will drive it
# =============================================================================================

OWNER = r'''
import os, subprocess, sys, time
# Stands in for the app: the supervisor's parent. Killing this process is the app's death.
child = subprocess.Popen(sys.argv[2:], start_new_session=True)
open(sys.argv[1], "w").write(str(child.pid))
child.wait()
'''


class Lead(object):
    """One `claude` session. `permission` is how a `can_use_tool` is answered ('allow', as the
    permission desk would for a lead in bypass mode, or 'deny'); every other control request
    the child makes is answered with an error, as native.rs does. `owner=True` puts a stand-in
    for the app between this harness and the supervisor, so the app's death can be caused."""

    def __init__(self, ctx, probe, name, args, env=None, supervised=True, reap=False, cwd=None,
                 permission='allow', owner=False, scrub=None):
        self.ctx, self.probe, self.name = ctx, probe, name
        # A frame rewrite applied only when the frames are SAVED (P17: his account's artifact
        # list is read to prove the tool works, and never written into the record).
        self.scrub = scrub
        self.supervised = supervised
        self.frames = []
        self.requests = []
        self.permission = permission
        self.cond = threading.Condition()
        self.closed = False
        command = (ctx.supervisor() + (['--reap-descendants'] if reap else []) if supervised else []) + [GUEST_CLAUDE] + args
        self.owner_pidfile = None
        if owner:
            self.owner_pidfile = ctx.p.work / ('%s-%s.supervisor.pid' % (probe, name))
            owner_script = ctx.p.work / 'owner.py'
            owner_script.write_text(OWNER)
            command = [ctx.python, '-B', str(owner_script), str(self.owner_pidfile)] + command
        self.stderr_path = ctx.p.results / ('%s-%s.stderr.txt' % (probe, name))
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=open(str(self.stderr_path), 'w'), cwd=str(cwd or ctx.p.entity),
                                     env=env if env is not None else ctx.lead_env(), start_new_session=True,
                                     text=True, bufsize=1)
        self.started = time.time()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def supervisor_pid(self):
        """The supervisor's pid: this harness's own child, or the owner stand-in's."""
        if not self.owner_pidfile:
            return self.proc.pid
        for _ in range(50):
            if self.owner_pidfile.exists() and self.owner_pidfile.read_text().strip().isdigit():
                return int(self.owner_pidfile.read_text().strip())
            time.sleep(0.1)
        return None

    def _answer(self, frame):
        req = frame.get('request') or {}
        rid = frame.get('request_id')
        self.requests.append({'t': round(time.time() - self.started, 3), 'subtype': req.get('subtype'),
                              'tool_name': req.get('tool_name'), 'input_keys': sorted((req.get('input') or {}).keys())})
        if req.get('subtype') == 'can_use_tool':
            body = {'behavior': 'allow', 'updatedInput': req.get('input') or {}} if self.permission == 'allow' else \
                {'behavior': 'deny', 'message': 'Denied by the probe.'}
            reply = {'type': 'control_response', 'response': {'subtype': 'success', 'request_id': rid, 'response': body}}
        else:
            reply = {'type': 'control_response', 'response': {'subtype': 'error', 'request_id': rid,
                                                              'error': 'Not implemented by the probe harness.'}}
        try:
            self.send(reply)
        except (BrokenPipeError, ValueError):
            pass

    def _read(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                frame = json.loads(line)
            except ValueError:
                continue
            if frame.get('type') == 'control_request':
                self._answer(frame)
            with self.cond:
                self.frames.append((time.time(), frame))
                self.cond.notify_all()
        with self.cond:
            self.closed = True
            self.cond.notify_all()

    def send(self, obj):
        self.proc.stdin.write(json.dumps(obj) + '\n')
        self.proc.stdin.flush()

    def user(self, text, priority=None, uuid=None):
        # P18 resends one message under the uuid it was first sent with, as the app's retry does.
        mid = uuid or str(uuid_module.uuid4())
        msg = {'type': 'user', 'uuid': mid, 'message': {'role': 'user', 'content': [{'type': 'text', 'text': text}]}}
        if priority:
            msg['priority'] = priority
        self.send(msg)
        return mid

    def control(self, subtype, **fields):
        rid = 'req_' + uuid.uuid4().hex[:10]
        request = dict(subtype=subtype, **fields)
        self.send({'type': 'control_request', 'request_id': rid, 'request': request})
        return rid

    def wait(self, pred, timeout, start=0):
        deadline = time.time() + timeout
        index = start
        with self.cond:
            while True:
                while index < len(self.frames):
                    t, f = self.frames[index]
                    if pred(f):
                        return index, t, f
                    index += 1
                remaining = deadline - time.time()
                if remaining <= 0 or self.closed:
                    return None
                self.cond.wait(min(remaining, 1.0))

    def initialize(self, **declared):
        """The handshake. `declared` carries capability fields of the initialize request, such
        as perTaskStopAffordance (P3, P12)."""
        rid = self.control('initialize', hooks={}, **declared)
        got = self.wait(lambda f: f.get('type') == 'control_response'
                        and (f.get('response') or {}).get('request_id') == rid, 60)
        return bool(got) and got[2]['response'].get('subtype') == 'success'

    def result_after(self, start, timeout):
        return self.wait(lambda f: f.get('type') == 'result', timeout, start)

    def count(self):
        with self.cond:
            return len(self.frames)

    def all_frames(self):
        with self.cond:
            return [f for _, f in self.frames]

    def init_frame(self):
        for f in self.all_frames():
            if f.get('type') == 'system' and f.get('subtype') == 'init':
                return f
        return None

    def alarms(self):
        found = []
        for f in self.all_frames():
            found.extend(alarms_in(f))
        return found

    def text(self, start=0):
        with self.cond:
            frames = [f for _, f in self.frames[start:]]
        out = []
        for f in frames:
            if f.get('type') == 'assistant':
                for block in (f.get('message') or {}).get('content') or []:
                    if block.get('type') == 'text':
                        out.append(block.get('text', ''))
        return '\n'.join(out)

    def claude_pid(self):
        if not self.supervised:
            return self.proc.pid
        sup = self.supervisor_pid()
        kids = children_of(sup) if sup else []
        return kids[0] if kids else None

    def close(self, grace=15):
        try:
            self.proc.stdin.close()
        except Exception:  # noqa: BLE001
            pass
        try:
            self.proc.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            # Owned: the group of the process this harness started, never a name.
            for sig in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(self.proc.pid, sig)
                except ProcessLookupError:
                    break
                try:
                    self.proc.wait(timeout=5)
                    break
                except subprocess.TimeoutExpired:
                    continue
        self.save()

    def save(self):
        path = self.ctx.p.results / ('%s-%s.jsonl' % (self.probe, self.name))
        with self.cond:
            rows = list(self.frames)
        with open(str(path), 'w') as f:
            for t, frame in rows:
                if frame.get('type') == 'control_response' and \
                        (frame.get('response') or {}).get('response', {}).get('account') is not None:
                    frame = {'type': 'control_response', 'response': {'subtype': frame['response'].get('subtype'),
                                                                      'request_id': frame['response'].get('request_id'),
                                                                      'response': '<initialize reply removed>'}}
                if self.scrub:
                    frame = self.scrub(frame)
                f.write(json.dumps({'t': round(t - self.started, 3), 'frame': redact(frame)}) + '\n')


def session_record(p, session_id=None, pid=None):
    for path in glob.glob(str(p.claude_dir / 'sessions' / '*.json')):
        try:
            d = json.loads(Path(path).read_text())
        except (OSError, ValueError):
            continue
        if (session_id and d.get('sessionId') == session_id) or (pid and d.get('pid') == pid):
            return {k: d.get(k) for k in ('pid', 'kind', 'entrypoint', 'cwd', 'sessionId', 'version')}
    return None


def ask(lead, text, timeout, priority=None):
    start = lead.count()
    lead.user(text, priority=priority)
    got = lead.result_after(start, timeout)
    return start, got


# Said honestly, because it is true, and because a lead that is not told refuses (run 2, P8:
# "has the shape of reconnaissance/exfiltration"). The probe measures the lead's platform, so
# the lead is told it is a probe.
FIXTURE_NOTE = ('This session is an automated probe of the operator setup, running in a disposable test virtual '
                'machine that is deleted after the run. The folders, the files and the git remote here are scratch '
                'fixtures made for this probe (the remote is a bare repository on this same machine). Nothing here is '
                'shared or real. The request below is the probe\'s measurement; please carry it out exactly as written. ')


def do(lead, text, timeout, priority=None):
    """A request the lead must carry out, with the fixture note in front of it."""
    return ask(lead, FIXTURE_NOTE + text, timeout, priority)


def tool_uses(lead, start, name=None):
    return [b for f in lead.all_frames()[start:] if f.get('type') == 'assistant'
            for b in (f.get('message') or {}).get('content') or []
            if b.get('type') == 'tool_use' and (name is None or b.get('name') == name)]


def tool_result_text(lead, tool_use_id):
    for f in lead.all_frames():
        if f.get('type') != 'user':
            continue
        for b in (f.get('message') or {}).get('content') or []:
            if isinstance(b, dict) and b.get('type') == 'tool_result' and b.get('tool_use_id') == tool_use_id:
                c = b.get('content')
                return c if isinstance(c, str) else '\n'.join(x.get('text', '') for x in c or [] if isinstance(x, dict))
    return None


def bash_output(lead, text, timeout=300):
    """Ask for one Bash command and read its output off the stream, not from a file."""
    start, _ = do(lead, text, timeout)
    uses = tool_uses(lead, start, 'Bash')
    return (tool_result_text(lead, uses[-1].get('id')) if uses else None), start


# =============================================================================================
# teammates, started the way his lead starts them: spawn.sh, then the Agent call it prints
# =============================================================================================

def brief(ctx, name, seconds, dirty=False, then=''):
    path = ctx.p.work / ('brief-%s.md' % name)
    first = ('First, give your workspace one uncommitted change by running exactly this Bash command in your '
             'current working directory: echo change > probe-change.txt\nThen: ') if dirty else ''
    # The owned-state line is the engine's own documented way through guard-owned-state.sh
    # (run 3: the fixture has no CI, so its CI check reports unhealthy and refuses every spawn).
    path.write_text(first + 'Run exactly this Bash command in the foreground, with the Bash tool\'s timeout parameter set to '
                    '600000, and wait for it to finish: %s\n'
                    'It is a stand-in for a long build and takes about %d seconds. %sThen reply with the single word '
                    'finished. Do nothing else.\n\n'
                    'owned-state-ack: ci — this probe fixture has no CI at all, and its VM is deleted after the run\n'
                    % (long_task(ctx, seconds), seconds, (then + ' ') if then else ''))
    return path


def long_task(ctx, seconds):
    return 'python3 %s %d' % (ctx.p.work / 'long-task.py', seconds)


def in_lead_shell(ctx, lead, start, timeout=120):
    """Wait until the LEAD ITSELF (not one of its agents, whose frames carry a
    parent_tool_use_id) makes a Bash call."""
    return lead.wait(lambda f: f.get('type') == 'assistant' and not f.get('parent_tool_use_id') and any(
        b.get('type') == 'tool_use' and b.get('name') == 'Bash' for b in (f.get('message') or {}).get('content') or []),
        timeout, start)


def lead_long_task(ctx, seconds, then):
    """The prompt that puts the lead itself mid-turn in a shell command for `seconds`."""
    return FIXTURE_NOTE + ('Run exactly this Bash command yourself, in the foreground, with the Bash tool\'s timeout '
                           'parameter set to 600000, and wait for it: %s. It is a stand-in for a long build. '
                           'Then reply %s.' % (long_task(ctx, seconds), then))


def spawn_teammate(ctx, lead, name, agent_type, seconds, timeout=300, dirty=False, then=''):
    """Returns (task_id, worktree_path, detail). The lead runs spawn.sh and makes the Agent call
    with run_in_background, exactly as his terminal's lead does (engine-status banner)."""
    before = set(r['worktree'] for r in ctx.worktrees())
    ctx.spawned.append(name)
    # --description: the Agent tool requires one and spawn.sh adds it only when asked (final run:
    # "The parameter `description` type is expected as `string` but provided as `unknown`").
    command = "%s %s --repo %s --type %s --brief %s --model sonnet --description 'Probe teammate %s'" % (
        ctx.p.engine / 'scripts' / 'spawn.sh', name, ctx.p.entity, agent_type, brief(ctx, name, seconds, dirty, then), name)
    start, got = do(lead, (
        'Start one teammate. Step 1: run exactly this Bash command and read its standard output:\n%s\n'
        'Step 2: its standard output is one JSON object of Agent tool parameters. Call the Agent tool ONCE with '
        'exactly those parameters, adding run_in_background set to true. Step 3: reply with the single word '
        'started. Do nothing else. If step 1 refuses, reply with its full refusal text instead.' % command), timeout)
    frames = lead.all_frames()[start:]
    agent_use = None
    for f in frames:
        if f.get('type') == 'assistant':
            for block in (f.get('message') or {}).get('content') or []:
                if block.get('type') == 'tool_use' and block.get('name') in ('Agent', 'Task') and \
                        (block.get('input') or {}).get('name') == name:
                    agent_use = block
    task_id = None
    for f in frames:
        if f.get('type') == 'system' and f.get('subtype') == 'task_started' and agent_use and \
                f.get('tool_use_id') == agent_use.get('id'):
            task_id = f.get('task_id')
    after = [r for r in ctx.worktrees() if r['worktree'] not in before]
    detail = {'agent_call': bool(agent_use), 'task_id': task_id, 'new_worktrees': after,
              'lead_said': lead.text(start)[-1500:], 'turn_ended': bool(got)}
    return task_id, (after[0]['worktree'] if len(after) == 1 else None), detail


# =============================================================================================
# the probes
# =============================================================================================

PROBES = []
RETIRED = {}
EXPLICIT = set()


def probe(pid, retired=None, explicit=False):
    """Register a probe. A RETIRED one stays in the file as the record of what it measured and
    is never run again: its verdict is RETIRED with the reason. An EXPLICIT one (the W2 walk)
    runs only when --only names it."""
    def register(fn):
        PROBES.append((pid, fn))
        if retired:
            RETIRED[pid] = retired
        if explicit:
            EXPLICIT.add(pid)
        return fn
    return register


@probe('P1')
def p1(ctx, r):
    main = Lead(ctx, 'P1', 'lead', ctx.lead_args())
    try:
        r['initialize'] = main.initialize()
        ask(main, 'Reply with exactly the word ready.', 180)
        init = main.init_frame()
        banners = [parse_banner(dedupe_key(a['text'])) for a in main.alarms()]
        banner = next((b for b in banners if b), None)
        r['plugins'] = [{'name': x.get('name'), 'path': x.get('path'), 'source': x.get('source')}
                        for x in (init or {}).get('plugins') or []]
        r['banner'] = banner
        r['declared_engine'] = str(ctx.p.engine)
        r['plugin_root_note'] = ('CLAUDE_PLUGIN_ROOT is set only inside the plugin\'s own hooks; the banner\'s '
                                 'Engine: is that hook resolving its own root (resolve_engine_root(SCRIPT_DIR)), '
                                 'which is the same fact, read off the wire.')
        r['main_check'] = init_check(init, banner, ctx.p.engine)
    finally:
        main.close()
    control = Lead(ctx, 'P1', 'control-app-engine', ctx.lead_args(extra=['--plugin-dir', str(ctx.p.app_engine_plugin)]))
    try:
        control.initialize()
        ask(control, 'Reply with exactly the word ready.', 180)
        cinit = control.init_frame()
        cbanner = next((b for b in (parse_banner(dedupe_key(a['text'])) for a in control.alarms()) if b), None)
        r['control_plugins'] = [x.get('name') for x in (cinit or {}).get('plugins') or []]
        r['control_check'] = init_check(cinit, cbanner, ctx.p.engine)
    finally:
        control.close()
    if 'richos-app-engine' not in r['control_plugins']:
        return 'PREMISE-FALSE', 'the control run did not load the app engine plugin, so the init check was not tested'
    if r['control_check'][0] != 'refuse':
        return 'FAIL', 'the init check opened a lead with the app engine loaded'
    if r['main_check'][0] != 'open':
        return 'FAIL', 'the lead did not pass the init check: ' + str(r['main_check'][1])
    return 'PASS', 'richos-engine loaded once from %s; the app-engine control was refused' % (banner or {}).get('engine_root')


def interactive_session(ctx, r, seconds_for_record=60):
    """An interactive `claude` in a pseudo-terminal, as his terminal runs it. Returns
    (pid, fd, record) with the session still running; the caller ends it."""
    env = dict(ctx.stored, HOME=str(ctx.p.home), TERM='xterm-256color', **HARNESS_PIN)
    if ctx.ssh_auth_sock:
        env['SSH_AUTH_SOCK'] = ctx.ssh_auth_sock
    env['TMPDIR'] = ctx.tmpdir
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(str(ctx.p.entity))
        os.execve(GUEST_CLAUDE, [GUEST_CLAUDE, '--dangerously-skip-permissions', '--model', PROBE_MODEL], env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 50, 160, 0, 0))
    screen = []
    deadline = time.time() + seconds_for_record
    record = None
    while time.time() < deadline and record is None:
        drain(fd, screen, 1.0)
        record = session_record(ctx.p, pid=pid)
    return pid, fd, record, screen


def drain(fd, screen, seconds):
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.2)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                return
            if not chunk:
                return
            screen.append(chunk.decode('utf-8', 'replace'))


def type_into(fd, text, screen):
    os.write(fd, text.encode())
    drain(fd, screen, 1.0)
    os.write(fd, b'\r')


def end_interactive(pid, fd):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.kill(pid, sig)  # owned: the pid pty.fork returned
        except ProcessLookupError:
            break
        for _ in range(20):
            done, _ = os.waitpid(pid, os.WNOHANG)
            if done:
                break
            time.sleep(0.25)
        else:
            continue
        break
    try:
        os.close(fd)
    except OSError:
        pass


@probe('P2')
def p2(ctx, r):
    # ---- the records: a lead, the front desk, an interactive session, and a print-mode
    #      claude started from that interactive session's tool shell ------------------------
    sid = str(uuid.uuid4())
    lead = Lead(ctx, 'P2', 'lead', ctx.lead_args(session_id=sid))
    fd_sid = str(uuid.uuid4())
    desk_env = dict(ctx.stored, HOME=str(ctx.p.home), TMPDIR=ctx.tmpdir, **HARNESS_PIN)
    desk = Lead(ctx, 'P2', 'front-desk', ctx.front_desk_args(fd_sid), env=desk_env, supervised=False)
    ipid = ifd = None
    try:
        lead.initialize()
        desk.initialize()
        ask(lead, 'Reply with exactly the word ready.', 180)
        ask(desk, 'Reply with exactly the word ready.', 180)
        r['lead_record'] = session_record(ctx.p, session_id=sid)
        r['front_desk_record'] = session_record(ctx.p, session_id=fd_sid) or \
            session_record(ctx.p, pid=desk.proc.pid)
        ipid, ifd, irecord, screen = interactive_session(ctx, r)
        r['interactive_record'] = irecord
        env_file = ctx.p.work / 'p2-interactive-tool-env.txt'
        type_into(ifd, 'Run exactly this Bash command and nothing else, then reply done: env -0 > %s' % env_file, screen)
        deadline = time.time() + 180
        while time.time() < deadline and not (env_file.exists() and env_file.stat().st_size > 0):
            drain(ifd, screen, 1.0)
        tool_env = {}
        if env_file.exists():
            for item in env_file.read_text().split('\0'):
                if '=' in item:
                    k, v = item.split('=', 1)
                    tool_env[k] = v
        r['interactive_tool_shell_names'] = sorted(tool_env)
        r['interactive_tool_shell_entrypoint'] = tool_env.get('CLAUDE_CODE_ENTRYPOINT')
        if tool_env:
            nested_sid = str(uuid.uuid4())
            nested = Lead(ctx, 'P2', 'print-from-tool-shell',
                          ['--print', '--input-format=stream-json', '--output-format=stream-json', '--verbose',
                           '--session-id', nested_sid, '--model', PROBE_MODEL], env=tool_env, supervised=False)
            try:
                nested.initialize()
                ask(nested, 'Reply with exactly the word ready.', 180)
                r['print_from_tool_shell_record'] = session_record(ctx.p, session_id=nested_sid) or \
                    session_record(ctx.p, pid=nested.proc.pid)
            finally:
                nested.close()
        (ctx.p.results / 'P2-interactive-screen.txt').write_text(re.sub(r'\x1b\[[0-9;?]*[A-Za-z]', '', ''.join(screen))[-20000:])
        # ---- the agent: visible and unreaped while it runs, NOT-ALIVE after ------------------
        task_id, worktree, detail = spawn_teammate(ctx, lead, 'probe-sonnet-p2a', 'alpha', 60)
        r['spawn'] = detail
        if worktree:
            r['liveness_running'] = ctx.liveness(worktree)
            second = Lead(ctx, 'P2', 'second-session', ctx.lead_args())
            try:
                second.initialize()
                ask(second, 'Reply with exactly the word ready.', 180)
                texts = [a['text'] for a in second.alarms()]
                # Real signals only: a reaper saying it could not decide, or that it reaped
                # something. (Run 4's first version matched "the scheduled reaper is NOT
                # INSTALLED", which is a notice about cron, not about this agent.)
                r['second_session_alarms_mentioning_reap_or_indeterminate'] = [
                    t[:600] for t in texts
                    if re.search(r'INDETERMINATE|undecidable=[1-9]|reaped=[1-9]|reaped [1-9]|removed .*agent-', t)]
                r['second_session_alarm_count'] = len(texts)
            finally:
                second.close()
            r['worktree_still_there_after_second_session'] = any(w['worktree'] == worktree for w in ctx.worktrees())
            done = lead.wait(lambda f: f.get('type') == 'system' and f.get('subtype') == 'task_notification'
                             and f.get('task_id') == task_id, 240)
            r['agent_ended'] = bool(done) and done[2].get('status')
            time.sleep(5)
            r['liveness_after'] = ctx.liveness(worktree)
    finally:
        if ipid:
            end_interactive(ipid, ifd)
        desk.close()
        lead.close()
    lead_ep = (r.get('lead_record') or {}).get('entrypoint')
    ctl_ep = (r.get('interactive_record') or {}).get('entrypoint')
    if ctl_ep != 'cli':
        return 'PREMISE-FALSE', 'the interactive control recorded entrypoint %r, not cli' % ctl_ep
    problems = []
    if lead_ep != 'sdk-cli':
        problems.append('the lead recorded entrypoint %r' % lead_ep)
    if r.get('spawn', {}).get('task_id') is None:
        problems.append('no teammate could be started for the liveness half (see spawn)')
    else:
        if (r.get('liveness_running') or [None])[0] != 'ALIVE':
            problems.append('liveness while running: %s' % (r.get('liveness_running') or [None])[0])
        if (r.get('liveness_after') or [None])[0] != 'NOT-ALIVE':
            problems.append('liveness after: %s' % (r.get('liveness_after') or [None])[0])
        if r.get('second_session_alarms_mentioning_reap_or_indeterminate'):
            problems.append('the second session reported reaping or INDETERMINATE')
        if not r.get('worktree_still_there_after_second_session'):
            problems.append('the agent worktree was gone after the second session started')
    if problems:
        return 'FAIL', '; '.join(problems)
    return 'PASS', 'lead entrypoint sdk-cli, interactive cli; front desk record: %s' % (
        'present' if r.get('front_desk_record') else 'ABSENT')


def interrupt_run(ctx, r, key, declared):
    """One lead with one backgrounded teammate, mid-turn in a shell command, interrupted.
    `declared` is whether the initialize request says perTaskStopAffordance: true."""
    rec = r.setdefault(key, {'perTaskStopAffordance': declared})
    lead = Lead(ctx, 'P3', key, ctx.lead_args())
    try:
        rec['initialize'] = lead.initialize(**({'perTaskStopAffordance': True} if declared else {}))
        task_id, worktree, detail = spawn_teammate(ctx, lead, 'probe-sonnet-p3%s' % ('d' if declared else 'u'), 'alpha', 420)
        rec['spawn'] = detail
        if not worktree:
            rec['not_run'] = 'no teammate could be started (see spawn)'
            return rec
        start = lead.count()
        lead.user(lead_long_task(ctx, 90, 'slept'))
        rec['lead_in_shell'] = bool(in_lead_shell(ctx, lead, start))
        queued = lead.user('Reply with exactly: QUEUED-MARK')
        time.sleep(3)
        # The premise: the agent is running at the moment of the interrupt.
        rec['liveness_before_interrupt'] = ctx.liveness(worktree)[0]
        rec['interrupt_sent_at'] = round(time.time() - lead.started, 3)
        rid = lead.control('interrupt')
        reply = lead.wait(lambda f: f.get('type') == 'control_response' and
                          (f.get('response') or {}).get('request_id') == rid, 30)
        rec['interrupt_reply'] = redact(reply[2].get('response')) if reply else None
        rec['queued_uuid_still_queued'] = bool(reply) and queued in json.dumps(reply[2])
        ended = lead.result_after(start, 60)
        rec['turn_ended'] = bool(ended) and ended[2].get('subtype')
        time.sleep(5)
        rec['liveness_after_interrupt'] = ctx.liveness(worktree)
        rec['worktree_locked'] = any(w['worktree'] == worktree and 'locked' in w for w in ctx.worktrees())
        rec['agent_notified_stopped'] = [f.get('status') for f in lead.all_frames()
                                         if f.get('subtype') == 'task_notification' and f.get('task_id') == task_id]
        later = lead.wait(lambda f: f.get('type') == 'result', 45, (ended[0] + 1) if ended else start)
        rec['queued_message_answered'] = 'QUEUED-MARK' in lead.text(start)
        rec['queued_turn_ran'] = bool(later)
    finally:
        lead.close()
    rec['agent_survived'] = rec['liveness_after_interrupt'][0] == 'ALIVE' and rec['worktree_locked'] \
        and 'stopped' not in rec['agent_notified_stopped']
    return rec


@probe('P3')
def p3(ctx, r):
    # The operator client stops agents one by one with stop_task (d), so it declares the
    # per-task stop affordance; the 2.1.282 binary documents that an interrupt then "spares
    # running background agents/workflows (Stop only aborts the turn)". The control is the same
    # run without the declaration, which run 4 measured killing the agent.
    main = interrupt_run(ctx, r, 'declared', True)
    control = interrupt_run(ctx, r, 'control-undeclared', False)
    for rec in (main, control):
        if rec.get('not_run'):
            return 'NOT-RUN', rec['not_run']
        if not rec.get('lead_in_shell'):
            return 'PREMISE-FALSE', 'a lead never started its shell command, so the interrupt did not land mid-command'
        if rec.get('liveness_before_interrupt') != 'ALIVE':
            return 'PREMISE-FALSE', 'an agent was already %s before the interrupt' % rec.get('liveness_before_interrupt')
    if control['agent_survived']:
        return 'PREMISE-FALSE', 'without the declaration the agent survived too, so the declaration was not what spared it'
    if main['agent_survived'] and main['turn_ended']:
        return 'PASS', ('with perTaskStopAffordance declared the turn ended and the agent stayed ALIVE with its lock held; '
                        'without it the interrupt stopped the agent')
    return 'FAIL', 'declared run: liveness %s, locked %s, notifications %s, turn ended %s' % (
        main['liveness_after_interrupt'][0], main['worktree_locked'], main['agent_notified_stopped'], main['turn_ended'])


def two_teammates(ctx, lead, r, tag):
    a_id, a_wt, a_detail = spawn_teammate(ctx, lead, 'probe-sonnet-%sa' % tag, 'alpha', 480)
    b_id, b_wt, b_detail = spawn_teammate(ctx, lead, 'probe-sonnet-%sb' % tag, 'beta', 480)
    r['spawn_a'], r['spawn_b'] = a_detail, b_detail
    return (a_id, a_wt), (b_id, b_wt)


def wait_not_alive(ctx, worktree, seconds):
    began = time.time()
    last = None
    while time.time() - began < seconds:
        last = ctx.liveness(worktree)
        if last[0] == 'NOT-ALIVE':
            return round(time.time() - began, 2), last
        time.sleep(1)
    return None, last


@probe('P4')
def p4(ctx, r):
    lead = Lead(ctx, 'P4', 'lead', ctx.lead_args())
    try:
        lead.initialize()
        (a_id, a_wt), (b_id, b_wt) = two_teammates(ctx, lead, r, 'p4')
        if not (a_wt and b_wt):
            return 'NOT-RUN', 'two teammates could not be started (see spawn_a, spawn_b)'
        r['liveness_before'] = {'a': ctx.liveness(a_wt)[0], 'b': ctx.liveness(b_wt)[0]}
        if r['liveness_before'] != {'a': 'ALIVE', 'b': 'ALIVE'}:
            return 'PREMISE-FALSE', 'before the stop the agents were %s' % r['liveness_before']
        t0 = time.time()
        start = lead.count()
        lead.user(FIXTURE_NOTE + 'Stop the teammate named probe-sonnet-p4a, and ONLY that one: first run exactly this Bash command: '
                  '%s probe-sonnet-p4a --entity %s --ceo-word \'stop p4a\' ; then make the TaskStop call it prints. '
                  'Do not stop probe-sonnet-p4b. Then reply stopped.' % (ctx.p.engine / 'scripts' / 'stop.sh', ctx.p.entity))
        r['seconds_to_not_alive'], r['liveness_a'] = wait_not_alive(ctx, a_wt, 180)
        lead.result_after(start, 180)
        r['liveness_b'] = ctx.liveness(b_wt)
        r['lead_said'] = lead.text(start)[-1500:]
        r['taskstop_calls'] = [b.get('input') for f in lead.all_frames()[start:] if f.get('type') == 'assistant'
                               for b in (f.get('message') or {}).get('content') or []
                               if b.get('type') == 'tool_use' and b.get('name') == 'TaskStop']
        r['elapsed_total'] = round(time.time() - t0, 2)
    finally:
        lead.close()
    if not r['taskstop_calls']:
        return 'FAIL', 'the lead made no TaskStop call'
    if r['seconds_to_not_alive'] is not None and r['liveness_b'][0] == 'ALIVE':
        return 'PASS', 'NOT-ALIVE in %s s, the other agent ALIVE' % r['seconds_to_not_alive']
    return 'FAIL', 'a: %s after %s s, b: %s' % (r['liveness_a'][0], r['seconds_to_not_alive'], r['liveness_b'][0])


def grade_p5(rows, task_id, sent_until):
    """P5's pass line as r4 §1.1 and §5 state it (docs/plans/2026-09-25-operator-back-end-spec-r4.md):
    "a lead turn starts with nothing sent after the agent's task_notification, and SubagentStop
    fires". `rows` are (seconds, frame) in stream order; nothing was sent to the lead after
    `sent_until`. Pure, so the same function grades a live run and the recorded P5 frames.

    TeammateIdle and TaskCompleted are recorded and never required: r4 §1.1 measured that his
    terminal does not get them either, so nothing of his depends on them."""
    facts = {'task_id': task_id, 'sent_until': sent_until}
    notified = [t for t, f in rows if f.get('type') == 'system' and f.get('subtype') == 'task_notification'
                and f.get('task_id') == task_id and f.get('status') == 'completed']
    facts['notification_completed_at'] = notified[0] if notified else None
    facts['subagent_stop_at'] = [t for t, f in rows if f.get('type') == 'system' and f.get('subtype') == 'hook_response'
                                 and f.get('hook_event') == 'SubagentStop']
    facts['teammate_hooks_fired'] = sorted(set(f.get('hook_event') for _, f in rows if f.get('type') == 'system'
                                               and f.get('subtype') in ('hook_started', 'hook_response')
                                               and f.get('hook_event') in ('TeammateIdle', 'TaskCompleted')))
    if not notified:
        return 'FAIL', 'the host saw no task_notification completed for the agent', facts
    if notified[0] <= sent_until:
        return 'PREMISE-FALSE', 'the agent ended before the harness stopped sending, so a platform turn cannot be told apart', facts
    starts = [t for t, f in rows if t > notified[0] and f.get('type') == 'system' and f.get('subtype') == 'hook_started'
              and f.get('hook_event') == 'UserPromptSubmit']
    facts['turn_started_at'] = starts[0] if starts else None
    facts['seconds_notification_to_turn'] = round(starts[0] - notified[0], 3) if starts else None
    ended = [t for t, f in rows if starts and t > starts[0] and f.get('type') == 'result']
    facts['turn_result_at'] = ended[0] if ended else None
    if not starts or not ended:
        return 'FAIL', 'no lead turn started and ended after the task_notification, with nothing sent', facts
    if not facts['subagent_stop_at']:
        return 'FAIL', 'a platform turn started, but SubagentStop never fired, so his registry never saw the end', facts
    return 'PASS', ('task_notification completed at %s s; a lead turn started with nothing sent %s s later; SubagentStop '
                    'fired at %s' % (notified[0], facts['seconds_notification_to_turn'], facts['subagent_stop_at'])), facts


@probe('P5')
def p5(ctx, r):
    sid = str(uuid.uuid4())
    lead = Lead(ctx, 'P5', 'lead', ctx.lead_args(session_id=sid))
    try:
        lead.initialize(perTaskStopAffordance=True)
        task_id, worktree, detail = spawn_teammate(ctx, lead, 'probe-sonnet-p5a', 'alpha', 20)
        r['spawn'] = detail
        if not task_id:
            return 'NOT-RUN', 'no teammate could be started (see spawn)'
        # Nothing is sent from here on. A new turn must start by itself when the agent ends.
        sent_until = time.time() - lead.started
        mark = lead.count()
        lead.wait(lambda f: f.get('type') == 'system' and f.get('subtype') == 'task_notification'
                  and f.get('task_id') == task_id, 300, mark)
        lead.wait(lambda f: f.get('type') == 'result', 120, mark)
        r['session_team'] = session_team(ctx, sid)
    finally:
        lead.close()
    with lead.cond:
        rows = [(round(t - lead.started, 3), f) for t, f in lead.frames]
    verdict, why, facts = grade_p5(rows, task_id, round(sent_until, 3))
    r.update(facts)
    return verdict, why


def session_team(ctx, session_id):
    """The session team his terminal's lead gets: ~/.claude/teams/session-<first 8>/config.json,
    naming the lead's session (measured on the host: name, createdAt, leadAgentId,
    leadSessionId, members)."""
    path = ctx.p.claude_dir / 'teams' / ('session-%s' % session_id[:8]) / 'config.json'
    try:
        d = json.loads(path.read_text())
        return {'path': str(path), 'leadSessionId': d.get('leadSessionId'),
                'members': [m.get('name') for m in d.get('members') or []]}
    except (OSError, ValueError):
        return None


@probe('P6', retired='r4 §1.1: his terminal has no consumer of the session team\'s two hooks either, so the switch '
                     'has nothing to decide; the code stays as the record of what it measured')
def p6(ctx, r):
    # 1) As femcboost sets it: the switch comes from the entity's project settings.
    sid = str(uuid.uuid4())
    lead = Lead(ctx, 'P6', 'lead-project-settings', ctx.lead_args(session_id=sid))
    try:
        lead.initialize()
        task_id, worktree, detail = spawn_teammate(ctx, lead, 'probe-sonnet-p6a', 'alpha', 15)
        r['spawn'] = detail
        r['team_from_project_settings'] = session_team(ctx, sid)
        r['lead_tools_team'] = sorted(t for t in (lead.init_frame() or {}).get('tools') or []
                                      if t in ('TeamCreate', 'TeamDelete', 'SendMessage', 'TaskCreate', 'TaskUpdate'))
    finally:
        lead.close()
    # 2) r3's fallback: the switch in the lead's own environment.
    sid2 = str(uuid.uuid4())
    forced = Lead(ctx, 'P6', 'lead-switch-in-environment', ctx.lead_args(session_id=sid2),
                  env=dict(ctx.lead_env(), CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS='1'))
    try:
        forced.initialize()
        ask(forced, 'Reply with exactly the word ready.', 180)
        time.sleep(3)
        r['team_from_environment'] = session_team(ctx, sid2)
    finally:
        forced.close()
    # 3) The control: an interactive session in the same entity, as his terminal runs.
    ipid, ifd, record, screen = interactive_session(ctx, r)
    try:
        type_into(ifd, 'Reply with exactly the word ready.', screen)
        deadline = time.time() + 90
        team = None
        while time.time() < deadline and team is None:
            drain(ifd, screen, 1.0)
            team = session_team(ctx, (record or {}).get('sessionId') or '') if record else None
        r['interactive_record'] = record
        r['team_interactive'] = team
    finally:
        end_interactive(ipid, ifd)
    if not r['team_interactive']:
        return 'PREMISE-FALSE', 'the interactive control got no session team either, so the fixture cannot make one'
    if r['team_from_project_settings']:
        return 'PASS', 'the project-settings switch is honored in print mode: the lead got its session team'
    if r['team_from_environment']:
        return 'FAIL', ('the switch from project settings is NOT honored in print mode, and it IS when it is in the lead\'s '
                        'environment: it goes into the stored environment (r3 P6 fallback)')
    return 'FAIL', 'print mode created no session team with the switch from settings or from the environment'


def filler_files(ctx, count, kib):
    folder = ctx.p.work / 'p7-files'
    folder.mkdir(exist_ok=True)
    words = ['anchor', 'basalt', 'cinder', 'dune', 'ember', 'fjord', 'glacier', 'harbor', 'island', 'juniper']
    for i in range(count):
        path = folder / ('part-%03d.txt' % i)
        if path.exists():
            continue
        lines = []
        n = 0
        while n < kib * 1024:
            line = 'part %d line %d %s %s %s %d\n' % (i, len(lines), words[len(lines) % 10], words[(len(lines) * 7) % 10],
                                                        words[(len(lines) * 3 + i) % 10], (len(lines) * 7919 + i) % 100003)
            lines.append(line)
            n += len(line)
        path.write_text(''.join(lines))
    return folder


@probe('P7')
def p7(ctx, r):
    sid = str(uuid.uuid4())
    # 60 files of 60 KiB: each stays under the Read tool's per-call token cap (about 21k tokens
    # at the ~2.9 characters per token this text runs at), and together they pass 1M tokens,
    # so automatic compaction must happen before the last one is read.
    folder = filler_files(ctx, 60, 60)
    hook_log = ctx.p.work / 'p7-sessionstart.jsonl'
    settings = ctx.p.work / 'p7-settings.json'
    settings.write_text(json.dumps({'hooks': {'SessionStart': [{'hooks': [{'type': 'command',
        'command': '/bin/cat >> %s; echo >> %s' % (hook_log, hook_log)}]}]}}))
    lead = Lead(ctx, 'P7', 'lead', ctx.lead_args(session_id=sid, model=HIS_MODEL, extra=['--settings', str(settings)]))
    usage = []
    try:
        lead.initialize()
        start, got = do(lead, 'Read every file in %s with the Read tool, ten files per message in parallel, '
                               'until all 60 are read. Do not summarize them. Then reply with exactly: ALL-READ' % folder,
                         ctx.timeout * 3)
        compacted = [f for f in lead.all_frames() if f.get('type') == 'system' and
                     f.get('subtype') in ('compact_boundary', 'status') and 'compact' in json.dumps(f)]
        r['compaction_frames'] = compacted[:5]
        r['auto_compacted'] = any((f.get('compact_metadata') or {}).get('trigger') == 'auto' for f in compacted)
        s2, _ = ask(lead, 'Answer with only the two tokens, nothing else: the fixture watchword, and the fixture memory token.', 300)
        r['answer_after_compaction'] = lead.text(s2)[-300:]
        usage = [f.get('usage') for f in lead.all_frames() if f.get('type') == 'result']
    finally:
        lead.close()
    r['usage_per_turn'] = usage
    resumed = Lead(ctx, 'P7', 'resumed', ctx.lead_args(resume=sid, model=HIS_MODEL, extra=['--settings', str(settings)]))
    try:
        resumed.initialize()
        s3, _ = ask(resumed, 'What was the exact word you replied with after reading the files? Reply with only it.', 300)
        r['answer_after_resume'] = resumed.text(s3)[-300:]
    finally:
        resumed.close()
    sources = []
    if hook_log.exists():
        for line in hook_log.read_text().splitlines():
            try:
                sources.append(json.loads(line).get('source'))
            except ValueError:
                pass
    r['sessionstart_sources'] = sources
    both = 'PERIWINKLE-4417' in r['answer_after_compaction'] and 'AMBER-2291' in r['answer_after_compaction']
    if not r['auto_compacted']:
        return 'FAIL', 'no automatic compaction was observed at this context size'
    if not both or 'resume' not in sources or 'ALL-READ' not in r['answer_after_resume']:
        return 'FAIL', 'after compaction/resume: tokens %s, sources %s, resumed answer %r' % (
            both, sources, r['answer_after_resume'][:60])
    return 'PASS', 'compacted automatically, both lines survived, resumed with source resume'


def shell_names(ctx, lead, root_pid, seconds=23):
    """Have the lead's tool shell run a short timer and read that process's variable names from
    the kernel. The model is asked for nothing but a wait; run 4 showed a lead rightly refusing
    to list its environment on request.

    The timer is node, not sleep: macOS withholds the environment of Apple binaries (sleep and
    the shell itself), and node, started by the shell, carries the shell's exported environment
    untouched."""
    marker = 'setTimeout(function(){}, %d000)' % seconds
    start = lead.count()
    lead.user(FIXTURE_NOTE + 'Run exactly this Bash command in the foreground and wait for it to finish '
                             '(it waits %d seconds and prints nothing): %s -e "%s". Then reply waited.'
              % (seconds, ctx.node, marker))
    names, deadline = None, time.time() + 120
    while names is None and time.time() < deadline:
        for pid, command in descendants(root_pid):
            if marker in command and 'node' in command.split()[0]:
                names = environ_names(pid)
                break
        if names is None:
            time.sleep(0.5)
    lead.result_after(start, 180)
    return names


@probe('P8')
def p8(ctx, r):
    hook_out = ctx.p.work / 'p8-hook-tools.txt'
    settings = ctx.p.work / 'p8-settings.json'
    # `command -v` from inside a hook of the lead: the harness's own hook, writing to scratch.
    settings.write_text(json.dumps({'hooks': {'PreToolUse': [{'matcher': 'Bash', 'hooks': [{'type': 'command',
        'command': 'for t in python3 git gh java; do command -v $t; done > %s' % hook_out}]}]}}))
    env = ctx.lead_env()
    # The "app's environment" carries a planted key; the lead's is built from empty, so it must
    # not arrive anywhere. (This harness process is the app's stand-in.)
    os.environ['ANTHROPIC_API_KEY'] = 'planted-by-the-probe-not-a-key'
    lead = Lead(ctx, 'P8', 'lead', ctx.lead_args(extra=['--settings', str(settings)]), env=env)
    try:
        lead.initialize()
        r['claude_pid'] = lead.claude_pid()
        r['lead_process_names'] = environ_names(r['claude_pid']) if r['claude_pid'] else None
        r['lead_tool_shell_names'] = shell_names(ctx, lead, r['claude_pid'])
        facts_text, start = bash_output(lead, 'Run this Bash command and show its output: '
                                              'mktemp && echo "CLAUDE_PID=$CLAUDE_PID"')
        push_text, _ = bash_output(lead, 'Push the current commit to a new branch on the fixture remote with this '
                                         'Bash command and show its output: git push origin HEAD:refs/heads/probe-p8')
    finally:
        lead.close()
        os.environ.pop('ANTHROPIC_API_KEY', None)
    lines = (facts_text or '').split()
    r['tmpfile'] = next((l for l in lines if l.startswith('/')), None)
    r['tmpfile_under_tmpdir'] = bool(r['tmpfile']) and r['tmpfile'].startswith(ctx.tmpdir.rstrip('/'))
    reported_pid = next((l.split('=', 1)[1] for l in lines if l.startswith('CLAUDE_PID=')), None)
    r['claude_pid_reported'] = reported_pid
    r['claude_pid_matches'] = reported_pid == str(r.get('claude_pid'))
    code, _, _ = run(['git', '--git-dir', str(ctx.p.remote), 'rev-parse', '--verify', '--quiet', 'refs/heads/probe-p8'])
    r['push'] = 'ok' if code == 0 else 'failed'
    r['push_said'] = (push_text or '')[-300:]
    code, out, _ = run(['/bin/zsh', '-lc', 'for t in python3 git gh java; do command -v $t; done'], timeout=60)
    r['terminal_tools'] = out.split()
    r['hook_tools'] = hook_out.read_text().split() if hook_out.exists() else None
    # ---- the control: a claude started with almost nothing, to learn what claude and its
    #      shell set by themselves -----------------------------------------------------------
    bare = {'HOME': str(ctx.p.home), 'PATH': ctx.stored['PATH']}
    bare.update(HARNESS_PIN)
    control = Lead(ctx, 'P8', 'control-bare', ctx.lead_args(), env=bare, supervised=False)
    try:
        control.initialize()
        control_shell = shell_names(ctx, control, control.proc.pid)
    finally:
        control.close()
    r['control_tool_shell_names'] = control_shell
    claude_own = sorted(set(control_shell or []) - set(bare))
    r['names_claude_or_its_shell_set_itself'] = claude_own
    # ---- the lead process: exactly what the app handed the supervisor, plus what the
    #      supervisor adds (r3 §11 item 3) --------------------------------------------------
    expected = sorted(set(k for k in ALLOWLIST if k in env) | set(APP_NAMES) | set(SUPERVISOR_NAMES) |
                      set(HARNESS_PIN))
    r['expected_names'] = expected
    process = set(r['lead_process_names'] or [])
    r['process_extra'] = sorted(process - set(expected))
    r['process_extra_that_the_interpreter_adds'] = sorted((process - set(expected)) & set(INTERPRETER_NAMES))
    r['process_missing'] = sorted(set(expected) - process)
    # ---- the tool shell, minus what claude and its shell add by themselves ---------------
    shell = set(r['lead_tool_shell_names'] or []) - set(claude_own) - {'PWD', 'OLDPWD', 'SHLVL', '_'}
    r['shell_measured_minus_claude_own'] = sorted(shell)
    r['shell_extra'] = sorted(shell - set(expected) - set(INTERPRETER_NAMES))
    r['shell_missing'] = sorted(set(expected) - shell - set(claude_own))
    r['planted_key_reached_lead'] = 'ANTHROPIC_API_KEY' in process or 'ANTHROPIC_API_KEY' in (r['lead_tool_shell_names'] or [])
    r['ssh_auth_sock_method'] = ctx.ssh_method
    if r['lead_process_names'] is None or r['lead_tool_shell_names'] is None:
        return 'PREMISE-FALSE', 'the environment of the lead or of its tool shell could not be read'
    if control_shell is None:
        return 'PREMISE-FALSE', 'the control session\'s tool shell could not be read, so claude\'s own names are unknown'
    problems = []
    if r['push'] != 'ok':
        problems.append('the push did not reach the fixture remote')
    if not r['tmpfile_under_tmpdir']:
        problems.append('mktemp gave %s, not a path under the derived TMPDIR %s' % (r['tmpfile'], ctx.tmpdir))
    if not r['claude_pid_matches']:
        problems.append('CLAUDE_PID %s is not the lead pid %s' % (reported_pid, r.get('claude_pid')))
    if r['hook_tools'] != r['terminal_tools']:
        problems.append('tools in a hook %s differ from the terminal %s' % (r['hook_tools'], r['terminal_tools']))
    if r['planted_key_reached_lead']:
        problems.append('the planted ANTHROPIC_API_KEY reached the lead')
    if r['process_missing'] or set(r['process_extra']) - set(INTERPRETER_NAMES):
        problems.append('lead process set equality: extra %s, missing %s' % (r['process_extra'], r['process_missing']))
    if r['shell_extra'] or r['shell_missing']:
        problems.append('tool shell set equality: extra %s, missing %s' % (r['shell_extra'], r['shell_missing']))
    if problems:
        return 'FAIL', '; '.join(problems)
    return 'PASS', ('set equality held for the lead process and its tool shell%s; push ok; TMPDIR and CLAUDE_PID right'
                    % (' (plus the interpreter\'s own %s)' % r['process_extra_that_the_interpreter_adds']
                       if r['process_extra_that_the_interpreter_adds'] else ''))


@probe('P9')
def p9(ctx, r):
    q = 'Answer from what you already know, without looking anything up: the fixture watchword, and the fixture ' \
        'memory token. Reply with only the two tokens, nothing else. If you do not know one, write UNKNOWN in its place.'
    # Run 2 showed a control finding the CLAUDE.md token by searching the fixture with its
    # tools. What is being measured is what LOADED, so no run of this probe may look anything up.
    no_lookup = ('AskUserQuestion', 'Bash', 'Read', 'Grep', 'Glob', 'Task', 'Agent', 'WebFetch', 'WebSearch',
                 'NotebookRead', 'ToolSearch')
    r['tools_withheld_in_every_run'] = list(no_lookup)
    lead = Lead(ctx, 'P9', 'lead', ctx.lead_args(disallow=no_lookup))
    try:
        lead.initialize()
        s, _ = ask(lead, q, 180)
        r['answer'] = lead.text(s)[-300:]
    finally:
        lead.close()
    # r3's control. Run 1 (2026-09-25) found it answers both tokens: --setting-sources '' does
    # not stop CLAUDE.md or auto memory from loading. It is kept and recorded as that finding.
    control = Lead(ctx, 'P9', 'control-r3-no-setting-sources', ctx.lead_args(sources='', disallow=no_lookup))
    try:
        control.initialize()
        s, _ = ask(control, q, 180)
        r['r3_control_answer'] = control.text(s)[-300:]
    finally:
        control.close()
    # A control that can fail: what the product front desk does to keep them out
    # (engine_profile.rs: claudeMdExcludes and autoMemoryEnabled false, plus the variable).
    excludes = json.dumps({'claudeMdExcludes': ['**/CLAUDE.md', '**/CLAUDE.local.md', '**/.claude/rules/**'],
                           'autoMemoryEnabled': False})
    env = dict(ctx.lead_env(), CLAUDE_CODE_DISABLE_AUTO_MEMORY='1')
    working = Lead(ctx, 'P9', 'control-excluded', ctx.lead_args(sources='', extra=['--settings', excludes],
                                                                disallow=no_lookup), env=env)
    try:
        working.initialize()
        s, _ = ask(working, q, 180)
        r['control_answer'] = working.text(s)[-300:]
    finally:
        working.close()
    tokens = ('PERIWINKLE-4417', 'AMBER-2291')
    r['r3_control_answered'] = [t for t in tokens if t in r['r3_control_answer']]
    if any(t in r['control_answer'] for t in tokens):
        return 'PREMISE-FALSE', 'even with both excluded the control answered a token'
    if all(t in r['answer'] for t in tokens):
        return 'PASS', ('both lines answered natively; the excluded control answered neither. r3\'s own control '
                        '(--setting-sources \'\') answered %s, so it cannot serve as a control' % r['r3_control_answered'])
    return 'FAIL', 'the lead answered %r' % r['answer'][:120]


@probe('P10')
def p10(ctx, r):
    settings = ctx.p.work / 'p10-settings.json'
    def say(marker):
        return {'type': 'command', 'command': 'echo \'{"systemMessage":"%s"}\'' % marker}
    settings.write_text(json.dumps({'hooks': {
        'SessionStart': [{'hooks': [say('PROBE-P10-SESSIONSTART')]}],
        'PreToolUse': [{'matcher': 'Bash', 'hooks': [say('PROBE-P10-PRETOOLUSE')]}],
        'Stop': [{'hooks': [say('PROBE-P10-STOP'),
                            {'type': 'command', 'command': 'echo PROBE-P10-STDERR >&2; exit 0'}]}]}}))
    lead = Lead(ctx, 'P10', 'lead', ctx.lead_args(extra=['--settings', str(settings)]))
    try:
        lead.initialize()
        ask(lead, 'Run the Bash command true, then reply with exactly: ok', 180)
        time.sleep(3)
        alarms = lead.alarms()
        frames = lead.all_frames()
    finally:
        lead.close()
    r['alarms'] = [{'text': a['text'][:300], 'frame': a['frame'], 'hook_event': a.get('hook_event'),
                    'level': a.get('level')} for a in alarms]
    r['hook_frame_shapes'] = sorted(set('%s:%s' % (f.get('subtype'), ','.join(sorted(f.keys())))
                                        for f in frames if f.get('type') == 'system'
                                        and f.get('subtype') in ('hook_started', 'hook_progress', 'hook_response',
                                                                 'informational')))
    r['meta_assistant_frames'] = sum(1 for f in frames if f.get('type') == 'assistant' and f.get('is_meta'))
    def seen(marker, frame=None):
        return any(marker in a['text'] and (frame is None or a['frame'] == frame) for a in alarms)
    r['sessionstart'] = seen('PROBE-P10-SESSIONSTART')
    r['pretooluse'] = seen('PROBE-P10-PRETOOLUSE')
    r['stop_hook_response'] = seen('PROBE-P10-STOP', 'hook_response')
    r['stop_informational'] = seen('PROBE-P10-STOP', 'informational')
    r['stderr_as_alarm'] = seen('PROBE-P10-STDERR')
    r['stderr_anywhere_on_the_wire'] = any('PROBE-P10-STDERR' in json.dumps(f) for f in frames)
    r['banner'] = next((b for b in (parse_banner(dedupe_key(a['text'])) for a in alarms) if b), None)
    if r['stderr_as_alarm']:
        return 'PREMISE-FALSE', 'the stderr control appeared as an alarm'
    ok = r['sessionstart'] and r['pretooluse'] and r['stop_hook_response'] and r['stop_informational'] and r['banner']
    if ok:
        return 'PASS', 'SessionStart, PreToolUse and Stop arrived; Stop also as informational; banner readable'
    return 'FAIL', 'missing: %s' % [k for k in ('sessionstart', 'pretooluse', 'stop_hook_response',
                                               'stop_informational', 'banner') if not r[k]]


@probe('P11')
def p11(ctx, r):
    lead = Lead(ctx, 'P11', 'lead', ctx.lead_args())
    try:
        lead.initialize()
        ask(lead, 'Reply with exactly the word ready.', 180)
        lead_tools = sorted((lead.init_frame() or {}).get('tools') or [])
    finally:
        lead.close()
    r['lead_tools'] = lead_tools
    ipid, ifd, record, screen = interactive_session(ctx, r)
    tools_out = ctx.p.work / 'p11-interactive-tools.txt'
    try:
        type_into(ifd, 'Write the exact name of every tool you can call, one per line and nothing else, into the '
                       'file %s using the Write tool. Then reply done.' % tools_out, screen)
        deadline = time.time() + 240
        while time.time() < deadline and not (tools_out.exists() and tools_out.stat().st_size > 0):
            drain(ifd, screen, 1.0)
        time.sleep(3)
    finally:
        end_interactive(ipid, ifd)
    interactive = names_file(tools_out) or []
    r['interactive_tools_model_reported'] = interactive
    r['missing_from_lead'] = sorted(set(interactive) - set(lead_tools))
    r['only_in_lead'] = sorted(set(lead_tools) - set(interactive))
    # Dispositions stated from evidence in these runs; anything else goes to Rich.
    disposition = {
        'AskUserQuestion': 'replaced by report(question): disallowed on purpose, spec (n)',
        'Agent': 'not missing: system/init lists the same tool as Task, and the lead called Agent in P2-P12',
        'SendFeedback': 'not needed: it sends product feedback to Anthropic and plays no part in his team\'s work',
    }
    r['dispositions'] = {t: disposition.get(t, 'NEEDS A DISPOSITION FROM RICH') for t in r['missing_from_lead']}
    if not interactive:
        return 'PREMISE-FALSE', 'the interactive session did not list its tools'
    return 'RECORDED', '%d tools only in his terminal, each with a disposition in the result' % len(r['missing_from_lead'])


def workspaces(ctx, session_id, *args):
    """His engine's registry command, from the guest's engine root, with RICHOS_SESSION_ID
    naming the lead's session: the host is not the lead's descendant, so the ancestor walk
    cannot find it (r4 §2.1, workspaces.py current_session)."""
    env = dict(ctx.stored, HOME=str(ctx.p.home), CLAUDE_PROJECT_DIR=str(ctx.p.entity), RICHOS_SESSION_ID=session_id)
    code, out, err = run(['/bin/bash', str(ctx.p.engine / 'scripts' / 'workspaces.sh')] + list(args),
                         env=env, cwd=str(ctx.p.entity), timeout=120)
    return code, out, err


@probe('P12')
def p12(ctx, r):
    sid = str(uuid.uuid4())
    r['lead_session'] = sid
    lead = Lead(ctx, 'P12', 'lead', ctx.lead_args(session_id=sid))
    try:
        # Declared as the app's operator client will declare it: it renders a per-task stop.
        r['initialize'] = lead.initialize(perTaskStopAffordance=True)
        (a_id, a_wt), (b_id, b_wt) = two_teammates(ctx, lead, r, 'p12')
        r['task_id_map'] = {'probe-sonnet-p12a': a_id, 'probe-sonnet-p12b': b_id}
        r['mapping_frame'] = 'system/task_started.tool_use_id -> the Agent tool_use block whose input.name is the teammate'
        if not (a_id and b_id and a_wt and b_wt):
            return 'NOT-RUN', 'two teammates could not be started or mapped (see spawn_a, spawn_b)'
        start = lead.count()
        lead.user(lead_long_task(ctx, 60, 'slept'))
        r['lead_in_shell'] = bool(in_lead_shell(ctx, lead, start))
        time.sleep(3)
        r['liveness_before'] = {'a': ctx.liveness(a_wt)[0], 'b': ctx.liveness(b_wt)[0]}
        if r['liveness_before'] != {'a': 'ALIVE', 'b': 'ALIVE'}:
            return 'PREMISE-FALSE', 'before stop_task the agents were %s' % r['liveness_before']
        rid = lead.control('stop_task', task_id=a_id)
        reply = lead.wait(lambda f: f.get('type') == 'control_response' and
                          (f.get('response') or {}).get('request_id') == rid, 30)
        r['stop_task_reply'] = redact(reply[2]) if reply else None
        r['seconds_to_not_alive'], r['liveness_a'] = wait_not_alive(ctx, a_wt, 120)
        r['liveness_b'] = ctx.liveness(b_wt)
        # P12′ (r4 §2.1, §5): the host's registry step, taken the moment the stream says stopped
        # and the resolver says NOT-ALIVE — when the host takes it — never a minute later: by
        # then the lead's own engine may have closed a clean agent itself (run 2026-09-25 11:26,
        # "its work was landed" after the platform removed the clean worktree).
        r['registry_a'] = registry_step(ctx, lead, sid, 'probe-sonnet-p12a', a_id, a_wt, b_wt)
        done = lead.result_after(start, 300)
        r['lead_turn_result'] = done[2].get('subtype') if done else None
        r['lead_said'] = lead.text(start)[-400:]
        r['notification_a'] = [f.get('status') for f in lead.all_frames()
                               if f.get('subtype') == 'task_notification' and f.get('task_id') == a_id]
        # ---- the realistic case: an agent whose workspace has an uncommitted change --------
        # Run 6 showed NOT-ALIVE decided only by the platform deleting a CLEAN worktree; the
        # registry still said RUNNING. Claude Code keeps a worktree with changes, so this is
        # the case where the resolver would need another signal.
        c_id, c_wt, c_detail = spawn_teammate(ctx, lead, 'probe-sonnet-p12c', 'alpha', 480)
        r['spawn_c'] = c_detail
        if c_id and c_wt:
            # What matters to the platform at the stop is that the workspace HOLDS a change, not
            # who wrote it (final run: a teammate briefed to write it did not), so the harness
            # writes it into the agent's own worktree.
            (Path(c_wt) / 'probe-change.txt').write_text('change\n')
            code, out, _ = run(['git', '-C', c_wt, 'status', '--porcelain'], timeout=30)
            r['dirty_before_stop'] = 'probe-change.txt' in out
            r['liveness_c_before'] = ctx.liveness(c_wt)[0]
            rid = lead.control('stop_task', task_id=c_id)
            lead.wait(lambda f: f.get('type') == 'control_response' and
                      (f.get('response') or {}).get('request_id') == rid, 30)
            r['notification_c'] = None
            got = lead.wait(lambda f: f.get('subtype') == 'task_notification' and f.get('task_id') == c_id, 30)
            r['notification_c'] = got[2].get('status') if got else None
            r['seconds_to_not_alive_c'], r['liveness_c'] = wait_not_alive(ctx, c_wt, 60)
            r['worktree_c_after'] = [w for w in ctx.worktrees() if w['worktree'] == c_wt]
            if r['seconds_to_not_alive_c'] is not None:
                r['registry_c'] = registry_step(ctx, lead, sid, 'probe-sonnet-p12c', c_id, c_wt, b_wt)
        # What the lead's own engine had recorded for a by the end (informational).
        r['registry_a_at_end'] = registry_of(ctx.liveness(a_wt))
    finally:
        lead.close()
    detail = (r.get('liveness_a') or [None, {}])[1]
    r['signal_that_decided'] = json.dumps(detail)[:1500]
    r['signal_that_decided_c'] = json.dumps((r.get('liveness_c') or [None, {}])[1])[:1500]
    if not r['lead_in_shell']:
        return 'PREMISE-FALSE', 'the lead never started its shell command, so stop_task did not land mid-turn'
    ok = r['seconds_to_not_alive'] is not None and r['liveness_b'][0] == 'ALIVE' and 'slept' in r['lead_said']
    if not ok:
        return 'FAIL', 'a: %s after %s s; b: %s; lead said %r' % ((r.get('liveness_a') or [None])[0], r['seconds_to_not_alive'],
                                                                  r['liveness_b'][0], r['lead_said'][-80:])
    verdict, registry_note = grade_registry_step(r.get('registry_a') or {})
    if verdict == 'PASS' and r.get('registry_c'):
        verdict_c, note_c = grade_registry_step(r['registry_c'])
        verdict, registry_note = (verdict_c, 'a: %s; c (with a change): %s' % (registry_note, note_c))
    if verdict != 'PASS':
        return verdict, registry_note
    if not r.get('dirty_before_stop') or r.get('liveness_c_before') != 'ALIVE':
        return 'PASS', ('clean worktree: NOT-ALIVE in %s s, the other ALIVE, the lead\'s command finished. The dirty-'
                        'worktree case could not be set up (see spawn_c), so it is NOT measured; %s' % (r['seconds_to_not_alive'],
                                                                                          registry_note))
    if r['seconds_to_not_alive_c'] is None:
        return 'FAIL', ('stop_task works (clean worktree NOT-ALIVE in %s s, the other ALIVE, the lead\'s command finished), '
                        'but a stopped agent whose worktree holds a change still reads %s after 60 s: the platform keeps '
                        'the locked worktree and the registry never learns of the stop'
                        % (r['seconds_to_not_alive'], (r.get('liveness_c') or [None])[0]))
    return 'PASS', ('NOT-ALIVE in %s s (clean) and %s s (with a change); the other ALIVE; the lead\'s command finished; %s'
                    % (r['seconds_to_not_alive'], r['seconds_to_not_alive_c'], registry_note))


def registry_of(liveness):
    """(verdict, registry_finished, registry_why) from one agent-liveness.sh --json answer."""
    verdict, detail = liveness
    evidence = (detail or {}).get('evidence') or {}
    return [verdict, evidence.get('registry_finished'), evidence.get('registry_why')]


def registry_step(ctx, lead, session_id, name, task_id, worktree, other_worktree):
    """r4 §2.1 item 3a as the host does it: wait for the stream's `stopped` for that task, read
    the registry (the control), run `RICHOS_SESSION_ID=<lead session> workspaces.sh stop <name>
    --why '<words>'`, read it again for the stopped agent and the other one."""
    got = lead.wait(lambda f: f.get('subtype') == 'task_notification' and f.get('task_id') == task_id
                    and f.get('status') == 'stopped', 10)
    step = {'stream_said_stopped': bool(got),
            'before': registry_of(ctx.liveness(worktree)), 'other_before': registry_of(ctx.liveness(other_worktree))}
    code, out, err = workspaces(ctx, session_id, 'stop', name, '--why', 'operator probe P12: stop %s' % name)
    step['run'] = {'exit': code, 'said': (out + err).strip()[-400:]}
    step['after'] = registry_of(ctx.liveness(worktree))
    step['other_after'] = registry_of(ctx.liveness(other_worktree))
    return step


def grade_registry_step(step):
    """P12′'s added line (r4 §5). Control: when the stream has said stopped and the resolver says
    NOT-ALIVE, the registry still reads the run as open. Pass: after the step, the stopped agent
    is NOT-ALIVE and finished as stopped; the other is ALIVE with its run still open."""
    before, after, other = step.get('before') or [None] * 3, step.get('after') or [None] * 3, step.get('other_after') or [None] * 3
    if not step.get('stream_said_stopped'):
        return 'PREMISE-FALSE', 'the stream never said stopped for the task, so the host would not take the step'
    if before[1] is not False or not str(before[2] or '').startswith('its run has not ended'):
        return 'PREMISE-FALSE', ('before the registry step the registry read %s, not an open run, so the step is not what '
                                 'closed it' % (before,))
    if (step.get('run') or {}).get('exit') != 0:
        return 'FAIL', 'the registry step did not run cleanly: %s' % step.get('run')
    if after[0] == 'NOT-ALIVE' and after[1] is True and str(after[2] or '').startswith('it was stopped') and \
            other[0] == 'ALIVE' and other[1] is False:
        return 'PASS', 'registry step: before %s; after %s; the other %s' % (before, after, other)
    return 'FAIL', 'after the registry step: %s; the other %s' % (after, other)


def priority_order(ctx, r, key, use_priority):
    """The order the CLI TAKES the three messages in, read off its own replay of each one
    (--replay-user-messages echoes a user message, by its uuid, when it is dequeued), not off
    the model's replies: the final run showed three queued messages answered in one turn."""
    rec = r.setdefault(key, {})
    lead = Lead(ctx, 'P13', key, ctx.lead_args())  # the profile carries --replay-user-messages now
    try:
        lead.initialize()
        start = lead.count()
        lead.user(lead_long_task(ctx, 30, 'with exactly: A-DONE'))
        rec['lead_in_shell'] = bool(in_lead_shell(ctx, lead, start))
        uuids = {
            'MARK-B': lead.user('Reply with exactly: MARK-B', priority='later' if use_priority else None),
            'MARK-C': lead.user('Reply with exactly: MARK-C', priority='later' if use_priority else None),
            'MARK-D': lead.user('Reply with exactly: MARK-D', priority='now' if use_priority else None),
        }
        by_uuid = {v: k for k, v in uuids.items()}
        deadline = time.time() + 300
        taken = []
        while time.time() < deadline and len(taken) < 3:
            taken = []
            for f in lead.all_frames()[start:]:
                if f.get('type') == 'user' and f.get('uuid') in by_uuid and by_uuid[f['uuid']] not in taken:
                    taken.append(by_uuid[f['uuid']])
            time.sleep(1)
        lead.result_after(lead.count(), 60)
        time.sleep(3)
        rec['taken_order'] = taken
        rec['replies'] = lead.text(start)[-300:]
        own = [b for b in tool_uses(lead, start, 'Bash') if 'long-task.py' in json.dumps(b.get('input'))]
        result = tool_result_text(lead, own[0].get('id')) if own else None
        rec['own_command_result'] = (result or '')[:300]
        rec['aborted_running_turn'] = bool(result) and 'long task: done' not in result
    finally:
        lead.close()
    return rec


@probe('P13')
def p13(ctx, r):
    main = priority_order(ctx, r, 'with-priority', True)
    control = priority_order(ctx, r, 'control-no-priority', False)
    if not control['lead_in_shell'] or control['taken_order'] != ['MARK-B', 'MARK-C', 'MARK-D'] or \
            control['aborted_running_turn']:
        return 'PREMISE-FALSE', ('without priority the CLI did not take the messages first-come-first-served behind a '
                                 'finished turn: %s' % control)
    if not main['lead_in_shell'] or len(main['taken_order']) != 3:
        return 'PREMISE-FALSE', 'with priority the CLI did not take all three: %s' % main
    first = main['taken_order'][0] == 'MARK-D'
    if first and not main['aborted_running_turn']:
        return 'PASS', 'now was taken before two later messages, after the running turn finished: %s' % main['taken_order']
    if first:
        return 'FAIL', ('now was taken first by ABORTING the running turn: the lead\'s own foreground command got the '
                        'interrupt (%r). It jumps the queue by interrupting the lead, which (d) forbids for a named stop. '
                        'Control, without priority: %s' % (main['own_command_result'][:90], control['taken_order']))
    return 'FAIL', 'now was not taken first: %s; control %s' % (main['taken_order'], control['taken_order'])


@probe('P14')
def p14(ctx, r):
    # Every question the platform still asks is answered NO. A write the platform does NOT ask
    # about goes through (run 3 wrote into .git/info unasked), which is itself the measurement,
    # so each target's modification time is taken before and after.
    targets = (ctx.p.entity / '.git' / 'info' / 'probe-note.txt',
               ctx.p.entity / '.claude' / 'settings.local.json',
               ctx.p.entity / '.vscode' / 'settings.json')
    before = {str(x): (x.stat().st_mtime if x.exists() else None) for x in targets}
    lead = Lead(ctx, 'P14', 'lead', ctx.lead_args(), permission='deny')
    try:
        lead.initialize()
        # Three writes Claude Code is known to guard: inside .git, a project settings file,
        # and an editor settings file. Each is scratch in this fixture.
        start, got = do(lead, (
            'Make these three tool calls, one each, then reply done. It is expected that some of them are refused; '
            'that refusal is what the probe records. 1) Use the Write tool to create %s containing: probe note. '
            '2) Use the Write tool to rewrite %s with exactly the content it already has: {"env": {"CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"}}. '
            '3) Use the Write tool to create %s containing: {}.') % (
            ctx.p.entity / '.git' / 'info' / 'probe-note.txt',
            ctx.p.entity / '.claude' / 'settings.local.json',
            ctx.p.entity / '.vscode' / 'settings.json'), 300)
        r['lead_said'] = lead.text(start)[-800:]
        r['tool_calls'] = [(b.get('name'), (b.get('input') or {}).get('file_path'))
                           for f in lead.all_frames()[start:] if f.get('type') == 'assistant'
                           for b in (f.get('message') or {}).get('content') or [] if b.get('type') == 'tool_use']
    finally:
        lead.close()
    r['requests_that_reached_the_host'] = lead.requests
    r['written_without_asking'] = {str(x): (x.exists() and (x.stat().st_mtime != before[str(x)])) for x in targets}
    if not r['tool_calls']:
        return 'PREMISE-FALSE', 'the lead made none of the three calls, so nothing could reach the host'
    return 'RECORDED', '%d control request(s) reached the host: %s' % (
        len(lead.requests), sorted(set('%s/%s' % (q['subtype'], q['tool_name']) for q in lead.requests)))


@probe('P15')
def p15(ctx, r):
    reaps = ctx.supervisor_reaps()
    r['supervisor_has_reap_descendants'] = reaps
    # Two triggers: the app's death (the owner stand-in is killed), and, Frank's G9 on r3, the
    # LEAD's own crash (SIGKILL by its recorded pid). Both run under today's supervisor as the
    # controls; both run again under --reap-descendants when the engine has it.
    runs = [('control-no-reap', False, 'owner'), ('control-no-reap-lead-crash', False, 'lead')]
    if reaps:
        runs += [('reap', True, 'owner'), ('reap-lead-crash', True, 'lead')]
    for key, reap, trigger in runs:
        rec = r.setdefault(key, {})
        lead = Lead(ctx, 'P15', key, ctx.lead_args(), reap=reap, owner=True)
        recorded, groups = [], set()
        try:
            # Anything left from an earlier run would be read as this run's; the finally below
            # ends each run's own groups, so this is expected to be empty.
            rec['long_tasks_before'] = long_tasks(process_rows())
            lead.initialize()
            rec['supervisor_pid'] = lead.supervisor_pid()
            rec['supervisor_pgid'] = pgid_of(rec['supervisor_pid']) if rec['supervisor_pid'] else None
            # Asked the way a teammate is briefed, with the reason both must run at once. The
            # 2026-09-25 runs asked the lead to write its own pids through a subshell that
            # orphaned a grandchild, under "carry it out exactly as written"; three of four
            # leads read that as an attack and declined (P15-control-no-reap.jsonl 48.9 s). This
            # harness finds the processes itself, among the lead's own descendants.
            lead.user(P15_NOTE + 'First, start this stand-in for a long build with the Bash tool\'s run_in_background '
                      'set to true, and reply started: %s' % long_task(ctx, 600))
            rec['claude_pid'] = lead.claude_pid()
            rec['lead_pgid'] = pgid_of(rec['claude_pid']) if rec['claude_pid'] else None
            bg = wait_long_task(rec['claude_pid'], 600, 120)
            lead.user('Now run this one in the foreground, with the Bash tool\'s timeout parameter set to 600000, and '
                      'wait for it while the first keeps running: %s' % long_task(ctx, 500))
            fg = wait_long_task(rec['claude_pid'], 500, 120)
            # One plain re-ask if either is missing, recorded; a half that still has none is
            # PREMISE-FALSE in the grade, never a pass.
            rec['re_asked'] = False
            if not (bg and fg):
                rec['re_asked'] = True
                lead.user('This run measures what happens to a background command and a foreground command when the '
                          'session ends, so both need to be running at the same time. Please start whichever of the '
                          'two above is not running yet: the 600-second one in the background, the 500-second one in '
                          'the foreground.')
                bg = bg or wait_long_task(rec['claude_pid'], 600, 120)
                fg = fg or wait_long_task(rec['claude_pid'], 500, 120)
            rows = process_rows()
            for label, task in (('bg', bg), ('fg', fg)):
                rec[label] = tool_group(rows, task) if task else []
                for member in rec[label]:
                    recorded.append(('%s-%s-%d' % (label, member['role'], member['pid']), member['pid']))
                    groups.add(member['pgid'])
            if rec['claude_pid']:
                recorded.append(('lead', rec['claude_pid']))
            rec['own_group'] = {label: bool(rec[label]) and all(m['pgid'] != rec['lead_pgid'] for m in rec[label])
                                for label in ('fg', 'bg')}
            rec['trigger'] = trigger
            p15_trigger(lead, rec, trigger)
            grace = 5  # OPERATOR_REAP_GRACE's default (r3 (q) item 2)
            time.sleep(grace + 1)
            rec['alive_after_grace_plus_one'] = {name: alive(pid) for name, pid in recorded}
        finally:
            lead.close(grace=5)
            # Owned: the process groups of the tool commands our own lead started, read off
            # its descendants above, and the pids recorded with them; never a name.
            for pgid in groups:
                try:
                    os.killpg(pgid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
            for name, pid in recorded:
                if alive(pid):
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
    return grade_p15(r, reaps)


# The supervisor records the lead's descendants once a second (provider-supervisor.py, "THE
# LIMIT, STATED"), so a command younger than one snapshot when the lead crashes is reparented
# to launchd unrecorded, on every engine. Measured 2026-10-20 on the host with the 40a01e7f and
# the HEAD supervisor alike: a child whose provider was killed 0.3 s after the child started
# survived 5 of 5 times, at 1.5 s 0 of 5. Found within a second of starting, the foreground command fell inside that
# window, so the probe measured the poll, not the reap (FAIL on HEAD twice, PASS on 40a01e7f
# twice, by phase). Two snapshots' worth of settling puts both commands past it.
P15_SETTLE_SECONDS = 2.0


def p15_trigger(lead, rec, trigger):
    """End the run: once both commands are older than the supervisor's snapshot interval, kill
    the app stand-in (owner) or the lead (lead crash)."""
    time.sleep(P15_SETTLE_SECONDS)
    if trigger == 'owner':
        os.kill(lead.proc.pid, signal.SIGKILL)  # owned: the app stand-in this harness started
    elif rec['claude_pid']:
        os.kill(rec['claude_pid'], signal.SIGKILL)  # owned: the lead, our supervisor's own child


# P15's context, said plainly: why the two commands run together, and that they are scratch.
P15_NOTE = ('This session runs in a disposable test virtual machine, deleted after the run. It measures what '
            'happens to the commands a session has running when the session ends. The script named below is a '
            'scratch stand-in for a long build: it prints its progress every ten seconds and exits. ')


def process_rows():
    """Every process: pid, parent, group and command line, from ps."""
    code, out, _ = run(['ps', '-A', '-ww', '-o', 'pid=,ppid=,pgid=,command='], timeout=30)
    rows = []
    for line in out.splitlines():
        parts = line.split(None, 3)
        if len(parts) >= 3 and all(x.isdigit() for x in parts[:3]):
            rows.append({'pid': int(parts[0]), 'ppid': int(parts[1]), 'pgid': int(parts[2]),
                         'command': parts[3] if len(parts) > 3 else ''})
    return rows


def long_tasks(rows):
    return [row for row in rows if 'long-task.py' in row['command'] and 'python' in row['command'].lower()]


def below(rows, pid):
    """The rows under `pid`, by parent pid."""
    found, frontier = [], {pid}
    while frontier:
        nxt = {row['pid'] for row in rows if row['ppid'] in frontier}
        found.extend(row for row in rows if row['ppid'] in frontier)
        frontier = nxt
    return found


def wait_long_task(lead_pid, seconds, within):
    """The pid of the long task of `seconds` running under the lead, once it appears."""
    if not lead_pid:
        return None
    end = time.time() + within
    while time.time() < end:
        for row in long_tasks(below(process_rows(), lead_pid)):
            if row['command'].rstrip().endswith(' %d' % seconds):
                return row['pid']
        time.sleep(1)
    return None


def tool_group(rows, task_pid):
    """The tool command's whole process group: the shell the Bash tool started and the task
    under it, with each one's role."""
    row = next((x for x in rows if x['pid'] == task_pid), None)
    if row is None:
        return []
    return [{'pid': x['pid'], 'pgid': x['pgid'], 'role': 'task' if x['pid'] == task_pid else 'shell',
             'command': x['command'][:200]} for x in rows if x['pgid'] == row['pgid']]


def tool_processes(run_record):
    """{name: alive} for the tool processes of one run; the lead is not one of them."""
    return {k: v for k, v in (run_record or {}).get('alive_after_grace_plus_one', {}).items() if k != 'lead'}


def measured(run_record):
    """Both commands' processes were found. Names start with the command's label, in this
    design (fg-task-123) and in the 2026-09-25 one (fg-self)."""
    names = tool_processes(run_record)
    return all(any(n.startswith(side + '-') for n in names) for side in ('fg', 'bg'))


def grade_p15(r, reaps):
    """The premise is per trigger: under today's supervisor, the tool processes outlive the
    app's death and the lead's crash. A control that found nothing, or where nothing
    survived, shows nothing for its trigger."""
    controls = {'owner': r.get('control-no-reap'), 'lead': r.get('control-no-reap-lead-crash')}
    unshown = {trigger: (tool_processes(c) if measured(c) else 'not measured') for trigger, c in controls.items()
               if not (measured(c) and any(tool_processes(c).values()))}
    if unshown:
        return 'PREMISE-FALSE', ('without --reap-descendants no tool process was shown to survive for %s, so that '
                                 'control shows nothing: %s' % (sorted(unshown), unshown))
    if not reaps:
        return 'NOT-RUN', ('the controls show survivors (own group: %s, %s); the reap half needs provider-supervisor.py '
                           '--reap-descendants, which this engine does not have yet (r3 (q), zach)'
                           % (controls['owner'].get('own_group'), controls['lead'].get('own_group')))
    return grade_p15_reap(r)


def grade_p15_reap(r):
    """The reap half. A run in which either command's processes were never found measured
    nothing about them: it is PREMISE-FALSE, never a pass (the 2026-09-25 run's lead-crash half
    recorded only the lead, and "no survivor among the recorded" read as all gone)."""
    unmeasured = [key for key in ('reap', 'reap-lead-crash') if not measured(r.get(key))]
    if unmeasured:
        return 'PREMISE-FALSE', ('the tool processes were not found in %s, so the reap of them was not measured: %s'
                                 % (unmeasured, {k: (r.get(k) or {}).get('alive_after_grace_plus_one') for k in unmeasured}))
    survivors = {key: [n for n, a in r[key]['alive_after_grace_plus_one'].items() if a]
                 for key in ('reap', 'reap-lead-crash')}
    if not any(survivors.values()):
        return 'PASS', 'on the app\'s death and on the lead\'s crash, all gone within the grace period plus one second'
    return 'FAIL', 'still alive: %s' % survivors


def team_state(team_dir):
    """What the session team directory holds: whether it exists, whether it has the roster
    `config.json` an interactive session gets, and the names its spawn ledger holds."""
    d = Path(team_dir)
    return {'exists': d.is_dir(), 'config_json': (d / 'config.json').is_file(),
            'spawned_names': names_file(d / 'spawned-names.log') if (d / 'spawned-names.log').exists() else None}


def tool_result(lead, tool_use_id):
    """(text, is_error) of one tool call, read off the stream."""
    for f in lead.all_frames():
        if f.get('type') != 'user':
            continue
        for b in (f.get('message') or {}).get('content') or []:
            if isinstance(b, dict) and b.get('type') == 'tool_result' and b.get('tool_use_id') == tool_use_id:
                c = b.get('content')
                text = c if isinstance(c, str) else '\n'.join(x.get('text', '') for x in c or [] if isinstance(x, dict))
                return text, bool(b.get('is_error'))
    return None, None


def subagent_transcripts(ctx, session_id, agent_id):
    """A teammate's own transcript: `<claude dir>/projects/<project>/<lead session>/subagents/agent-<id>.jsonl`
    (the layout of his own session directory, measured on the host 2026-09-25)."""
    return sorted(glob.glob(str(ctx.p.claude_dir / 'projects' / '*' / session_id / 'subagents' / ('agent-%s.jsonl' % agent_id))))


def grade_p16(r):
    """r4 §5, P16. Required: (1) the name-reuse clause refuses A's name and a fresh control name
    passes; (2) the team directory resolves by exact session id; (3) the lead's SendMessage to
    running B reaches B's transcript. Recorded only: B's message to the lead."""
    reuse = r.get('reuse_a') or {}
    fresh = r.get('control_fresh_name') or {}
    one = 'BLOCKED (name reuse)' in (reuse.get('said') or '') and bool(fresh.get('agent_call'))
    two = (r.get('resolve_teams_dir') or '').startswith('exact session id match')
    three = bool(r.get('token_in_b_transcript'))
    r['required'] = {'1_name_reuse_refused_and_fresh_passes': one, '2_exact_session_match': two,
                     '3_send_message_reached_b': three}
    if not r.get('b_alive_at_send') == 'ALIVE':
        return 'PREMISE-FALSE', 'B was %s when the lead sent its message, so "a running teammate" was not tested' % r.get('b_alive_at_send')
    if not fresh.get('agent_call'):
        return 'PREMISE-FALSE', 'the fresh-name control did not spawn, so the reuse refusal cannot be told from a broken spawn'
    if one and two and three:
        return 'PASS', ('team dir before the first spawn %s, after %s; name reuse refused, fresh name passed; %s; the lead\'s '
                        'message reached B' % (r['team_dir_before_first_spawn'], r['team_dir_after_first_spawn'],
                                               r['resolve_teams_dir']))
    return 'FAIL', 'required: %s' % r['required']


@probe('P16')
def p16(ctx, r):
    sid = str(uuid.uuid4())
    team = ctx.p.claude_dir / 'teams' / ('session-%s' % sid[:8])
    token_to_b = 'P16-LEAD-TO-B-' + uuid.uuid4().hex[:8]
    token_to_lead = 'P16-B-TO-LEAD-' + uuid.uuid4().hex[:8]
    lead = Lead(ctx, 'P16', 'lead', ctx.lead_args(session_id=sid))
    try:
        r['initialize'] = lead.initialize(perTaskStopAffordance=True)
        ask(lead, 'Reply with exactly the word ready.', 180)
        r['team_dir_before_first_spawn'] = team_state(team)
        a_id, a_wt, r['spawn_a'] = spawn_teammate(ctx, lead, 'probe-sonnet-p16a', 'alpha', 15)
        r['team_dir_after_first_spawn'] = team_state(team)
        b_id, b_wt, r['spawn_b'] = spawn_teammate(
            ctx, lead, 'probe-sonnet-p16b', 'gamma', 150,
            then='Then use the SendMessage tool once to send team-lead exactly this message: "Fixture note from B: %s".'
                 % token_to_lead)
        if not (a_id and b_id and b_wt):
            return 'NOT-RUN', 'two teammates could not be started (see spawn_a, spawn_b)'
        env = dict(ctx.stored, HOME=str(ctx.p.home))
        code, out, err = run(['python3', str(ctx.p.engine / 'scripts' / 'lib' / 'teammate-identity.py'),
                              '--resolve-teams-dir', '--session', sid, '--how'], env=env, timeout=60)
        r['resolve_teams_dir'] = out.strip() or err.strip()[-300:]
        # ListAgents while B runs (recorded).
        r['b_alive_at_list'] = ctx.liveness(b_wt)[0]
        start, _ = do(lead, 'Call the ListAgents tool once, then reply with the single word listed.', 180)
        uses = tool_uses(lead, start, 'ListAgents')
        r['list_agents'] = [dict(zip(('text', 'is_error'), tool_result(lead, u.get('id')))) for u in uses]
        # (3) A message from the lead to running B.
        r['b_alive_at_send'] = ctx.liveness(b_wt)[0]
        start, _ = do(lead, 'Use the SendMessage tool once to send the running teammate named probe-sonnet-p16b exactly this '
                            'message: "Fixture note: %s. No action is needed." Then reply with the single word sent.'
                      % token_to_b, 180)
        uses = tool_uses(lead, start, 'SendMessage')
        r['send_message_calls'] = [{'input_keys': sorted((u.get('input') or {}).keys()),
                                    'result': dict(zip(('text', 'is_error'), tool_result(lead, u.get('id'))))} for u in uses]
        # (1) Once A has ended, its name again; then the fresh-name control.
        lead.wait(lambda f: f.get('subtype') == 'task_notification' and f.get('task_id') == a_id, 120)
        r['a_ended'] = [f.get('status') for f in lead.all_frames()
                        if f.get('subtype') == 'task_notification' and f.get('task_id') == a_id]
        command = "%s probe-sonnet-p16a --repo %s --type alpha --brief %s --model sonnet --description 'Probe teammate again'" % (
            ctx.p.engine / 'scripts' / 'spawn.sh', ctx.p.entity, brief(ctx, 'probe-sonnet-p16a-again', 10))
        output, _ = bash_output(lead, 'Run exactly this Bash command once and reply with its full output. Do not call the '
                                      'Agent tool, whatever it prints: %s' % command, 180)
        r['reuse_a'] = {'said': (output or '')[-2000:]}
        c_id, c_wt, fresh = spawn_teammate(ctx, lead, 'probe-sonnet-p16c', 'alpha', 10)
        r['control_fresh_name'] = fresh
        # (3) continued: B's own transcript, once B has had its turn boundary.
        lead.wait(lambda f: f.get('subtype') == 'task_notification' and f.get('task_id') == b_id, 300)
        deadline = time.time() + 90
        found = []
        while time.time() < deadline:
            files = subagent_transcripts(ctx, sid, b_id)
            found = [x for x in files if token_to_b in Path(x).read_text(errors='replace')]
            if found:
                break
            time.sleep(3)
        r['b_transcripts'] = subagent_transcripts(ctx, sid, b_id)
        r['token_in_b_transcript'] = bool(found)
        # Recorded only: did B's message to team-lead reach the lead's stream?
        r['b_to_lead_seen_by_lead'] = token_to_lead in json.dumps(lead.all_frames())
        r['team_dir_at_end'] = team_state(team)
    finally:
        lead.close()
    return grade_p16(r)


def scrub_artifact_results(ids):
    """Replace the text of these tool results with their length: P17 reads his account's
    artifact list to prove the tool works, and the record keeps only that it answered."""
    def scrub(frame):
        if frame.get('type') != 'user':
            return frame
        blocks = (frame.get('message') or {}).get('content')
        if not isinstance(blocks, list) or not any(isinstance(b, dict) and b.get('tool_use_id') in ids() for b in blocks):
            return frame
        out = json.loads(json.dumps(frame))
        for b in out['message']['content']:
            if isinstance(b, dict) and b.get('tool_use_id') in ids():
                text = json.dumps(b.get('content'))
                b['content'] = '<Artifact result removed by the probe: %d characters, is_error %s>' % (len(text), b.get('is_error'))
        return out
    return scrub


def artifact_action(tool_input):
    """r4 names the two read-only actions `list_types` and `list`. The 2.1.282 tool spells the
    first `{"action": "list", "scope": "types"}` (measured, run 2026-09-25): it has no
    `list_types` action. Both spellings are the same read-only question."""
    tool_input = tool_input or {}
    action = tool_input.get('action')
    if action == 'list_types' or (action == 'list' and tool_input.get('scope') == 'types'):
        return 'list_types'
    return action


@probe('P17')
def p17(ctx, r):
    for key, with_variable in (('artifact', True), ('control', False)):
        env = ctx.lead_env()
        if not with_variable:
            env.pop(ARTIFACT[0], None)
        rec = r.setdefault(key, {'has_variable': ARTIFACT[0] in env})
        seen = set()
        lead = Lead(ctx, 'P17', key, ctx.lead_args(), env=env, scrub=scrub_artifact_results(lambda: seen))
        try:
            rec['initialize'] = lead.initialize(perTaskStopAffordance=True)
            ask(lead, 'Reply with exactly the word ready.', 180)
            rec['artifact_in_init_tools'] = 'Artifact' in ((lead.init_frame() or {}).get('tools') or [])
            if key == 'artifact' and rec['artifact_in_init_tools']:
                start, _ = do(lead, 'Call the Artifact tool twice, both read-only: first with the action list_types, then '
                                    'with the action list. Publish nothing, create nothing and change nothing. Then reply '
                                    'with the single word done.', 300)
                calls = []
                for u in tool_uses(lead, start, 'Artifact'):
                    seen.add(u.get('id'))
                    text, is_error = tool_result(lead, u.get('id'))
                    calls.append({'action': artifact_action(u.get('input')), 'input': u.get('input'), 'is_error': is_error,
                                  'answered': text is not None, 'result_chars': len(text or '')})
                rec['calls'] = calls
        finally:
            lead.close()
    main, control = r['artifact'], r['control']
    if control.get('artifact_in_init_tools'):
        return 'PREMISE-FALSE', 'without the variable the lead has Artifact too, so the variable is not what grants it'
    if not main.get('artifact_in_init_tools'):
        return 'FAIL', 'with CLAUDE_CODE_ARTIFACT=1 the lead still has no Artifact tool'
    by_action = {c['action']: c for c in main.get('calls') or []}
    ok = all(a in by_action and by_action[a]['answered'] and by_action[a]['is_error'] is not True for a in ('list_types', 'list'))
    if ok:
        return 'PASS', 'with CLAUDE_CODE_ARTIFACT=1 Artifact is listed and list_types and list both succeed; without it, absent'
    return 'FAIL', 'Artifact is listed, but the read-only calls did not both succeed: %s' % main.get('calls')


# =============================================================================================
# P18: does the provider drop a resent uuid across a restart? (richos-hq
# docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md §1.4). The 2.1.281-2.1.283 code
# drops a stream-json user message whose uuid the resumed session's saved transcript already
# holds with an answered turn, and echoes it; that property is undocumented. The app's retry of
# an answer relies on it, so it is measured here, on the version the host has at run time.
# =============================================================================================

OPERATOR_REAP_GRACE = 5  # provider-supervisor.py's default grace (r3 (q) item 2), as in P15


def transcript_rows(ctx, session_id):
    """The session's saved transcript, one dict per line; a torn line is skipped."""
    path = ctx.p.claude_dir / 'projects' / slug(ctx.p.entity) / ('%s.jsonl' % session_id)
    rows = []
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                rows.append(json.loads(line))
            except ValueError:
                continue
    return rows


def uuid_entries(rows, uid):
    """The non-sidechain `user` entries that carry `uid`: the message, as the provider saved it."""
    return [e for e in rows if isinstance(e, dict) and e.get('type') == 'user' and e.get('uuid') == uid
            and not e.get('isSidechain')]


def after_entry(rows, uid):
    """What the transcript holds after the first entry carrying `uid`, as types (no text), and
    whether an `interruptedByShutdown` marker is among them (the provider's re-execute case)."""
    index = next((i for i, e in enumerate(rows) if isinstance(e, dict) and e.get('uuid') == uid), None)
    if index is None:
        return {'found': False, 'after': [], 'interrupted_by_shutdown': False}
    later = rows[index + 1:]
    return {'found': True,
            'after': [(e.get('type'), (e.get('message') or {}).get('role') if isinstance(e.get('message'), dict) else None)
                      for e in later if isinstance(e, dict)][:40],
            'interrupted_by_shutdown': any(isinstance(e, dict) and e.get('interruptedByShutdown') is True for e in later)}


def transcript_summary(rows):
    """Every entry's shape, never its text: what the provider wrote, for the record."""
    return [{k: e.get(k) for k in ('type', 'subtype', 'uuid', 'parentUuid', 'isSidechain', 'interruptedByShutdown',
                                    'operation') if k in e} for e in rows if isinstance(e, dict)]


def frames_between(lead, t0, seconds):
    """Every frame in [t0, t0 + seconds], as its shape (C5: what tells the host that a dropped
    duplicate started no turn)."""
    with lead.cond:
        rows = list(lead.frames)
    out = []
    for t, f in rows:
        if t0 <= t <= t0 + seconds:
            row = {'t': round(t - t0, 3), 'type': f.get('type'), 'subtype': f.get('subtype')}
            for k in ('uuid', 'isReplay', 'session_id', 'is_error'):
                if k in f:
                    row[k] = f.get(k)
            if f.get('type') == 'stream_event':
                row['event'] = (f.get('event') or {}).get('type')
            out.append(row)
    return out


def results_with(lead, start, marker):
    """How many `result` frames since `start` carry `marker` in their final text."""
    return sum(1 for f in lead.all_frames()[start:] if f.get('type') == 'result' and marker in (f.get('result') or ''))


def die_like_the_app(ctx, lead, rec):
    """The app's death, the P15 way: SIGKILL to the owner stand-in this harness started; the
    supervisor then ends the lead within its grace. Waits OPERATOR_REAP_GRACE + 1 s. What this
    lead started is recorded first, by the process tree under its own pid, and anything of it
    still alive after the grace is ended by that recorded pid (owned, never a name)."""
    supervisor = lead.supervisor_pid()
    claude = lead.claude_pid()
    recorded = [row['pid'] for row in below(process_rows(), claude)] if claude else []
    rec['claude_pid'] = claude
    os.kill(lead.proc.pid, signal.SIGKILL)  # owned: the app stand-in this harness started
    time.sleep(OPERATOR_REAP_GRACE + 1)
    rec['claude_alive_after_grace_plus_one'] = bool(claude) and alive(claude)
    rec['descendants_alive_after_grace_plus_one'] = [pid for pid in recorded if alive(pid)]
    rec['supervisor_alive_after_grace_plus_one'] = bool(supervisor) and alive(supervisor)
    for pid in [x for x in (supervisor, claude) if x] + recorded:
        if alive(pid):
            try:
                os.kill(pid, signal.SIGKILL)  # owned: recorded under this harness's own lead above
            except ProcessLookupError:
                pass
    lead.close(grace=5)


def resend(ctx, lead, uid, text, rec, echo_timeout=90):
    """Send `text` under `uid` again, and record the echo, if any, and every frame for 30 s
    after it."""
    start = lead.count()
    lead.user(text, uuid=uid)
    got = lead.wait(lambda f: f.get('type') == 'user' and f.get('uuid') == uid, echo_timeout, start)
    rec['echo'] = bool(got)
    rec['echo_is_replay'] = bool(got) and got[2].get('isReplay') is True
    rec['echo_seconds'] = round(got[1] - lead.started, 3) if got else None
    # The echo carries the session it was taken in: the one reading of the resumed session a
    # dropped duplicate gives, because a drop starts no turn and so sends no system/init (run
    # 2026-09-27, case B).
    rec['echo_session_id'] = got[2].get('session_id') if got else None
    return start, got


def resumed_identity(lead, rec):
    init = lead.init_frame() or {}
    rec['resumed_session_id'] = init.get('session_id') or rec.get('echo_session_id')
    rec['claude_code_version'] = init.get('claude_code_version') or init.get('version')


def p18_answered(ctx, r):
    """Case A (and D on the same resumed lead): the answered turn's uuid, resent after a restart."""
    rec = r.setdefault('A', {})
    ctl = r.setdefault('D', {})
    rec['session'] = session = str(uuid_module.uuid4())
    uid = str(uuid_module.uuid4())
    first = Lead(ctx, 'P18', 'A-first', ctx.lead_args(session_id=session), reap=True, owner=True)
    try:
        first.initialize()
        start = first.count()
        first.user('Reply with exactly: MARK-A', uuid=uid)
        rec['first_answered'] = bool(first.result_after(start, 180)) and results_with(first, start, 'MARK-A') >= 1
        die_like_the_app(ctx, first, rec)
    finally:
        first.close(grace=5)
    rec['transcript_u_before_resend'] = len(uuid_entries(transcript_rows(ctx, session), uid))
    resumed = Lead(ctx, 'P18', 'A-resumed', ctx.lead_args(resume=session), reap=True, owner=True)
    try:
        resumed.initialize()
        sent_at = time.time()
        start, got = resend(ctx, resumed, uid, 'Reply with exactly: MARK-A-AGAIN', rec)
        time.sleep(max(0, sent_at + 90 - time.time()))  # the 90 s window for any MARK-A-AGAIN
        rec['frames_30s_after_echo'] = frames_between(resumed, got[1], 30) if got else []
        rec['again_answered'] = 'MARK-A-AGAIN' in resumed.text(start) or results_with(resumed, start, 'MARK-A-AGAIN') > 0
        # D, the control: a FRESH uuid on the same resumed lead is answered, so the drop above
        # is keyed on the uuid and not on the lead refusing everything.
        d_start = resumed.count()
        ctl['uuid'] = resumed.user('Reply with exactly: MARK-D')
        ctl['answered'] = bool(resumed.result_after(d_start, 180)) and results_with(resumed, d_start, 'MARK-D') >= 1
        resumed_identity(resumed, rec)
        ctl['resumed_session_id'] = rec['resumed_session_id']
    finally:
        resumed.close()
    rows = transcript_rows(ctx, session)
    rec['transcript_u_after'] = len(uuid_entries(rows, uid))
    rec['transcript'] = transcript_summary(rows)


def p18_mid_turn(ctx, r):
    """Case B, recorded and not graded (C7): the app dies while U's own turn runs a command."""
    rec = r.setdefault('B', {})
    rec['session'] = session = str(uuid_module.uuid4())
    uid = str(uuid_module.uuid4())
    first = Lead(ctx, 'P18', 'B-first', ctx.lead_args(session_id=session), reap=True, owner=True)
    try:
        first.initialize()
        start = first.count()
        first.user(lead_long_task(ctx, 120, 'with exactly: MARK-B'), uuid=uid)
        rec['in_shell'] = bool(in_lead_shell(ctx, first, start))
        die_like_the_app(ctx, first, rec)
    finally:
        first.close(grace=5)
    rows = transcript_rows(ctx, session)
    rec['transcript_u_before_resend'] = len(uuid_entries(rows, uid))
    rec['after_u_before_resend'] = after_entry(rows, uid)
    resumed = Lead(ctx, 'P18', 'B-resumed', ctx.lead_args(resume=session), reap=True, owner=True)
    try:
        resumed.initialize()
        start, got = resend(ctx, resumed, uid, 'Reply with exactly: MARK-B-AGAIN', rec)
        # A re-executed turn may be the killed one (a 120 s command) or the resent text.
        resumed.result_after(start, 300)
        time.sleep(10)
        rec['frames_30s_after_echo'] = frames_between(resumed, got[1], 30) if got else []
        rec['turn_after_resend'] = results_with(resumed, start, '') > 0
        text = resumed.text(start)
        rec['ran'] = {'MARK-B-AGAIN': 'MARK-B-AGAIN' in text,
                      'MARK-B (the killed text)': bool(re.search(r'MARK-B(?!-AGAIN)', text)),
                      'long task again': any('long-task.py' in json.dumps(b.get('input'))
                                             for b in tool_uses(resumed, start, 'Bash'))}
        resumed_identity(resumed, rec)
    finally:
        resumed.close()
    rows = transcript_rows(ctx, session)
    rec['transcript_u_after'] = len(uuid_entries(rows, uid))
    rec['after_u_after'] = after_entry(rows, uid)
    rec['transcript'] = transcript_summary(rows)


def p18_queued(ctx, r):
    """Case C (window W4): U waits in the CLI's queue behind a running turn when the app dies."""
    rec = r.setdefault('C', {})
    rec['session'] = session = str(uuid_module.uuid4())
    uid = str(uuid_module.uuid4())
    first = Lead(ctx, 'P18', 'C-first', ctx.lead_args(session_id=session), reap=True, owner=True)
    try:
        first.initialize()
        start = first.count()
        rec['v_uuid'] = first.user(lead_long_task(ctx, 180, 'with exactly: MARK-V'))
        rec['in_shell'] = bool(in_lead_shell(ctx, first, start))
        queued_at = first.count()
        first.user('Reply with exactly: MARK-C', uuid=uid)
        time.sleep(5)
        rec['echo_before_kill'] = bool(first.wait(lambda f: f.get('type') == 'user' and f.get('uuid') == uid, 0.1,
                                                  queued_at))
        rows = transcript_rows(ctx, session)
        rec['u_in_transcript_before_kill'] = len(uuid_entries(rows, uid))
        rec['queue_operations_before_kill'] = sum(1 for e in rows if isinstance(e, dict)
                                                  and e.get('type') == 'queue-operation')
        die_like_the_app(ctx, first, rec)
    finally:
        first.close(grace=5)
    rec['transcript_u_before_resend'] = len(uuid_entries(transcript_rows(ctx, session), uid))
    resumed = Lead(ctx, 'P18', 'C-resumed', ctx.lead_args(resume=session), reap=True, owner=True)
    try:
        resumed.initialize()
        start, got = resend(ctx, resumed, uid, 'Reply with exactly: MARK-C-AGAIN', rec)
        resumed.wait(lambda f: f.get('type') == 'result' and 'MARK-C-AGAIN' in (f.get('result') or ''), 240, start)
        time.sleep(20)  # a second answer, if one comes, comes in this window
        rec['frames_30s_after_echo'] = frames_between(resumed, got[1], 30) if got else []
        rec['again_results'] = results_with(resumed, start, 'MARK-C-AGAIN')
        resumed_identity(resumed, rec)
    finally:
        resumed.close()
    rows = transcript_rows(ctx, session)
    rec['transcript_u_after'] = len(uuid_entries(rows, uid))
    rec['transcript'] = transcript_summary(rows)


def p18_live_repeat(ctx, r):
    """Case E: the same uuid twice to one live lead, no restart (the in-memory check)."""
    rec = r.setdefault('E', {})
    uid = str(uuid_module.uuid4())
    lead = Lead(ctx, 'P18', 'E-live', ctx.lead_args(), reap=True)
    try:
        lead.initialize()
        start = lead.count()
        lead.user('Reply with exactly: MARK-E', uuid=uid)
        rec['first_answered'] = bool(lead.result_after(start, 180)) and results_with(lead, start, 'MARK-E') >= 1
        start, got = resend(ctx, lead, uid, 'Reply with exactly: MARK-E-AGAIN', rec, echo_timeout=60)
        time.sleep(60)
        rec['frames_30s_after_echo'] = frames_between(lead, got[1], 30) if got else []
        rec['again_answered'] = 'MARK-E-AGAIN' in lead.text(start) or results_with(lead, start, 'MARK-E-AGAIN') > 0
    finally:
        lead.close()


def resumed_session(rec):
    """The session a resumed lead ran in: its system/init when a turn ran, else the session
    its echo carried (a dropped duplicate starts no turn, so it sends no init)."""
    if rec.get('resumed_session_id'):
        return rec['resumed_session_id']
    for row in rec.get('frames_30s_after_echo') or []:
        if row.get('session_id'):
            return row['session_id']
    return None


def grade_p18(r):
    """PASS when A, C, D and E pass and every resume kept its session (§1.4). B is recorded."""
    r = dict(r)
    for key in ('A', 'B', 'C'):
        if r.get(key):
            r[key] = dict(r[key], resumed_session_id=resumed_session(r[key]))
    a, b, c, d, e = (r.get(k) or {} for k in ('A', 'B', 'C', 'D', 'E'))
    premises = []
    if not a.get('first_answered'):
        premises.append('A: the first MARK-A turn was not answered, so nothing was saved to drop against')
    if not d.get('answered'):
        premises.append('D: the resumed lead did not answer a fresh uuid, so a silence proves nothing')
    if not c.get('in_shell') or c.get('echo_before_kill'):
        premises.append('C: U was not waiting behind a running command at the kill')
    if not e.get('first_answered'):
        premises.append('E: the first MARK-E turn was not answered')
    for key, rec in (('A', a), ('B', b), ('C', c)):
        if rec.get('claude_alive_after_grace_plus_one'):
            premises.append('%s: the lead outlived the app stand-in by more than the grace' % key)
        if rec and rec.get('resumed_session_id') is None:
            premises.append('%s: neither a system/init nor an echo named the resumed session' % key)
    if premises:
        return 'PREMISE-FALSE', '; '.join(premises)
    fails = []
    for key, rec in (('A', a), ('B', b), ('C', c)):
        if rec.get('resumed_session_id') != rec.get('session'):
            fails.append('%s: --resume did not keep the session (%s, not %s), so the saved-transcript check reads '
                         'another file' % (key, rec.get('resumed_session_id'), rec.get('session')))
    if not (a.get('echo') and not a.get('again_answered') and a.get('transcript_u_after') == 1):
        fails.append('A: echo %s, MARK-A-AGAIN answered %s, transcript entries for U %s (want echo, no answer, 1)'
                     % (a.get('echo'), a.get('again_answered'), a.get('transcript_u_after')))
    if not (c.get('u_in_transcript_before_kill') == 0 and c.get('again_results') == 1
            and c.get('transcript_u_after') == 1):
        fails.append('C: U saved before the kill %s, MARK-C-AGAIN answers %s, transcript entries for U %s '
                     '(want 0, 1, 1)' % (c.get('u_in_transcript_before_kill'), c.get('again_results'),
                                         c.get('transcript_u_after')))
    if not (e.get('echo') and not e.get('again_answered')):
        fails.append('E: echo %s, MARK-E-AGAIN answered %s (want echo, no answer)'
                     % (e.get('echo'), e.get('again_answered')))
    if fails:
        return 'FAIL', '; '.join(fails)
    return 'PASS', ('a resent uuid already answered in the resumed session was dropped with its echo (A), a fresh '
                    'uuid was answered (D), a queued message lost with the app was answered once on resend (C), a '
                    'live repeat was dropped (E), and every resume kept its session; B recorded: %s'
                    % {k: b.get(k) for k in ('echo', 'ran', 'transcript_u_after')})


@probe('P18')
def p18(ctx, r):
    r['cases'] = []
    for case in (p18_answered, p18_mid_turn, p18_queued, p18_live_repeat):
        try:
            case(ctx, r)
            r['cases'].append({'case': case.__name__, 'ok': True})
        except Exception:  # noqa: BLE001 - one case's harness failure is recorded; the others still run
            r['cases'].append({'case': case.__name__, 'error': traceback.format_exc()[-2000:]})
    errors = [c for c in r['cases'] if 'error' in c]
    if errors:
        return 'ERROR', 'harness failure in %s: %s' % ([c['case'] for c in errors], errors[0]['error'][-600:])
    return grade_p18(r)


# =============================================================================================
# W2: the operator walk (r3 §6 verification 5, as r4 §5 changes it), against the app side's
# own back end, the `operator_walk` example: the declaration gate, the operator host, the
# launcher with the claim, his engine's scripts, the report tool. Run only when asked.
# =============================================================================================

class Walk(object):
    """The walk's back end, driven over JSON lines: one command, one answer; notices arrive as
    {"event": "say"} lines at any time. Its pid is this harness's own child: owned."""

    def __init__(self, ctx, data, state, root, tag, crash=None):
        self.ctx, self.says, self.replies = ctx, [], {}
        self.cond = threading.Condition()
        self.next = 0
        env = {'HOME': str(ctx.p.home), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'TMPDIR': ctx.tmpdir,
               # A credential in the APP's environment must never reach his lead (P8's control).
               'ANTHROPIC_API_KEY': 'planted-by-the-walk-and-never-passed',
               # The guest's ssh session is outside the GUI launchd domain (harness finding 8):
               # the walk hands the launcher a launchctl that prints the GUI session's socket.
               'RICHOS_WALK_LAUNCHCTL': str(ctx.p.work / 'launchctl-walk'),
               # The engine's ECS store, so the walk opens and settles obligations for real.
               'RICHOS_WALK_PYTHON': ctx.python}
        if crash:
            # W3's crash matrix: the named point aborts the walk (compiled in only with the
            # `crash-points` feature). The launcher builds his lead's environment from empty,
            # so this never reaches a lead.
            env['RICHOS_CRASH_POINT'] = crash
        self.stderr_path = ctx.p.results / ('W2-walk-%s.stderr.txt' % tag)
        self.proc = subprocess.Popen([ctx.walk_binary, 'host', str(data), str(state), str(root)], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=open(str(self.stderr_path), 'w'), env=env,
                                     cwd=str(ctx.p.work), start_new_session=True, text=True, bufsize=1)
        self.started = time.time()
        threading.Thread(target=self._read, daemon=True).start()
        self.ready = self.wait_reply('ready', 60)

    def _read(self):
        for line in self.proc.stdout:
            try:
                v = json.loads(line)
            except ValueError:
                continue
            with self.cond:
                if v.get('event') == 'say':
                    v['t'] = round(time.time() - self.started, 3)
                    self.says.append(v)
                elif 'ready' in v:
                    self.replies['ready'] = v
                else:
                    self.replies[v.get('id')] = v
                self.cond.notify_all()
        with self.cond:
            self.replies['closed'] = True
            self.cond.notify_all()

    def wait_reply(self, rid, timeout):
        deadline = time.time() + timeout
        with self.cond:
            while rid not in self.replies and 'closed' not in self.replies and time.time() < deadline:
                self.cond.wait(1.0)
            return self.replies.get(rid)

    def call(self, cmd, timeout=180, **fields):
        self.next += 1
        rid = 'w%d' % self.next
        self.proc.stdin.write(json.dumps(dict(fields, id=rid, cmd=cmd)) + '\n')
        self.proc.stdin.flush()
        return self.wait_reply(rid, timeout) or {'error': 'no answer within %d s' % timeout}

    def wait_say(self, pred, timeout, start=0):
        deadline = time.time() + timeout
        with self.cond:
            while True:
                for s in self.says[start:]:
                    if pred(s):
                        return s
                if time.time() >= deadline or 'closed' in self.replies:
                    return None
                self.cond.wait(1.0)

    def count(self):
        with self.cond:
            return len(self.says)

    def kill(self):
        os.kill(self.proc.pid, signal.SIGKILL)  # owned: the walk process this harness started
        self.proc.wait(timeout=30)

    def quit(self):
        try:
            got = self.call('quit', timeout=120)
        except (BrokenPipeError, ValueError):
            got = None
        try:
            self.proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, signal.SIGKILL)  # owned: its own group
            self.proc.wait(timeout=30)
        return got


def w2_fixture(ctx, r):
    """The walk's own additions to the probe fixture: his declaration, the fences installed and
    on in the fixture entity, a planted escalation on every Stop, a cheap model."""
    p = ctx.p
    # Question publication checks this declared record with the real premise guard.
    # The generic probe's title and identity token alone contain no parseable ruling.
    rules = p.entity / 'CLAUDE.md'
    rules.write_text(rules.read_text() + '\n## Fixture boundaries\n\n'
                     'Work only in this disposable entity. Its conversations and assignments are synthetic.\n')
    home_env = dict(ctx.stored, HOME=str(p.home), **HARNESS_PIN)
    # A cheap model for the walk's leads: the profile passes none, so his settings decide it.
    settings = json.loads((p.claude_dir / 'settings.json').read_text())
    settings['model'] = PROBE_MODEL
    (p.claude_dir / 'settings.json').write_text(json.dumps(settings, indent=1) + '\n')
    # Step 9's planted escalation: a project Stop hook whose systemMessage every lead emits.
    project = p.entity / '.claude' / 'settings.json'
    ps = json.loads(project.read_text()) if project.exists() else {}
    ps.setdefault('hooks', {})['Stop'] = [{'hooks': [{'type': 'command',
                                                      'command': 'echo \'{"systemMessage":"W2-ESC-1 planted"}\''}]}]
    project.write_text(json.dumps(ps, indent=1) + '\n')
    # The launchctl the walk hands the launcher (see Walk).
    sock, _ = derive_ssh_auth_sock()
    lc = p.work / 'launchctl-walk'
    lc.write_text('#!/bin/sh\n[ "$1 $2" = "getenv SSH_AUTH_SOCK" ] || exit 9\necho %s\n' % (sock or ''))
    os.chmod(str(lc), 0o755)
    # The stored environment enable.sh would write: the allowlist, plus the harness's pin, with
    # the guest's claude on PATH (stored_environment).
    stored = dict(ctx.stored, **HARNESS_PIN)
    import hashlib
    decl = {'schema': 1, 'entity_root': str(p.entity), 'engine_root': str(p.engine), 'home': str(p.home),
            'claim': {'file': str(p.claude_dir / 'state' / 'operator-lead.json'),
                      'lock': str(p.claude_dir / 'state' / 'operator-lead.lock')},
            'permission_mode': 'bypassPermissions', 'environment': stored,
            'origins': ['desk-typed', 'desk-voice', 'desk-file'], 'file_roots': [str(p.ab)],
            'contract': {'path': str(p.contract), 'sha256': hashlib.sha256(p.contract.read_bytes()).hexdigest()}}
    git(p.entity, 'add', '-A')
    git(p.entity, 'commit', '-q', '-m', 'W2 fixture: planted escalation hook', '--allow-empty')
    git(p.entity, 'push', '-q', 'origin', 'main')
    return decl, home_env


def w2_gate(ctx, walk_binary, data):
    code, out, err = run([walk_binary, 'gate', str(data)], env={'HOME': str(ctx.p.home), 'PATH': '/usr/bin:/bin'},
                         timeout=120)
    try:
        return json.loads(out)
    except ValueError:
        return {'error': (out + err)[-500:]}


def state_listing(p):
    d = p.claude_dir / 'state'
    return sorted(str(x.relative_to(d)) for x in d.rglob('*')) if d.is_dir() else []


def heartbeat_command(path):
    return "while true; do date +%%s > %s; sleep 1; done" % path


def heartbeat_stopped(path, within):
    """True when the heartbeat file stops changing within `within` seconds."""
    time.sleep(within)
    first = path.read_text().strip() if path.exists() else ''
    time.sleep(2.5)
    return (path.read_text().strip() if path.exists() else '') == first


def spawn_via_walk(ctx, walk, thread, title, name, agent_type, seconds, timeout=360):
    """A teammate started the way his lead starts one: spawn.sh, then the Agent call it prints."""
    before = set(r['worktree'] for r in ctx.worktrees())
    ctx.spawned.append(name)
    command = "%s %s --repo %s --type %s --brief %s --model sonnet --description 'Walk teammate %s'" % (
        ctx.p.engine / 'scripts' / 'spawn.sh', name, ctx.p.entity, agent_type, brief(ctx, name, seconds), name)
    start = walk.count()
    given = walk.call('assign', thread=thread, title=title, origin='desk-typed', timeout=480, text=FIXTURE_NOTE + (
        'Start one teammate. Step 1: run exactly this Bash command and read its standard output:\n%s\n'
        'Step 2: its standard output is one JSON object of Agent tool parameters. Call the Agent tool ONCE with exactly '
        'those parameters, adding run_in_background set to true. Step 3: reply with the single word started.' % command))
    walk.wait_say(lambda s: s['thread'] == thread and s['kind'] == 'update', timeout, start)
    # The lead now reports on its handle right after its Agent call, before a background
    # teammate's worktree exists (r5b: w2-sonnet-b's Agent call returned ok at 285.6 s and the
    # single snapshot found no worktree). So the new worktree is waited for, up to 90 s, never
    # read once.
    deadline = time.time() + 90
    after = [r for r in ctx.worktrees() if r['worktree'] not in before]
    while len(after) != 1 and time.time() < deadline:
        time.sleep(3)
        after = [r for r in ctx.worktrees() if r['worktree'] not in before]
    ctx.spawn_handles[name] = given.get('handle')
    return after[0]['worktree'] if len(after) == 1 else None


def grade_question_step(step):
    methods = ('phone_tap', 'phone_typed', 'phone_voice')
    answers = step.get('answers', [])
    questions = step.get('after', [])
    retry = step.get('retry', {})
    return (step.get('question_count') == 3 and step.get('independent_progress') is True
            and step.get('lead_received') is True and len(answers) == 3 and len(questions) == 3
            and [a.get('enqueued') for a in answers] == [0, 0, 1]
            and all(a.get('reconciled') == 0 and a.get('answer', {}).get('outcome') == 'accepted' for a in answers)
            and sorted(q.get('answer', {}).get('method', '') for q in questions) == sorted(methods)
            and all(q.get('delivered') is True and q.get('revision') == 1 for q in questions)
            and retry.get('enqueued') == 0 and retry.get('reconciled') == 0
            and retry.get('answer', {}).get('outcome') == 'accepted'
            and retry.get('answer', {}).get('question', {}).get('revision') == 1)


def question_step(walk, asking, independent):
    """S6's resolved-answer seam, with real lead reports and durable production stores.

    This begins after phone authentication and recognition. Device/transport/microphone
    checks are separate; method labels here do not claim any of those happened.
    """
    step = {}
    start = walk.count()
    prepared = [{'text': 'When should release %s ship?' % name, 'options': [
        {'label': 'Ship today', 'description': 'Earlier fixes'},
        {'label': 'Ship tomorrow', 'description': 'More testing'}]} for name in ('one', 'two', 'three')]
    step['register'] = walk.call('assign', thread=asking[0], title=asking[1], origin='desk-typed', timeout=480,
        text=FIXTURE_NOTE + ('Call mcp__richos_operator__report once with kind question, handle set to this '
        'assignment\'s handle, text "W2 prepared question set", and questions set to this exact array: %s. '
        'After recording it, continue independent work by reporting kind update with the same handle and '
        'text "W2 question posted". When the complete answer set arrives, report kind update on the same '
        'handle with text "W2 got all three answers". Never ask this set again.' % json.dumps(prepared)))
    handle = step['register'].get('handle')
    if not handle:
        return dict(step, **{'pass': False})
    deadline = time.time() + 300
    questions = []
    while time.time() < deadline:
        questions = walk.call('questions', thread=asking[0], handle=handle).get('questions', [])
        if len(questions) == 3:
            break
        time.sleep(1)
    step['question_count'] = len(questions)
    if len(questions) != 3:
        return dict(step, **{'pass': False})
    walk.call('assign', thread=independent[0], title=independent[1], origin='desk-typed',
              text='Reply with exactly: W2-QUESTION-INDEPENDENT')
    other = walk.wait_say(lambda s: s['thread'] == independent[0] and 'W2-QUESTION-INDEPENDENT' in s['text'], 240, start)
    posted = walk.wait_say(lambda s: s.get('handle') == handle and 'W2 question posted' in s['text'], 240, start)
    step['independent_progress'] = bool(other) and bool(posted)
    answers = step.setdefault('answers', [])
    request = None
    for question, method in zip(sorted(questions, key=lambda q: q['set_index']), ('phone_tap', 'phone_typed', 'phone_voice')):
        option = next((o['id'] for o in question['options'] if o['label'] == 'Ship tomorrow'), None)
        if option is None:
            return dict(step, **{'pass': False})
        request = {'question_id': question['id'], 'client_id': 'w2-' + method,
                   'option_ids': [option], 'text': '', 'expected_revision': None}
        answers.append(walk.call('answer-question', thread=asking[0], method=method, answer=request))
    step['retry'] = walk.call('answer-question', thread=asking[0], method='phone_voice', answer=request)
    got = walk.wait_say(lambda s: s.get('handle') == handle and 'W2 got all three answers' in s['text'], 300, start)
    step['lead_received'] = bool(got)
    step['after'] = walk.call('questions', thread=asking[0], handle=handle).get('questions', [])
    step['pass'] = grade_question_step(step)
    return step


@probe('W2', explicit=True)
def w2(ctx, r, question_only=False):
    if not getattr(ctx, 'walk_binary', None) or not os.path.exists(ctx.walk_binary):
        return 'NOT-RUN', 'no operator_walk binary was handed to the guest (run-probes.py --walk-binary)'
    p = ctx.p
    steps = r.setdefault('steps', {})
    left = r.setdefault('left', [])
    r['state_before'] = state_listing(p)
    decl, home_env = w2_fixture(ctx, r)
    data = p.work / 'w2-data'
    data.mkdir(parents=True, exist_ok=True)
    state = p.work / 'w2-engine-state'
    root = p.work / 'w2-operator'

    # ---- step 0: the gate's four states, the product path first -----------------------------
    s0 = steps.setdefault('0', {})
    s0['no_file'] = w2_gate(ctx, ctx.walk_binary, data)
    (data / 'operator.json').write_text('{ not json')
    s0['broken_file'] = w2_gate(ctx, ctx.walk_binary, data)
    (data / 'operator.json').write_text(json.dumps(decl, indent=1))
    s0['switch_off'] = w2_gate(ctx, ctx.walk_binary, data)
    config = p.entity / 'orchestration.config'
    config.write_text(config.read_text() + 'OPERATOR_FENCES="on"\nOPERATOR_FENCES_REPOS="%s"\n' % p.entity)
    git(p.entity, 'add', 'orchestration.config')
    git(p.entity, 'commit', '-q', '-m', 'W2 fixture: the operator switch declared on')
    git(p.entity, 'push', '-q', 'origin', 'main')
    s0['declared_on_fences_absent'] = w2_gate(ctx, ctx.walk_binary, data)
    fences = p.engine / 'scripts' / 'operator-fences.sh'
    s0['fence_install'] = run(['/bin/bash', str(fences), 'install', '--entity', str(p.entity)], env=home_env,
                              cwd=str(p.entity), timeout=120)[:2]
    s0['fence_on'] = run(['/bin/bash', str(fences), 'on', '--entity', str(p.entity)], env=home_env,
                         cwd=str(p.entity), timeout=120)[:2]
    s0['fence_status'] = run(['/bin/bash', str(fences), 'status', '--entity', str(p.entity)], env=home_env,
                             cwd=str(p.entity), timeout=120)[:2]
    s0['valid'] = w2_gate(ctx, ctx.walk_binary, data)
    s0['pass'] = (s0['no_file'].get('gate') == 'product' and s0['broken_file'].get('gate') == 'refused'
                  and s0['switch_off'].get('gate') == 'refused' and s0['declared_on_fences_absent'].get('gate') == 'refused'
                  and s0['valid'].get('gate') == 'operator')
    if s0['valid'].get('gate') != 'operator':
        return 'RECORDED', 'the walk stops at the gate: %s' % s0['valid']

    walk = Walk(ctx, data, state, root, 'first')
    r['walk_ready'] = walk.ready
    if not walk.ready:
        r['walk_stderr'] = walk.stderr_path.read_text()[-2000:] if walk.stderr_path.exists() else ''
        return 'RECORDED', 'the walk\'s back end did not start past the gate: %s' % r['walk_stderr'][-300:]
    A, B = ('w2-a', 'Walk A: land'), ('w2-b', 'Walk B: stop')
    claim_file = p.claude_dir / 'state' / 'operator-lead.json'
    try:
        if question_only:
            steps['4'] = question_step(walk, A, B)
            left.append('Resolved phone answer methods only: physical transport, recognition, microphone '
                        'capture and visible cards need the separate device/window checks.')
            passed = s0['pass'] and steps['4']['pass']
            return ('PASS' if passed else 'FAIL'), 'S6 gate and durable operator question round trip'
        # ---- step 6 FIRST: the claim is taken at the first lead, so the terminal must be live
        # before any lead opens for the app-side refusal to be what decides -------------------
        s6 = steps.setdefault('6', {})
        ipid, ifd, record, screen = interactive_session(ctx, r)
        try:
            s6['terminal_record'] = record
            s6['with_terminal'] = walk.call('assign', thread=A[0], title=A[1], origin='desk-typed',
                                            text='Reply with exactly: W2-CLAIM-1')
        finally:
            end_interactive(ipid, ifd)
        time.sleep(2)
        s6['after_terminal_ended'] = walk.call('assign', thread=A[0], title=A[1], origin='desk-typed',
                                               text='Reply with exactly: W2-CLAIM-2')
        opened = walk.wait_say(lambda s: s['thread'] == A[0] and 'W2-CLAIM-2' in s['text'], 240)
        s6['claim_while_running'] = json.loads(claim_file.read_text()) if claim_file.exists() else None
        # A corrupted claim: a new lead refuses and names the way through; restored after.
        good = claim_file.read_text() if claim_file.exists() else ''
        claim_file.write_text('{ corrupted by the walk')
        s6['with_corrupt_claim'] = walk.call('assign', thread='w2-d', title='Walk D: corrupt claim', origin='desk-typed',
                                             text='Reply with exactly: W2-D')
        claim_file.write_text(good)
        s6['pass'] = ('running in the terminal' in (s6['with_terminal'].get('error') or '')
                      and s6['after_terminal_ended'].get('relayed') is True and bool(opened)
                      and bool(s6['claim_while_running']) and s6['claim_while_running'].get('owner') == 'app'
                      and 'unreadable' in (s6['with_corrupt_claim'].get('error') or ''))
        left.append('step 6: a fresh terminal refusing Agent and the lease while the app runs his team is the engine '
                    'claim hook\'s (zach C6a); the walk shows the app side of the claim only')

        # ---- step 1: a land, pushed, confirmed in Git; step 2: B picked up while A lands ----
        s1 = steps.setdefault('1', {})
        start = walk.count()
        s1['relay'] = walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text=FIXTURE_NOTE + (
            'In the repository %s (its main checkout), create a branch named w2-land, add a file w2-land.txt holding the '
            'line walk, commit it on that branch, merge the branch into main with git merge --no-ff, and push main to '
            'origin. If a command or a hook tells you to take the land lease first, do exactly what it says, and release '
            'the lease after the push. Then call the tool mcp__richos_operator__report (find it with ToolSearch if it is '
            'deferred) with kind outcome, handle set to this assignment\'s handle (given at the top of this message), '
            'text "W2 land done", and lands: one entry with repository %s, commit set to the FULL hash of the merge '
            'commit now on main, and branch w2-land.' % (p.entity, p.entity)), timeout=480)
        s2 = steps.setdefault('2', {})
        b_sent = time.time()
        s2['relay'] = walk.call('assign', thread=B[0], title=B[1], origin='desk-typed',
                                text='Reply with exactly: W2-B-PICKUP', timeout=480)
        got = walk.wait_say(lambda s: s['thread'] == B[0] and 'W2-B-PICKUP' in s['text'], 240)
        s2['b_pickup_seconds'] = round(time.time() - b_sent, 2) if got else None
        s2['a_still_in_its_turn'] = not any(s['thread'] == A[0] for s in walk.says[start:] if s['kind'] in ('outcome', 'update'))
        s2['pass'] = bool(got)
        landed = walk.wait_say(lambda s: s['thread'] == A[0] and 'Landed' in s['text'], 900, start)
        s1['said'] = [s for s in walk.says[start:] if s['thread'] == A[0]][-6:]
        code, out, _ = run(['git', '-C', str(p.remote), 'log', '--oneline', '-3', 'main'], timeout=60)
        s1['remote_main'] = out.strip()
        s1['settle_calls'] = (root / 'settle-calls.jsonl').read_text() if (root / 'settle-calls.jsonl').exists() else ''
        s1['settled'] = walk.call('settled', thread=A[0], handle=s1['relay'].get('handle'))
        s1['obligation_opened'] = s1['relay'].get('obligation')
        s1['pass'] = bool(landed) and 'Landed and pushed w2-land' in (landed or {}).get('text', '') \
            and s1['relay'].get('relayed') is True and s1['settled'].get('state') == 'settled' \
            and s1['obligation_opened'] == 'opened' and 'Settled' in str(s1['settled'].get('ecs'))
        left.append('steps 2 (land wait on a lease), 3, 3a, 3b, 3c: the engine side\'s fence and lease cases, walked by '
                    'its own fence suite; not repeated here')

        # ---- step 4: a question round trip on one handle ----------------------------------
        steps['4'] = question_step(walk, A, B)
        left.append('step 4 covers resolved phone answer methods through the durable store and operator inbox. '
                    'Physical phone transport, word recognition, microphone capture and visible cards require '
                    'the separate S6 device/window checks; a method label is not device evidence.')

        # ---- step 5: one named agent stopped mid-command, from the PHONE (step 10) ----------
        s5 = steps.setdefault('5', {})
        a_wt = spawn_via_walk(ctx, walk, B[0], B[1], 'w2-sonnet-a', 'alpha', 480)
        b_wt = spawn_via_walk(ctx, walk, B[0], B[1], 'w2-sonnet-b', 'beta', 480)
        s5['worktrees'] = [a_wt, b_wt]
        if a_wt and b_wt:
            start = walk.count()
            walk.call('assign', thread=B[0], title=B[1], origin='desk-typed', text=lead_long_task(ctx, 60, 'slept'))
            time.sleep(8)
            s5['before'] = {'a': ctx.liveness(a_wt)[0], 'b': ctx.liveness(b_wt)[0]}
            t0 = time.time()
            s5['stop'] = walk.call('stop', thread=B[0], title=B[1], names=['w2-sonnet-a'], words='stop w2-sonnet-a',
                                   origin='phone', timeout=240)
            s5['stop_seconds'] = round(time.time() - t0, 2)
            s5['after'] = {'a': registry_of(ctx.liveness(a_wt)), 'b': registry_of(ctx.liveness(b_wt))}
            done = walk.wait_say(lambda s: s['thread'] == B[0] and 'slept' in s['text'].lower(), 240, start)
            s5['lead_command_finished'] = bool(done)
            reply = s5['stop'].get('reply') or {}
            log_text = (data / 'operator' / 'operator.log').read_text() if (data / 'operator' / 'operator.log').exists() else ''
            s5['operator_log_origin'] = [l for l in log_text.splitlines() if 'stop of w2-sonnet-a' in l][-1:]
            s5['pass'] = 'w2-sonnet-a' in (reply.get('stopped') or []) and s5['after']['a'][0] == 'NOT-ALIVE' \
                and bool(s5['after']['a'][1]) and s5['after']['b'][0] == 'ALIVE' and bool(done) \
                and any('Phone' in l for l in s5['operator_log_origin'])
            # His Esc: the lead's turn ends, and b stays alive.
            # The r4d run sent the second message 0 s after the first and pressed Esc 12 s
            # later, and the interrupt answered 0 still queued: nothing recorded whether the
            # lead was inside its command yet. Now the second message goes only once the
            # lead's own 93-second command is running (read off the walk's descendants), and
            # what the queued message did afterwards is recorded.
            s5b = steps.setdefault('5b', {})  # his Esc, graded on its own
            esc_start = walk.count()
            s5b['esc_relay_long'] = walk.call('assign', thread=B[0], title=B[1], origin='desk-typed',
                                             text=lead_long_task(ctx, 93, 'slept again'))
            s5b['esc_long_task_pid'] = wait_long_task(walk.proc.pid, 93, 180)
            s5b['esc_relay_queued'] = walk.call('assign', thread=B[0], title=B[1], origin='desk-typed',
                                               text='Reply with exactly: W2-QUEUED')
            time.sleep(3)
            s5b['esc_queued_said_before_esc'] = any('W2-QUEUED' in s['text'] for s in walk.says[esc_start:])
            s5b['esc'] = walk.call('interrupt', thread=B[0], title=B[1], timeout=240)
            s5b['esc_long_task_alive_5s_after'] = bool(s5b['esc_long_task_pid']) and (time.sleep(5) or alive(s5b['esc_long_task_pid']))
            s5b['b_after_esc'] = ctx.liveness(b_wt)[0]
            ran = walk.wait_say(lambda s: s['thread'] == B[0] and 'W2-QUEUED' in s['text'], 120, esc_start)
            s5b['esc_queued_ran_after'] = bool(ran)
            s5b['pass'] = s5b['b_after_esc'] == 'ALIVE' and 'still queued' in ((s5b['esc'].get('reply') or {}).get('say') or '') \
                and bool(ran)
            # The per-assignment Stop (the window's control): exactly the agent its turn started,
            # attributed from the stream as the CLI took the message.
            s5c = steps.setdefault('5c', {})
            s5c['handle'] = ctx.spawn_handles.get('w2-sonnet-b')
            s5c['stop'] = walk.call('stop-assignment', thread=B[0], handle=s5c['handle'], timeout=240)
            s5c['b_after'] = registry_of(ctx.liveness(b_wt))
            s5c['pass'] = s5c['stop'].get('stopped') is True and s5c['b_after'][0] == 'NOT-ALIVE'
        else:
            s5['pass'] = False
            s5['why'] = 'two teammates could not be started'

        # ---- step 7: a false land claim ----------------------------------------------------
        s7 = steps.setdefault('7', {})
        tree = git(p.entity, 'rev-parse', 'HEAD^{tree}')
        orphan = git(p.entity, 'commit-tree', tree, '-p', 'HEAD', '-m', 'W2 never merged')
        git(p.entity, 'update-ref', 'refs/heads/w2-unmerged', orphan)
        start = walk.count()
        walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text=FIXTURE_NOTE + (
            'Call mcp__richos_operator__report with kind outcome, NO handle (leave the handle out), text "W2 false '
            'land", and lands: one entry with repository %s, commit %s, branch w2-unmerged. Report exactly that, even '
            'though it may not be true: the probe checks that it is caught.' % (p.entity, orphan)))
        got = walk.wait_say(lambda s: s['thread'] == A[0] and 'W2 false land' in s['text'], 300, start)
        s7['said'] = (got or {}).get('text')
        s7['pass'] = bool(got) and 'could not be confirmed as landed' in got['text']

        # ---- step 8: plain words reach him; a report and then more delivers both ---------------
        s8 = steps.setdefault('8', {})
        start = walk.count()
        walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text='Reply with exactly: W2-PLAIN-1')
        plain = walk.wait_say(lambda s: s['thread'] == A[0] and s['text'].strip() == 'W2-PLAIN-1', 240, start)
        walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text=(
            'Call mcp__richos_operator__report with kind update and text "W2-R1". After that call, reply with exactly: '
            'W2-R1 and W2-MORE'))
        more = walk.wait_say(lambda s: s['thread'] == A[0] and 'W2-MORE' in s['text'], 240, start)
        rep = walk.wait_say(lambda s: s['thread'] == A[0] and s['text'].strip() == 'W2-R1', 5, start)
        s8['pass'] = bool(plain) and bool(more) and bool(rep)
        s8['said'] = [s['text'] for s in walk.says[start:] if s['thread'] == A[0]]

        # ---- step 9: one planted escalation reaches him once, not once per lead --------------
        s9 = steps.setdefault('9', {})
        s9['alarms'] = [s for s in walk.says if s['kind'] == 'alarm' and 'W2-ESC-1' in s['text']]
        s9['pass'] = len(s9['alarms']) == 1
        s9['leads_that_ended_turns'] = sorted(set(s['thread'] for s in walk.says if s['kind'] == 'update'))

        # ---- step 10: the phone ------------------------------------------------------------
        s10 = steps.setdefault('10', {})
        s10['phone_assignment'] = walk.call('assign', thread=A[0], title=A[1], origin='phone',
                                            text='Land something from my phone.')
        s10['phone_settled'] = walk.call('settled', thread=A[0], handle=s10['phone_assignment'].get('handle'))
        s10['no_origin'] = walk.call('assign', thread=A[0], title=A[1], origin='unrecorded', text='An unrecorded thought.')
        s10['phone_stop'] = 'step 5\'s stop was said on the phone; the operator log names its origin'
        s10['pass'] = s10['phone_assignment'].get('relayed') is False \
            and 'only takes work from the Mac' in (s10['phone_assignment'].get('sentence') or '') \
            and s10['phone_settled'].get('state') == 'failed' \
            and s10['no_origin'].get('relayed') is False \
            and "can't tell where this was asked from" in (s10['no_origin'].get('sentence') or '')

        # ---- step 11: the app's death takes the whole tree with it ----------------------------
        s11 = steps.setdefault('11', {})
        hb = p.work / 'w2-heartbeat-a.txt'
        start = walk.count()
        walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text=FIXTURE_NOTE + (
            'Start this Bash command with run_in_background set to true, then reply started: %s' % heartbeat_command(hb)))
        walk.wait_say(lambda s: s['thread'] == A[0] and 'started' in s['text'].lower(), 240, start)
        deadline = time.time() + 60
        while time.time() < deadline and not hb.exists():
            time.sleep(1)
        s11['heartbeat_running'] = hb.exists()
        claim_before = json.loads(claim_file.read_text()) if claim_file.exists() else {}
        walk.kill()
        # What the first walk's leads said, whole, for the record (r5a kept only each step's extract).
        r['says_first_walk'] = list(walk.says)
        s11['heartbeat_stopped_within_grace_plus_one'] = heartbeat_stopped(hb, 6)
        s11['claim_pids_alive_after'] = {str(x['pid']): alive(x['pid']) for x in claim_before.get('processes', [])
                                         if x.get('role') != 'app'}
        log = data / 'operator' / 'operator.log'  # the desk's log, where the app keeps it
        s11['operator_log_tail'] = log.read_text()[-2500:] if log.exists() else ''
        walk = Walk(ctx, data, state, root, 'relaunched')
        s11['leads_before_a_message'] = (walk.call('read', thread=A[0], every=True).get('reply') or {}).get('conversations')
        walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', text='Reply with exactly: W2-RESUMED')
        got = walk.wait_say(lambda s: s['thread'] == A[0] and 'W2-RESUMED' in s['text'], 240)
        s11['resumed_answer'] = bool(got)
        s11['resumed_in_log'] = 'resumed' in (log.read_text() if log.exists() else '')
        s11['pass'] = s11['heartbeat_running'] and s11['heartbeat_stopped_within_grace_plus_one'] \
            and not any(s11['claim_pids_alive_after'].values()) and s11['resumed_answer'] and s11['resumed_in_log']

        # ---- step 13: idle retirement ---------------------------------------------------------
        s13 = steps.setdefault('13', {})
        E, F = ('w2-e', 'Walk E: idle'), ('w2-f', 'Walk F: busy')
        hb_f = p.work / 'w2-heartbeat-f.txt'
        start = walk.count()
        walk.call('assign', thread=E[0], title=E[1], origin='desk-typed', text='Reply with exactly: W2-IDLE-1')
        walk.call('assign', thread=F[0], title=F[1], origin='desk-typed', text=FIXTURE_NOTE + (
            'Start this Bash command with run_in_background set to true, then reply started: %s' % heartbeat_command(hb_f)))
        walk.wait_say(lambda s: s['thread'] == E[0] and 'W2-IDLE-1' in s['text'], 240, start)
        walk.wait_say(lambda s: s['thread'] == F[0] and 'started' in s['text'].lower(), 240, start)
        time.sleep(8)
        s13['retired'] = walk.call('retire', idle_seconds=5).get('retired')
        start = walk.count()
        walk.call('assign', thread=E[0], title=E[1], origin='desk-typed',
                  text='What is the fixture watchword? Reply with the watchword only.')
        got = walk.wait_say(lambda s: s['thread'] == E[0] and 'PERIWINKLE-4417' in s['text'], 240, start)
        s13['resumed_answer'] = (got or {}).get('text')
        s13['pass'] = E[0] in (s13['retired'] or []) and F[0] not in (s13['retired'] or []) and bool(got)
        left.append('step 13: a stdio MCP server in the fixture (G8) is not added; the lead\'s own report server is one')

        # ---- step 12: quit with an agent running names it, then stops everything -----------
        s12 = steps.setdefault('12', {})
        s12['team_before_quit'] = walk.call('team')
        s12['work_leases'] = walk.call('lease-spawns')
        s12['quit'] = walk.quit()
        s12['heartbeat_f_stopped'] = heartbeat_stopped(hb_f, 6)
        s12['pass'] = s12['heartbeat_f_stopped'] and bool(s12['team_before_quit'].get('quit_sheet')) \
            and s12['work_leases'].get('work_leases_opened') == 0
        left.append('step 12: the quit sheet\'s rendering is the window\'s; the walk shows the sentence it would say, '
                    'from the reading the shell takes inside its exit callback')
    finally:
        if walk.proc.poll() is None:
            walk.quit()
        r['says_last_walk'] = list(walk.says)
        r['state_after'] = state_listing(p)
        r['state_new'] = sorted(set(r['state_after']) - set(r['state_before']))
    passed = sorted(k for k, v in steps.items() if v.get('pass'))
    failed = sorted(k for k, v in steps.items() if not v.get('pass'))
    return 'RECORDED', 'W2 steps passed: %s; not passed: %s; left: %d items (see "left")' % (passed, failed, len(left))


@probe('S6', explicit=True)
def s6(ctx, r):
    """Run only the question acceptance, with W2's gate, fixture and owned cleanup."""
    return w2(ctx, r, question_only=True)


# =============================================================================================
# O2: the operator contract notes' §4 items 1 and 3 (richos-hq
# tools-private/operator/operator-contract-notes.md; Frank's F3 and F6), on the app's own
# composition: the real `operator_walk host`, real leads, the shipped engine and the desk's socket.
#   F3: a question written into a report's words, on a subject his record rules, is refused
#       with the ruling and never reaches his outbox; a plain report in the same turn does.
#   F6: a stop he gives in conversation A for conversation B's agent is executed by A's lead,
#       through its own `stop`, once: the agent is NOT-ALIVE and the operator log has one stop.
# Both leads are told which tool to call: what is measured is the app path, not whether a model
# chooses it unprompted (that is the contract's and the tool description's job).
# =============================================================================================

O2_RULED = 'Landed the fix.\n\nWhich splash screens should ship in v1, all seven variations or your two?'


def o2_ruling(ctx):
    """His record rules one subject, written as the engine's own ceo-ruled suite writes one."""
    p = ctx.p
    (p.entity / 'wiki').mkdir(parents=True, exist_ok=True)
    (p.entity / 'wiki' / 'ceo-decisions.md').write_text(
        '# Decisions\n\n## 21. The start screen (CEO, 2026-09-01)\n\n'
        '### The splash screens — TWO, and the order is DETERMINISTIC (CEO, 2026-09-01)\n\n'
        '**His words:** *"I have never approved more than 2 splash screens."*\n')
    config = p.entity / 'orchestration.config'
    config.write_text(config.read_text() + 'CEO_RULINGS_PATHS="wiki/ceo-decisions.md"\n')
    git(p.entity, 'add', '-A')
    git(p.entity, 'commit', '-q', '-m', 'O2 fixture: one ruling on his record')
    git(p.entity, 'push', '-q', 'origin', 'main')


def o2_outbox(data):
    """Every report any lead recorded for him, as the host reads them."""
    lines = []
    for path in (data / 'operator').rglob('outbox.jsonl'):
        lines += [line for line in path.read_text().splitlines() if line.strip()]
    return lines


def grade_o2_f3(step):
    return (step.get('ruled_in_outbox') == 0 and step.get('control_in_outbox') == 1
            and 'O2-F3-REFUSED' in (step.get('reply') or '') and 'The splash screens' in (step.get('reply') or ''))


def grade_o2_f6(step):
    after = step.get('after') or [None, None]
    return (step.get('before') == 'ALIVE' and after[0] == 'NOT-ALIVE' and bool(after[1])
            and len(step.get('stop_lines') or []) == 1 and 'Stopped o2-sonnet-b' in (step.get('reply') or ''))


@probe('O2', explicit=True)
def o2(ctx, r):
    if not getattr(ctx, 'walk_binary', None) or not os.path.exists(ctx.walk_binary):
        return 'NOT-RUN', 'no operator_walk binary was handed to the guest (run-probes.py --walk-binary)'
    p = ctx.p
    decl, home_env = w2_fixture(ctx, r)
    o2_ruling(ctx)
    data = p.work / 'o2-data'
    data.mkdir(parents=True, exist_ok=True)
    state, root = p.work / 'o2-engine-state', p.work / 'o2-operator'
    r['gate'] = w3_gate_on(ctx, data, decl, home_env)
    if r['gate'].get('gate') != 'operator':
        # What the fence says, for the record: the gate's sentence names only the exit status.
        fences = p.engine / 'scripts' / 'operator-fences.sh'
        for verb in ('install', 'on', 'status'):
            r['fence_' + verb] = run(['/bin/bash', str(fences), verb, '--entity', str(p.entity)], env=home_env,
                                     cwd=str(p.entity), timeout=120)[:3]
        r['orchestration_config'] = (p.entity / 'orchestration.config').read_text()[-1500:]
        r['entity_status'] = git(p.entity, 'status', '--porcelain')
        return 'PREMISE-FALSE', 'the gate did not open his team: %s' % r['gate']
    walk = Walk(ctx, data, state, root, 'o2')
    r['walk_ready'] = walk.ready
    if not walk.ready:
        r['walk_stderr'] = walk.stderr_path.read_text()[-2000:] if walk.stderr_path.exists() else ''
        return 'RECORDED', 'the walk\'s back end did not start past the gate: %s' % r['walk_stderr'][-300:]
    A, B = ('o2-a', 'O2 A: a report and a stop'), ('o2-b', 'O2 B: owns the agent')
    steps = r.setdefault('steps', {})
    try:
        f3 = steps.setdefault('F3', {})
        start = walk.count()
        f3['relay'] = walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', timeout=480, text=FIXTURE_NOTE + (
            'Step 1: call the tool mcp__richos_operator__report (find it with ToolSearch if it is deferred) once, with '
            'kind update and exactly this text: %s\nStep 2: call it once more, with kind update and the text '
            '"O2-F3-CONTROL: nothing to decide here." Step 3: reply with O2-F3-REFUSED if step 1 was refused or '
            'O2-F3-RECORDED if it was recorded, followed by the exact words the tool answered in step 1.'
            % json.dumps(O2_RULED)))
        done = walk.wait_say(lambda s: s['thread'] == A[0] and 'O2-F3-RE' in s['text'], 300, start)
        f3['reply'] = (done or {}).get('text', '')[-2000:]
        outbox = o2_outbox(data)
        f3['ruled_in_outbox'] = sum('splash screens should ship' in line for line in outbox)
        f3['control_in_outbox'] = sum('O2-F3-CONTROL' in line for line in outbox)
        f3['pass'] = grade_o2_f3(f3)

        f6 = steps.setdefault('F6', {})
        wt = spawn_via_walk(ctx, walk, B[0], B[1], 'o2-sonnet-b', 'beta', 480)
        f6['worktree'] = wt
        if wt:
            f6['before'] = ctx.liveness(wt)[0]
            start = walk.count()
            t0 = time.time()
            walk.call('assign', thread=A[0], title=A[1], origin='desk-typed', timeout=480, text=FIXTURE_NOTE + (
                'He says: stop o2-sonnet-b. That agent belongs to another conversation. Call the tool '
                'mcp__richos_operator__stop (find it with ToolSearch if it is deferred) once, with names ["o2-sonnet-b"] '
                'and words "stop o2-sonnet-b". Then reply with O2-F6 followed by the exact words the tool answered.'))
            done = walk.wait_say(lambda s: s['thread'] == A[0] and 'O2-F6' in s['text'], 360, start)
            f6['seconds_to_reply'] = round(time.time() - t0, 1)
            f6['reply'] = (done or {}).get('text', '')[-2000:]
            f6['after'] = registry_of(ctx.liveness(wt))
            log = data / 'operator' / 'operator.log'
            f6['stop_lines'] = [line for line in (log.read_text() if log.exists() else '').splitlines()
                                if 'stop of o2-sonnet-b' in line]
        else:
            f6['why'] = 'the teammate could not be started'
        f6['pass'] = grade_o2_f6(f6)
    finally:
        r['says'] = list(walk.says)
        r['quit'] = walk.quit()
        claim = p.claude_dir / 'state' / 'operator-lead.json'
        left = json.loads(claim.read_text()).get('processes', []) if claim.exists() else []
        r['lead_pids_alive_after'] = {str(x['pid']): alive(x['pid']) for x in left if x.get('role') != 'app'}
    passed = all(steps.get(k, {}).get('pass') for k in ('F3', 'F6'))
    return ('PASS' if passed else 'FAIL'), 'F3 %s, F6 %s' % (
        'passed' if steps['F3'].get('pass') else 'not passed', 'passed' if steps['F6'].get('pass') else 'not passed')


# =============================================================================================
# W3: the crash matrix of his answers to his team (richos-hq
# docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md §4.2). One question set per cell,
# through the real `operator_walk host`, a real lead and the durable stores. The walk binary is
# built with the `crash-points` feature; RICHOS_CRASH_POINT names where it aborts. After each
# crash a new walk opens on the same data folder, and flushes at launch as the app does.
# =============================================================================================

W3_CELLS = ('baseline', 'W1b', 'W2', 'W3', 'W4')


def w3_gate_on(ctx, data, decl, home_env):
    """W2 step 0's last state only: the declaration written, the switch declared on and the
    fences installed and on, so the gate opens his team."""
    p = ctx.p
    (data / 'operator.json').write_text(json.dumps(decl, indent=1))
    config = p.entity / 'orchestration.config'
    config.write_text(config.read_text() + 'OPERATOR_FENCES="on"\nOPERATOR_FENCES_REPOS="%s"\n' % p.entity)
    git(p.entity, 'add', 'orchestration.config')
    git(p.entity, 'commit', '-q', '-m', 'W3 fixture: the operator switch declared on')
    git(p.entity, 'push', '-q', 'origin', 'main')
    fences = p.engine / 'scripts' / 'operator-fences.sh'
    for verb in ('install', 'on'):
        run(['/bin/bash', str(fences), verb, '--entity', str(p.entity)], env=home_env, cwd=str(p.entity), timeout=120)
    return w2_gate(ctx, ctx.walk_binary, data)


def w3_ledger(ctx, data, thread_name):
    """What the durable stores hold for one cell's conversation: the answer relay, the inbox's
    operator inputs still waiting, and how many transcript entries carry the relay's uuid."""
    names = json.loads((data / 'walk-threads.json').read_text()) if (data / 'walk-threads.json').exists() else {}
    tid = names.get(thread_name)
    out = {'thread_id': tid}
    record = data / 'operator' / 'femcboost' / (tid or '-') / 'lead.json'
    relays = json.loads(record.read_text()).get('answers', []) if record.exists() else []
    out['relays'] = relays
    waiting = 0
    for path in (data / 'engine-state' / 'questions' / 'work-inputs').glob('*.json'):
        try:
            row = json.loads(path.read_text())
        except ValueError:
            continue
        if not row.get('handed') and (row.get('delivery') or {}).get('thread_id') == tid:
            waiting += 1
    out['inbox_waiting'] = waiting
    out['transcript_uuid_counts'] = {rel['uuid']: len(uuid_entries(transcript_rows(ctx, rel.get('session') or '-'), rel['uuid']))
                                     for rel in relays if rel.get('session')}
    # The continuation a turn that died with the app was given (richos operator_host.rs
    # `continued`), and how many transcript entries carry its uuid.
    out['continuation_uuid_counts'] = {
        rel['continued']['uuid']: len(uuid_entries(transcript_rows(ctx, rel['continued'].get('session') or '-'),
                                                   rel['continued']['uuid']))
        for rel in relays if (rel.get('continued') or {}).get('uuid')}
    return out


def w3_reports(says, thread, marker):
    """How many times the lead made THE report: a notice on the thread whose text BEGINS with
    the marker ("<marker>: <label>", as instructed). The lead's own closing words may quote
    the marker ("Reported. His answer was ... I've recorded W3-W1B-GOT ..."); that is not a
    second report (run 2026-09-27, VM probes-adb41bb1d841, cells W1b, W2 and W3)."""
    return sum(1 for s in says if s.get('thread') == thread and (s.get('text') or '').startswith(marker))


def grade_w3_cell(rec):
    """A cell passes when the lead reported the answer once with NO words from him (a cell that
    needed them does not pass: the 2026-09-27 run's W3 cell did, which is the gap the continuation
    closes), the answer is in the transcript once, the inbox let it go, the relay is taken and no
    turn is left open; a continuation, when one was sent, is in the transcript once.

    A cell that names a crash point also passes only when that crash happened: the walk was
    aborted by the point (a nonzero first exit) and not killed at the wait's deadline
    (`crash_point_not_reached`), and the queued-answer window (W4) only when the long command
    it waits behind was still running at the kill. A late kill or a normal exit is a different
    test from the one the cell names (hunt part 2, finding 36)."""
    cell = rec.get('cell')
    if rec.get('crash_point_not_reached'):
        return False
    if cell in ('W1b', 'W2', 'W3') and not rec.get('first_exit'):
        return False
    if cell == 'W4' and not rec.get('long_task_alive_at_kill'):
        return False
    ledger = rec.get('ledger') or {}
    relays = ledger.get('relays') or []
    continued = list((ledger.get('continuation_uuid_counts') or {}).values())
    return (rec.get('reports') == 1 and not rec.get('nudged') and len(relays) == 1
            and relays[0].get('taken') is True and not relays[0].get('turn_open')
            and ledger.get('inbox_waiting') == 0
            and list((ledger.get('transcript_uuid_counts') or {}).values()) == [1]
            and continued in ([], [1]))


def w3_cell(ctx, data, state, root, cell):
    rec = {'cell': cell}
    thread, title = 'w3-%s' % cell.lower(), 'Walk W3 %s' % cell
    marker = 'W3-%s-GOT' % cell.upper()
    crash = cell if cell in ('W1b', 'W2', 'W3') else None
    prepared = [{'text': 'When should release %s ship?' % cell, 'options': [
        {'label': 'Ship today', 'description': 'Earlier fixes'},
        {'label': 'Ship tomorrow', 'description': 'More testing'}]}]
    walk = Walk(ctx, data, state, root, 'w3-%s-first' % cell, crash=crash)
    walks = [walk]
    try:
        if not walk.ready:
            rec['error'] = 'the walk did not start: %s' % (walk.stderr_path.read_text()[-600:] if walk.stderr_path.exists() else '')
            return rec
        rec['register'] = walk.call('assign', thread=thread, title=title, origin='desk-typed', timeout=480,
            text=FIXTURE_NOTE + ('Call mcp__richos_operator__report once with kind question, handle set to this '
            'assignment\'s handle, text "W3 question", and questions set to this exact array: %s. Then reply with '
            'exactly: W3-ASKED. When his answer arrives, call mcp__richos_operator__report once with kind update on '
            'the same handle and text "%s: " followed by the label he chose. Report it once only, and never ask this '
            'again.' % (json.dumps(prepared), marker)))
        handle = rec['register'].get('handle')
        questions = []
        deadline = time.time() + 300
        while handle and time.time() < deadline and len(questions) != 1:
            questions = walk.call('questions', thread=thread, handle=handle).get('questions', [])
            time.sleep(2)
        rec['question_count'] = len(questions)
        if len(questions) != 1:
            return rec
        walk.wait_say(lambda s: s['thread'] == thread and 'W3-ASKED' in s['text'], 240)
        if cell == 'W4':
            # Window W4: his answer waits in the CLI's queue behind a running command.
            walk.call('assign', thread=thread, title=title, origin='desk-typed',
                      text=lead_long_task(ctx, 180, 'with exactly: W3-LONG-DONE'))
            rec['long_task_pid'] = wait_long_task(walk.proc.pid, 180, 240)
        option = next(o['id'] for o in questions[0]['options'] if o['label'] == 'Ship tomorrow')
        request = {'question_id': questions[0]['id'], 'client_id': 'w3-' + cell, 'option_ids': [option], 'text': '',
                   'expected_revision': None}
        rec['answer'] = walk.call('answer-question', thread=thread, method='phone_tap', answer=request,
                                  timeout=60 if crash in ('W1b', 'W2') else 180)
        if cell == 'W4':
            time.sleep(5)
            rec['long_task_alive_at_kill'] = bool(rec.get('long_task_pid')) and alive(rec['long_task_pid'])
            walk.kill()
        elif crash:
            try:
                walk.proc.wait(timeout=180)  # the crash point aborts the walk
            except subprocess.TimeoutExpired:
                rec['crash_point_not_reached'] = True
                walk.kill()
        else:
            walk.wait_say(lambda s: s['thread'] == thread and marker in s['text'], 300)
        rec['first_exit'] = walk.proc.poll()
        if cell != 'baseline':
            time.sleep(OPERATOR_REAP_GRACE + 1)
            walk = Walk(ctx, data, state, root, 'w3-%s-relaunched' % cell)
            walks.append(walk)
            # No words from him, ever: when the crash killed the turn that had taken the answer
            # (W3 by construction, W2 when the lead began it first), the relaunched walk's
            # launch flush continues that turn itself (richos operator_host.rs
            # `continue_interrupted_answers`). The 2026-09-27 run needed his next words here;
            # a cell that would still need them fails. 300 s: a resumed lead's start plus one
            # model turn.
            rec['nudged'] = False
            rec['reported_without_his_words'] = bool(
                walk.wait_say(lambda s: s['thread'] == thread and marker in s['text'], 300))
        time.sleep(20)  # a second report, if any, comes in this window
        rec['marker'] = marker
        rec['reports'] = w3_reports([s for w in walks for s in w.says], thread, marker)
        # Every notice that names the marker is kept whole for the record, then the last 12 of the rest.
        rec['marker_says'] = [s for w in walks for s in w.says if s['thread'] == thread and marker in s['text']]
        rec['says'] = [s for w in walks for s in w.says if s['thread'] == thread][-12:]
        rec['ledger'] = w3_ledger(ctx, data, thread)
    finally:
        for w in walks:
            if w.proc.poll() is None:
                rec.setdefault('quit', []).append(w.quit())
        rec['walks_gone'] = [w.proc.poll() is not None for w in walks]
    rec['pass'] = grade_w3_cell(rec)
    return rec


@probe('W3', explicit=True)
def w3(ctx, r):
    if not getattr(ctx, 'walk_binary', None) or not os.path.exists(ctx.walk_binary):
        return 'NOT-RUN', 'no operator_walk binary built with --features crash-points was handed to the guest'
    p = ctx.p
    decl, home_env = w2_fixture(ctx, r)
    data = p.work / 'w3-data'
    data.mkdir(parents=True, exist_ok=True)
    state, root = p.work / 'w3-engine-state', p.work / 'w3-operator'
    r['gate'] = w3_gate_on(ctx, data, decl, home_env)
    if r['gate'].get('gate') != 'operator':
        return 'PREMISE-FALSE', 'the gate did not open his team: %s' % r['gate']
    cells = r.setdefault('cells', {})
    wanted = w3_wanted(getattr(ctx, 'w3_cells', ''))
    r['cells_run'] = list(wanted)
    for cell in wanted:
        try:
            cells[cell] = w3_cell(ctx, data, state, root, cell)
        except Exception:  # noqa: BLE001 - one cell's harness failure is recorded; the next still runs
            cells[cell] = {'cell': cell, 'error': traceback.format_exc()[-2000:], 'pass': False}
    claim = p.claude_dir / 'state' / 'operator-lead.json'
    left = json.loads(claim.read_text()).get('processes', []) if claim.exists() else []
    r['lead_pids_alive_after'] = {str(x['pid']): alive(x['pid']) for x in left if x.get('role') != 'app'}
    return w3_verdict(cells, wanted)


def unknown_probe_ids(only):
    """The names in a comma-separated --only that are not registered probes. An empty --only
    means all probes and has none."""
    known = {pid for pid, _ in PROBES}
    return [x for x in (y.strip() for y in (only or '').split(',')) if x and x not in known]


def w3_wanted(named):
    """The cells to run: all of them, or the comma-separated names given (--w3-cells), in the
    matrix's own order. An unknown name is refused, never skipped."""
    names = [x.strip() for x in (named or '').split(',') if x.strip()]
    unknown = [n for n in names if n not in W3_CELLS]
    if unknown:
        raise ValueError('not a crash-matrix cell: %s (cells: %s)' % (unknown, list(W3_CELLS)))
    return tuple(c for c in W3_CELLS if not names or c in names)


def w3_verdict(cells, wanted):
    passed = [c for c in wanted if (cells.get(c) or {}).get('pass')]
    failed = [c for c in wanted if not (cells.get(c) or {}).get('pass')]
    continued = [c for c in wanted if ((cells.get(c) or {}).get('ledger') or {}).get('continuation_uuid_counts')]
    if failed:
        return 'FAIL', 'cells passed: %s; not passed: %s; continued by the app: %s' % (passed, failed, continued)
    return 'PASS', ('cells %s: reported once by the lead with no words from him, one transcript entry for the uuid, '
                    'inbox empty, relay taken, no turn left open; continued by the app after the crash: %s'
                    % (list(wanted), continued))


# =============================================================================================
# W4: HISTORICAL ONLY. The per-assignment runner was removed; the existing RETIRED
# probe registry prevents this matrix from running against current builds. Its code
# and pure grading remain to interpret captured historical results. Current Stu
# recovery is covered by canonical inbox/WorkHost tests and rebuilt-app acceptance.
# Original work-path answer matrix (richos-hq
# docs/plans/2026-09-27-work-path-answer-delivery-design.md §4.2). One question set per cell,
# through the real `work_walk host` (built with the `crash-points` feature): a real work lease
# on the delivered runtime, the durable question store and inbox, boot recovery, and the
# question worker's pass exactly as the app ships it (`WorkHost::deliver_team_answers`, D8).
# The fixture job asks him one question whose options are two markers, and once answered it
# replies with exactly the marker he chose. RICHOS_CRASH_POINT names where a walk aborts; after
# it a new walk opens on the same data folder, reconciles and wakes as the app does at launch.
# =============================================================================================

W4_CELLS = ('baseline', 'P4-carry', 'P4-sent', 'P5', 'P4e', 'P7')
W4_POINT = {'P4-carry': 'WORK-CARRY', 'P4-sent': 'WORK-FIRST-ITEM', 'P5': 'WORK-TAKEN', 'P4e': 'WORK-DEAD-LEASE'}
# The provider supervisor ends a work lease whose owner died (native.rs, owner-death reap).
W4_REAP_GRACE = 8
# How long a walk may take to be ready. It verifies the whole delivered runtime (322 MB, SHA-256,
# a debug build) before recovery runs: measured 64.3 s on the loaded host, 2026-09-27, and over
# the old 120 s in the guest on run 1's P5 relaunch, whose recovery line was then read before it
# arrived. A bound on a wait, never a verdict; the time taken is recorded.
W4_READY_SECONDS = 600


def w4_wanted(named):
    """The cells to run: all of them, or the comma-separated names given (--w4-cells), in the
    matrix's own order, so one cell can be retried alone. An unknown name is refused."""
    names = [x.strip() for x in (named or '').split(',') if x.strip()]
    unknown = [n for n in names if n not in W4_CELLS]
    if unknown:
        raise ValueError('not a work crash-matrix cell: %s (cells: %s)' % (unknown, list(W4_CELLS)))
    return tuple(c for c in W4_CELLS if not names or c in names)


class WorkWalk(Walk):
    """The work walk, driven over the same JSON lines as `Walk`. Its pid is this harness's own
    child: owned, and quit or killed by that pid only."""

    def __init__(self, ctx, data, tag, crash=None):
        self.ctx, self.says, self.replies, self.recovery = ctx, [], {}, None
        self.cond = threading.Condition()
        self.next = 0
        env = {'HOME': str(ctx.p.home), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'TMPDIR': ctx.tmpdir,
               'RICHOS_CLAUDE_BIN': GUEST_CLAUDE, 'LANG': 'en_US.UTF-8'}
        env.update(HARNESS_PIN)
        if crash:
            env['RICHOS_CRASH_POINT'] = crash
        self.stderr_path = ctx.p.results / ('W4-walk-%s.stderr.txt' % tag)
        self.proc = subprocess.Popen([ctx.work_walk_binary, 'host', str(ctx.p.engine), str(ctx.runtime), str(data)],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(str(self.stderr_path), 'w'),
                                     env=env, cwd=str(ctx.p.work), start_new_session=True, text=True, bufsize=1)
        self.started = time.time()
        threading.Thread(target=self._read, daemon=True).start()
        self.ready = self.wait_reply('ready', W4_READY_SECONDS)
        self.ready_seconds = round(time.time() - self.started, 1) if self.ready else None

    def _read(self):
        for line in self.proc.stdout:
            try:
                v = json.loads(line)
            except ValueError:
                continue
            if not isinstance(v, dict):
                continue
            with self.cond:
                if v.get('event') == 'say':
                    v['t'] = round(time.time() - self.started, 3)
                    self.says.append(v)
                elif v.get('event') == 'recovery':
                    self.recovery = v
                elif 'ready' in v:
                    self.replies['ready'] = v
                else:
                    self.replies[v.get('id')] = v
                self.cond.notify_all()
        with self.cond:
            self.replies['closed'] = True
            self.cond.notify_all()

    def ok(self, cmd, timeout=180, **fields):
        got = self.call(cmd, timeout=timeout, **fields)
        if 'ok' not in got:
            raise RuntimeError('%s: %s' % (cmd, got))
        return got['ok']


def w4_brief(cell, two):
    ask = ('Ask the CEO exactly one question with the richos_questions ask tool: "%s", with two options, labeled '
           '"%s" (description: "Report the first marker.") and "%s" (description: "Report the second marker."). '
           'Then end your turn with exactly: %s.')
    first = ask % ('Which marker should this job report?', 'W4-%s-A' % cell.upper(), 'W4-%s-B' % cell.upper(),
                   'W4-ASKED')
    if not two:
        return ('This is a fixture job in a disposable test. Do not start helpers, run commands or change any file. '
                + first + ' When his answer arrives, reply with exactly the label he chose and nothing else.')
    second = ask % ('Which second marker should this job report?', 'W4-%s-C' % cell.upper(),
                    'W4-%s-D' % cell.upper(), 'W4-ASKED-2')
    return ('This is a fixture job in a disposable test. Do not start helpers, run commands or change any file. '
            + first + ' When his answer arrives, do not report it yet: ' + second + ' When that answer arrives, '
            'reply with exactly the two labels he chose, the first then the second, separated by one space, and '
            'nothing else.')


def w4_wait(walk, pred, timeout, every=2):
    deadline = time.time() + timeout
    while time.time() < deadline:
        got = pred()
        if got:
            return got
        if walk.proc.poll() is not None:
            return None
        time.sleep(every)
    return None


def w4_open_question(walk, thread):
    qs = walk.ok('questions', thread=thread).get('questions', [])
    return next((q for q in qs if q.get('state') == 'open'), None)


def w4_answer(walk, thread, question, label, tag):
    option = next(o['id'] for o in question['options'] if o['label'] == label)
    return walk.call('answer-question', thread=thread, timeout=60,
                     answer={'question_id': question['id'], 'client_id': 'w4-' + tag, 'option_ids': [option],
                             'text': '', 'expected_revision': None})


def w4_row(walk, thread):
    rows = walk.ok('rows', thread=thread).get('rows', [])
    return rows[-1] if rows else {}


def w4_marker_says(walks, thread, marker):
    return [s for w in walks for s in w.says if s.get('thread') == thread and marker in (s.get('text') or '')]


def w4_journal_tools(ctx, data, session):
    """C12's evidence: the tool calls the killed back end's own hook journal recorded."""
    path = data / 'engine-state' / 'evidence' / (session or '-') / 'callbacks.jsonl'
    names = []
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                cb = json.loads(line).get('callback') or {}
            except ValueError:
                continue
            if cb.get('hook_event_name') in ('PreToolUse', 'PostToolUse'):
                names.append('%s:%s' % (cb.get('hook_event_name'), cb.get('tool_name')))
    return names


def w4_cell(ctx, cell):
    rec = {'cell': cell}
    data = ctx.p.work / ('w4-%s' % cell.lower())
    data.mkdir(parents=True, exist_ok=True)
    thread = 'w4-%s' % cell.lower()
    two = cell == 'P7'
    first_label = 'W4-%s-B' % cell.upper()
    second_label = 'W4-%s-D' % cell.upper()
    marker = ('%s %s' % (first_label, second_label)) if two else first_label
    point = W4_POINT.get(cell)
    aborts = cell in ('P4-carry', 'P4-sent', 'P5')
    walk = WorkWalk(ctx, data, '%s-first' % cell, crash=point)
    walks = [walk]
    try:
        rec['ready_seconds'] = [walk.ready_seconds]
        if not walk.ready:
            rec['error'] = 'the walk did not start: %s' % walk.stderr_path.read_text()[-800:]
            return rec
        rec['register'] = walk.ok('assign', thread=thread, title='Fixture job %s' % cell, text=w4_brief(cell, two),
                                  timeout=180)
        q = w4_wait(walk, lambda: w4_open_question(walk, thread), 360)
        if not q:
            rec['error'] = 'the back end never asked'
            rec['row'] = w4_row(walk, thread)
            return rec
        rec['asked'] = q['text']
        if two:
            rec['answer_1'] = w4_answer(walk, thread, q, first_label, cell + '-1')
            q2 = w4_wait(walk, lambda: next((x for x in walk.ok('questions', thread=thread)['questions']
                                             if x.get('state') == 'open' and x['id'] != q['id']), None), 420)
            if not q2:
                rec['error'] = 'the back end never asked the second question'
                rec['row'] = w4_row(walk, thread)
                return rec
            blocked = w4_wait(walk, lambda: w4_row(walk, thread).get('state') == 'blocked', 240)
            rec['blocked_before_crash'] = bool(blocked)
            rec['inbox_before_crash'] = walk.ok('inbox')
            walk.kill()  # a crash while it waits on him: a quit would interrupt it (work_host shutdown)
            rec['first_exit'] = walk.proc.poll()
            time.sleep(W4_REAP_GRACE)
            walk = WorkWalk(ctx, data, '%s-relaunched' % cell)
            walks.append(walk)
            rec['ready_seconds'].append(walk.ready_seconds)
            rec['recovery'] = walk.recovery
            q2 = w4_open_question(walk, thread)
            rec['answer_2'] = w4_answer(walk, thread, q2, second_label, cell + '-2')
        else:
            before = w4_open_question(walk, thread)
            # The card, before the back end has the answer (D7): read right after the answer.
            rec['answer'] = w4_answer(walk, thread, before, first_label, cell)
            try:
                card = [x for x in walk.ok('questions', thread=thread, timeout=20)['questions'] if x['id'] == before['id']]
                rec['card_right_after_answer'] = card[0] if card else None
            except (RuntimeError, KeyError):
                rec['card_right_after_answer'] = 'unreadable (the walk may already have aborted)'
        if aborts:
            try:
                walk.proc.wait(timeout=300)  # the crash point aborts the walk
            except subprocess.TimeoutExpired:
                rec['crash_point_not_reached'] = True
                walk.kill()
            rec['first_exit'] = walk.proc.poll()
            time.sleep(W4_REAP_GRACE)
            walk = WorkWalk(ctx, data, '%s-relaunched' % cell)
            walks.append(walk)
            rec['ready_seconds'].append(walk.ready_seconds)
            rec['recovery'] = walk.recovery
        if cell == 'P5':
            # Nothing may re-run it: watch for a minute, then read.
            time.sleep(60)
        else:
            w4_wait(walk, lambda: w4_marker_says([walk], thread, marker), 420)
            time.sleep(15)  # a second report, if any, comes in this window
        rec['marker'] = marker
        rec['reports'] = len(w4_marker_says(walks, thread, marker))
        rec['says'] = [s for w in walks for s in w.says if s.get('thread') == thread][-12:]
        rec['row'] = w4_row(walk, thread)
        rec['inbox'] = walk.ok('inbox')
        rec['spawns_last_walk'] = walk.ok('spawns')['spawns']
        rec['spawns_per_walk'] = []
        for w in walks:
            try:
                rec['spawns_per_walk'].append(w.ok('spawns', timeout=20)['spawns'] if w.proc.poll() is None else None)
            except RuntimeError:
                rec['spawns_per_walk'].append(None)
        rec['cards'] = walk.ok('questions', thread=thread)['questions']
        if cell == 'P4-sent':
            rec['c12_killed_back_end_tools'] = w4_journal_tools(ctx, data, (rec.get('row') or {}).get('work_session'))
    finally:
        for w in walks:
            if w.proc.poll() is None:
                rec.setdefault('quit', []).append(w.quit())
        rec['walks_gone'] = [w.proc.poll() is not None for w in walks]
    rec['pass'], rec['why'] = grade_w4_cell(rec)
    return rec


def grade_w4_cell(rec):
    cell = rec['cell']
    row = rec.get('row') or {}
    inputs = (rec.get('inbox') or {}).get('inputs') or []
    taken = bool(inputs) and all(i.get('handed') and i.get('taken_in') for i in inputs)
    notices = [n.get('kind') for n in row.get('notices') or []]
    recovery = rec.get('recovery') or {}
    job = ((rec.get('register') or {}).get('assignment'))
    if not all(rec.get('walks_gone') or [False]):
        return False, 'a walk was left running'
    if cell in ('P4-carry', 'P4-sent', 'P5') and (rec.get('crash_point_not_reached') or not rec.get('first_exit')):
        # The cell's whole claim is recovery from an abort at its crash point (finding 36).
        return False, 'the crash point was not reached (crash_point_not_reached=%s first_exit=%s)' % (
            rec.get('crash_point_not_reached'), rec.get('first_exit'))
    if cell == 'P5':
        ok = (job in (recovery.get('unknown') or []) and row.get('state') == 'unknown' and taken
              and rec.get('reports', 0) <= 1 and rec.get('spawns_last_walk') == 0)
        return ok, 'unknown=%s state=%s taken=%s reports=%s spawns_after=%s' % (
            job in (recovery.get('unknown') or []), row.get('state'), taken, rec.get('reports'), rec.get('spawns_last_walk'))
    once = rec.get('reports') == 1
    settled = row.get('state') == 'settled'
    if cell == 'baseline':
        return once and taken and settled, 'reports=%s taken=%s state=%s' % (rec.get('reports'), taken, row.get('state'))
    if cell in ('P4-carry', 'P4-sent'):
        waiting = job in (recovery.get('answers_waiting') or []) and job not in (recovery.get('unknown') or [])
        no_unknown = 'Unknown' not in notices
        ok = waiting and no_unknown and once and taken
        return ok, 'answers_waiting=%s no_unknown_notice=%s reports=%s taken=%s' % (waiting, no_unknown, rec.get('reports'), taken)
    if cell == 'P4e':
        spawned = rec.get('spawns_last_walk') == 2
        never_failed = 'Failed' not in notices and row.get('state') != 'failed'
        return spawned and once and never_failed and taken, 'spawns=%s reports=%s never_failed=%s taken=%s' % (
            rec.get('spawns_last_walk'), rec.get('reports'), never_failed, taken)
    if cell == 'P7':
        ok = rec.get('blocked_before_crash') and once and taken
        return ok, 'blocked_before_crash=%s reports_with_both=%s taken=%s' % (rec.get('blocked_before_crash'), rec.get('reports'), taken)
    return False, 'unknown cell'


@probe('W4', explicit=True, retired='The per-assignment work runner was removed. W4 is historical capture grading only; current independent Stu recovery is verified by the core handoff lifecycle tests and rebuilt-app acceptance.')
def w4(ctx, r):
    if not getattr(ctx, 'work_walk_binary', None) or not os.path.exists(ctx.work_walk_binary):
        return 'NOT-RUN', 'no work_walk binary built with --features crash-points was handed to the guest'
    if not getattr(ctx, 'runtime', None) or not (Path(ctx.runtime) / 'delivery.json').exists():
        return 'NOT-RUN', 'no delivered runtime was handed to the guest'
    # A cheap model for the fixture's back ends, as W2 does: the lease passes none.
    settings_path = ctx.p.claude_dir / 'settings.json'
    settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
    settings['model'] = PROBE_MODEL
    settings_path.write_text(json.dumps(settings, indent=1) + '\n')
    cells = r.setdefault('cells', {})
    wanted = w4_wanted(getattr(ctx, 'w4_cells', ''))
    r['w4_cells'] = list(wanted)
    for cell in wanted:
        try:
            cells[cell] = w4_cell(ctx, cell)
        except Exception:  # noqa: BLE001 - one cell's harness failure is recorded; the next still runs
            cells[cell] = {'cell': cell, 'error': traceback.format_exc()[-2000:], 'pass': False}
        print('W4 %s: %s %s' % (cell, cells[cell].get('pass'), cells[cell].get('why') or cells[cell].get('error', '')[-300:]),
              flush=True)
    passed = [c for c in wanted if cells[c].get('pass')]
    failed = [c for c in wanted if not cells[c].get('pass')]
    if failed:
        return 'FAIL', 'cells passed: %s; not passed: %s' % (passed, failed)
    return 'PASS', ('cells %s: his answer reached the job once, taken by the back end, and a started job never '
                    're-ran' % list(wanted))


# =============================================================================================
# driver
# =============================================================================================

def regrade_p5(frames_path, task_id):
    """Grade a RECORDED P5 run by r4's line, with no guest and no model (r4 §5: "re-graded PASS
    from the existing record; no re-run"). Nothing was sent after the spawn turn ended, so that
    turn's `result` is where sending stopped."""
    rows = []
    for line in Path(frames_path).read_text().splitlines():
        if line.strip():
            row = json.loads(line)
            rows.append((row['t'], row['frame']))
    first_result = next((t for t, f in rows if f.get('type') == 'result'), None)
    if first_result is None:
        return 'ERROR', 'the record has no result frame, so where sending stopped cannot be read', {}
    return grade_p5(rows, task_id, first_result)


def regrade_p17(artifact_frames, control_frames):
    """Grade a RECORDED P17 run with no guest: the tool inputs and the error flags are in the
    saved frames (only the results' text was scrubbed)."""
    def rows(path):
        return [json.loads(line)['frame'] for line in Path(path).read_text().splitlines() if line.strip()]
    r = {'artifact': {}, 'control': {}}
    for key, path in (('artifact', artifact_frames), ('control', control_frames)):
        frames = rows(path)
        init = next((f for f in frames if f.get('type') == 'system' and f.get('subtype') == 'init'), {})
        r[key]['artifact_in_init_tools'] = 'Artifact' in (init.get('tools') or [])
        uses = [b for f in frames if f.get('type') == 'assistant' for b in (f.get('message') or {}).get('content') or []
                if b.get('type') == 'tool_use' and b.get('name') == 'Artifact']
        results = {b.get('tool_use_id'): b for f in frames if f.get('type') == 'user'
                   for b in (f.get('message') or {}).get('content') or [] if isinstance(b, dict) and b.get('type') == 'tool_result'}
        r[key]['calls'] = [{'action': artifact_action(u.get('input')), 'input': u.get('input'),
                            'is_error': (results.get(u.get('id')) or {}).get('is_error'),
                            'answered': u.get('id') in results} for u in uses]
    main, control = r['artifact'], r['control']
    if control['artifact_in_init_tools']:
        return 'PREMISE-FALSE', 'without the variable the lead has Artifact too', r
    if not main['artifact_in_init_tools']:
        return 'FAIL', 'with CLAUDE_CODE_ARTIFACT=1 the lead still has no Artifact tool', r
    by_action = {c['action']: c for c in main['calls']}
    ok = all(a in by_action and by_action[a]['answered'] and by_action[a]['is_error'] is not True for a in ('list_types', 'list'))
    if ok:
        return 'PASS', ('with CLAUDE_CODE_ARTIFACT=1 Artifact is listed and both read-only calls answered without error '
                        '(list with scope types, then list); without it, absent'), r
    return 'FAIL', 'the read-only calls did not both succeed: %s' % main['calls'], r


def regrade_w3(record):
    """Grade a saved W3 run with no guest: each cell's reports recounted by w3_reports over the
    notices it saved. Refuses (ERROR) when the saved notices cannot hold every marker notice the
    run counted, because a recount over a cut list would prove nothing."""
    cells = {}
    for cell, rec in (record.get('cells') or {}).items():
        rec = dict(rec)
        thread = 'w3-%s' % cell.lower()
        marker = rec.get('marker') or 'W3-%s-GOT' % cell.upper()
        saved = rec.get('marker_says') or [s for s in rec.get('says') or [] if marker in (s.get('text') or '')]
        naming = sum(1 for s in rec.get('says') or [] if marker in (s.get('text') or ''))
        if 'marker_says' not in rec and naming < (rec.get('reports') or 0):
            return 'ERROR', 'cell %s counted %s marker notices but saved only %s' % (cell, rec.get('reports'), naming), {}
        rec['reports'] = w3_reports(saved, thread, marker)
        rec['pass'] = grade_w3_cell(rec) if not rec.get('error') else False
        cells[cell] = rec
    wanted = tuple(record.get('cells_run') or W3_CELLS)
    facts = {c: {'reports': cells[c].get('reports'), 'nudged': cells[c].get('nudged'), 'pass': cells[c].get('pass')}
             for c in cells}
    verdict, why = w3_verdict(cells, wanted)
    return verdict, why, facts


def worse_exit(worst, verdict):
    """Process status after one more verdict. NOT-RUN is not a passing verdict, so a requested
    probe that never ran cannot end with exit 0 (hunt part 2, R52)."""
    if verdict == 'ERROR':
        return 2
    if verdict in ('FAIL', 'PREMISE-FALSE', 'NOT-RUN'):
        return max(worst, 1)
    return worst


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--grade-w3':
        if len(sys.argv) != 3:
            print('usage: guest_probes.py --grade-w3 <W3.json>', file=sys.stderr)
            return 2
        verdict, why, facts = regrade_w3(json.loads(Path(sys.argv[2]).read_text()))
        print(json.dumps({'probe': 'W3', 'graded_from': sys.argv[2], 'verdict': verdict, 'why': why, 'facts': facts},
                         indent=1))
        return 0 if verdict == 'PASS' else 1
    if len(sys.argv) > 1 and sys.argv[1] == '--grade-p18':
        if len(sys.argv) != 3:
            print('usage: guest_probes.py --grade-p18 <P18.json>', file=sys.stderr)
            return 2
        verdict, why = grade_p18(json.loads(Path(sys.argv[2]).read_text()))
        print(json.dumps({'probe': 'P18', 'graded_from': sys.argv[2], 'verdict': verdict, 'why': why}, indent=1))
        return 0 if verdict == 'PASS' else 1
    if len(sys.argv) > 1 and sys.argv[1] == '--grade-p17':
        if len(sys.argv) != 4:
            print('usage: guest_probes.py --grade-p17 <P17-artifact.jsonl> <P17-control.jsonl>', file=sys.stderr)
            return 2
        verdict, why, facts = regrade_p17(sys.argv[2], sys.argv[3])
        print(json.dumps({'probe': 'P17', 'graded_from': sys.argv[2:], 'verdict': verdict, 'why': why, 'facts': facts},
                         indent=1))
        return 0 if verdict == 'PASS' else 1
    if len(sys.argv) > 1 and sys.argv[1] == '--grade-p5':
        if len(sys.argv) != 4:
            print('usage: guest_probes.py --grade-p5 <P5-lead.jsonl> <task id>', file=sys.stderr)
            return 2
        verdict, why, facts = regrade_p5(sys.argv[2], sys.argv[3])
        print(json.dumps({'probe': 'P5', 'graded_from': sys.argv[2], 'verdict': verdict, 'why': why, 'facts': facts},
                         indent=1))
        return 0 if verdict == 'PASS' else 1
    ap = argparse.ArgumentParser()
    ap.add_argument('--payload', required=True)
    ap.add_argument('--engine-commit', required=True)
    ap.add_argument('--only', default='')
    ap.add_argument('--survey', action='store_true')
    ap.add_argument('--probe-timeout', type=int, default=900)
    ap.add_argument('--walk-binary', default='')
    ap.add_argument('--work-walk-binary', default='')
    ap.add_argument('--runtime', default='')
    ap.add_argument('--w4-cells', default='', help='comma-separated work crash-matrix cells for W4 (default: all)')
    ap.add_argument('--w3-cells', default='', help='comma-separated crash-matrix cells for W3 (default: all)')
    a = ap.parse_args()
    unknown_probes = unknown_probe_ids(a.only)
    if unknown_probes:  # like an unknown cell below: refused before any guest work, never skipped
        print('NOT A PROBE: %s (known: %s)' % (', '.join(unknown_probes), ', '.join(pid for pid, _ in PROBES)),
              file=sys.stderr)
        return 2
    w3_wanted(a.w3_cells)  # an unknown cell is refused before any guest work
    w4_wanted(a.w4_cells)
    p = Paths(a.payload)
    p.results.mkdir(parents=True, exist_ok=True)
    setup = {'engine_commit': a.engine_commit}
    try:
        setup_fixture(p, setup)
    except Exception:  # noqa: BLE001
        setup['error'] = traceback.format_exc()
        (p.results / 'setup.json').write_text(json.dumps(setup, indent=1) + '\n')
        print('SETUP FAILED:\n' + setup['error'])
        return 2
    code, out, _ = run([GUEST_CLAUDE, '--version'])
    setup['claude_version'] = out.strip()
    ctx = Context(p, a.probe_timeout)
    ctx.walk_binary = a.walk_binary
    ctx.work_walk_binary = a.work_walk_binary
    ctx.runtime = a.runtime
    ctx.w4_cells = a.w4_cells
    ctx.w3_cells = a.w3_cells
    setup['ssh_auth_sock_method'] = ctx.ssh_method
    setup['supervisor_python'] = ctx.python
    setup['stored_names'] = sorted(ctx.stored)
    (p.results / 'setup.json').write_text(json.dumps(setup, indent=1) + '\n')
    if a.survey:
        print(json.dumps(setup, indent=1))
        return 0
    wanted = [x.strip() for x in a.only.split(',') if x.strip()]
    summary = {'claude_version': setup['claude_version'], 'engine_commit': a.engine_commit, 'probes': {}}
    worst = 0
    for pid, fn in PROBES:
        if (wanted and pid not in wanted) or (pid in EXPLICIT and pid not in wanted):
            continue
        record = {'probe': pid, 'claude_version': setup['claude_version'], 'started': time.time()}
        print('=== %s' % pid, flush=True)
        if pid in RETIRED:
            verdict, why = 'RETIRED', RETIRED[pid]
        else:
            try:
                verdict, why = fn(ctx, record)
            except Exception:  # noqa: BLE001
                verdict, why = 'ERROR', traceback.format_exc()[-3000:]
        if ctx.spawned:
            try:
                record['cleanup'] = ctx.clean_up_teammates()
            except Exception:  # noqa: BLE001
                record['cleanup'] = {'error': traceback.format_exc()[-1500:]}
        record.update(verdict=verdict, why=why, seconds=round(time.time() - record['started'], 1))
        (p.results / ('%s.json' % pid)).write_text(json.dumps(redact(record), indent=1, default=str) + '\n')
        summary['probes'][pid] = {'verdict': verdict, 'why': why if verdict != 'ERROR' else why.splitlines()[-1]}
        print('%s %s: %s' % (pid, verdict, summary['probes'][pid]['why']), flush=True)
        worst = worse_exit(worst, verdict)
    (p.results / 'summary.json').write_text(json.dumps(summary, indent=1) + '\n')
    return worst


if __name__ == '__main__':
    sys.exit(main())
