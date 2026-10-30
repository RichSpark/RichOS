#!/usr/bin/env python3
"""operator_fences_admin.py: `operator-fences.sh install|on|off|status|uninstall`.

ONE INSTALLER OWNS THE CHAIN (spec r3 e8; Frank §3). `install` writes the
launcher as <common>/hooks/reference-transaction in each repository and moves
whatever hook was there into reference-transaction.d/, where the launcher runs
it on every transaction, refused ones included:

    reference-transaction.d/20-ref-transaction-forensics.sh   the recorder
    reference-transaction.d/50-repository-hook                  any other hook

`install-ref-forensics.sh` and femcboost's `.githooks/install.sh` learn the
launcher and write into those two slots instead of over it.

THE SWITCH LIVES IN THE LAUNCHERS (Frank G12). `on` and `off` rewrite one line
in each launcher this command owns; nothing in any repository's working tree is
touched, so an emergency `off` needs no lease and leaves every repository as
clean as it found it. The entity's orchestration.config line OPERATOR_FENCES is
the DECLARATION: `on` refuses unless it says "on", and `status` (and the
integrity probe through it) reports any disagreement.

THE LEASE HOME IS BAKED IN (Frank G10). The installer resolves the home and
the per-repository key with workspaces.py's own land_lock_path() and writes
both into the launcher, which is the only place the fence reads them from.

NOT DONE HERE, on purpose: ~/.codex/AGENTS.md is never edited by this command.
r3 e1 had the installer write a Codex block there; the brief for this build put
that text in the handoff for Rich to apply by hand, because it changes how the
CEO's Codex works. See the as-built record.
"""
import hashlib
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
sys.path.insert(0, HERE)
import operator_fences as F   # noqa: E402

ENGINE = os.path.realpath(os.path.join(HERE, "..", ".."))
TEMPLATE = os.path.join(HERE, "operator-fence-launcher.sh")
PROGRAM_SOURCE = os.path.join(HERE, "operator_fences.py")
PROGRAM_FILES = F.PROGRAM_FILES   # what install copies beside the hook
RECORDER_MARKER = "ref-transaction-forensics.sh"
REGISTRY_NAME = "operator-fences.json"
SETTINGS_LOG_NAME = "operator-fence-settings.jsonl"
_ASSIGN = re.compile(r'^\s*([A-Z_][A-Z0-9_]*)=(?:"([^"]*)"|\'([^\']*)\'|(\S*))\s*(?:#.*)?$')


def say(text):
    sys.stdout.write(text.rstrip("\n") + "\n")


def declaration(entity):
    """KEY -> value for the plain assignments of <entity>/orchestration.config,
    except SECOND_REVIEW_REPOS: its key holds what F.review_repos, its one
    reader, returns from bash sourcing the file, ([name] or None, why)."""
    config = os.path.join(entity, "orchestration.config")
    try:
        with open(config, encoding="utf-8") as fh:
            text = fh.read()
    except OSError:
        return None
    out = {}
    for line in text.splitlines():
        m = _ASSIGN.match(line)
        if m and m.group(1) != F.REVIEW_KEY:
            out[m.group(1)] = next(g for g in m.groups()[1:] if g is not None)
    out[F.REVIEW_KEY] = F.review_repos(config)
    return out


def review_unreadable(decl, config):
    """Why the declaration's SECOND_REVIEW_REPOS cannot be read, or "".
    `config` names the declaration in the sentence."""
    why = ((decl or {}).get(F.REVIEW_KEY) or (None, ""))[1]
    if not why:
        return ""
    return ("%s in %s cannot be read: %s; it must source cleanly in bash and set %s=\"<names>\""
            % (F.REVIEW_KEY, config, why, F.REVIEW_KEY))


def registry_path():
    return os.path.join(F._default_home(), REGISTRY_NAME)


def settings_log_path():
    """The fence settings record (every `off`, with its authority): beside the lease, in the
    machine's one Claude home (machine_settings.claude_home), whichever Claude account the caller
    spends. Until 2026-10-28 it followed CLAUDE_CONFIG_DIR, so an `off` from a Work lease went to
    claude-accounts/2 and a terminal reading ~/.claude never saw it."""
    return os.path.join(F._default_home(), SETTINGS_LOG_NAME)


def read_registry():
    return F.read_json(registry_path()) or {"repositories": {}}


def write_registry(reg):
    F.write_json_atomic(registry_path(), reg)


def resolve_entity(opts, reg):
    entity = opts.get("--entity") or (os.environ.get("RICHOS_ENTITY_ROOT") or "").strip() or reg.get("entity")
    if not entity:
        raise SystemExit("operator-fences: name the entity (--entity <path>): the repository whose "
                         "orchestration.config declares OPERATOR_FENCES")
    entity = os.path.realpath(os.path.expanduser(entity))
    if declaration(entity) is None:
        raise SystemExit("operator-fences: %s has no orchestration.config" % entity)
    return entity


def declared_repos(decl):
    raw = (decl or {}).get("OPERATOR_FENCES_REPOS", "")
    return [os.path.realpath(os.path.expanduser(p)) for p in raw.split()]


def declared_reviews(decl, main):
    """The review ledger the launcher of `main` must carry: the second review's
    ledger when the declaration's SECOND_REVIEW_REPOS lists it, else "". A
    declaration whose SECOND_REVIEW_REPOS cannot be read never gets here:
    cmd_install refuses it first and review_problems reports it."""
    names, why = (decl or {}).get(F.REVIEW_KEY) or (None, "")
    if why:
        raise SystemExit("operator-fences: REFUSED. SECOND_REVIEW_REPOS cannot be read: %s" % why)
    if F.review_listed(names, main):
        return F.review_ledger_default()
    return ""


def review_problems(repo, conf, decl):
    """[problem] when the launcher's review ledger disagrees with the
    declaration: an edited SECOND_REVIEW_REPOS does nothing until `install`
    copies it, as for LAND_LEASE_HOLDERS."""
    if decl is None:
        return []
    unreadable = review_unreadable(decl, "the entity's orchestration.config")
    if unreadable:
        return ["%s: %s, so whether it is reviewed cannot be established; fix it, then run operator-fences.sh "
                "install" % (repo, unreadable)]
    paths = F.repo_paths(repo)
    want = declared_reviews(decl, paths["main"] if paths else repo)
    have = conf.get("REVIEWS") or ""
    if want == have:
        return []
    if want and not have:
        return ["%s: SECOND_REVIEW_REPOS lists it but its launcher does not refuse unreviewed work; run "
                "operator-fences.sh install" % repo]
    if have and not want:
        return ["%s: its launcher refuses unreviewed work but SECOND_REVIEW_REPOS does not list it; run "
                "operator-fences.sh install" % repo]
    return ["%s: its launcher reads the review ledger %s, not %s; run operator-fences.sh install"
            % (repo, have, want)]


def chain_reachable(main):
    """Will a repository-local reference-transaction hook actually run? The
    same test install-ref-forensics.sh makes: with core.hooksPath set, the
    dispatcher there must chain to the repository's own hook."""
    rc, out, _ = F.git(main, "config", "--get", "core.hooksPath")
    hooks_path = out.strip() if rc == 0 else ""
    if not hooks_path:
        return True, "(unset)"
    dispatch = os.path.join(os.path.expanduser(hooks_path), "reference-transaction")
    real = os.path.realpath(dispatch)
    try:
        with open(real, encoding="utf-8", errors="replace") as fh:
            chains = "git-common-dir" in fh.read()
    except OSError:
        return False, hooks_path
    # Git runs a hook only when it is executable: a dispatcher that chains but
    # is mode 0644 is not reachable (P5-44).
    return chains and os.access(real, os.X_OK), hooks_path


def launcher_text(values):
    with open(TEMPLATE, encoding="utf-8") as fh:
        text = fh.read()
    for key, value in values.items():
        if '"' in value or "\n" in value or "$" in value or "`" in value:
            raise SystemExit("operator-fences: refusing to bake %s=%r into a hook" % (key, value))
        text = text.replace("@%s@" % key, value)
    return set_state_text(text, values.get("STATE", "off"))


def set_state_text(text, state):
    return re.sub(r'(?m)^OPERATOR_FENCES_STATE="[^"]*"$', 'OPERATOR_FENCES_STATE="%s"' % state, text, count=1)


def write_executable(path, text):
    tmp = "%s.%d.tmp" % (path, os.getpid())
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.chmod(tmp, 0o755)
    os.replace(tmp, path)


def chain_members(common):
    d = os.path.join(common, "hooks", F.CHAIN_DIR)
    try:
        return sorted(n for n in os.listdir(d) if os.path.isfile(os.path.join(d, n)))
    except OSError:
        return []


def copy_program(program_dir):
    """Copies the engine's fence program into `program_dir` and returns its path.
    The reference-transaction fence and managed land use the same recovery
    acceptance predicate, so its state reader and settings dependency ship beside
    the standalone fence rather than duplicating the predicate. Each file arrives
    by rename, so a hook starting meanwhile reads the old file or the new one."""
    os.makedirs(program_dir, exist_ok=True)
    for name in PROGRAM_FILES:
        dest = os.path.join(program_dir, name)
        tmp = "%s.%d.tmp" % (dest, os.getpid())
        shutil.copyfile(os.path.join(HERE, name), tmp)
        if name == "operator_fences.py":
            os.chmod(tmp, 0o755)  # copyfile carries no mode; check_one refuses a fence program git cannot run
        os.replace(tmp, dest)
    return os.path.join(program_dir, "operator_fences.py")


def bring_current(repo):
    """A STALE INSTALLED FENCE NEVER JUDGES A LAND (2026-10-19). The launcher
    runs a COPY of the engine's fence, made at `install`; nothing re-made it when
    the engine changed, so a land that passed its whole land check (1215 s) was
    refused by a copy that predated the follow-up rulings it needed. The land
    command calls this before it merges anything: when the installed copy differs
    from the engine's, the engine's is copied in (and the launcher's digest line
    follows). Only the program changes; the launcher's state, holders and review
    ledger stay what `install` baked. Returns the names replaced, [] when the
    copy is current or no launcher of ours is installed. Raises OSError when it
    cannot write them; the land then refuses, naming `install`."""
    conf, _paths = F.launcher_for(repo)
    if not conf or not conf.get("PROGRAM"):
        return []
    program_dir = os.path.dirname(conf["PROGRAM"])
    stale = [n for n in PROGRAM_FILES
             if F.file_digest(os.path.join(program_dir, n)) != F.file_digest(os.path.join(HERE, n))]
    if not stale:
        return []
    program = copy_program(program_dir)
    with open(conf["LAUNCHER"], encoding="utf-8") as fh:
        text = fh.read()
    write_executable(conf["LAUNCHER"], re.sub(r'(?m)^OPERATOR_FENCES_PROGRAM_DIGEST="[^"]*"$',
                                              'OPERATOR_FENCES_PROGRAM_DIGEST="%s"' % F.file_digest(program),
                                              text, count=1))
    return stale


def install_one(repo, entity, decl):
    paths = F.repo_paths(repo)
    if not paths or not paths["main"]:
        raise SystemExit("operator-fences: %s is not a repository with a main checkout" % repo)
    main, common = paths["main"], paths["common"]
    rc, out, _ = F.git(main, "config", "--get", "extensions.refStorage")
    if rc == 0 and out.strip() == "reftable":
        raise SystemExit("operator-fences: %s uses the reftable back end; the fence reads the files back end, "
                         "so it is not installed there" % main)
    ok, hooks_path = chain_reachable(main)
    if not ok:
        raise SystemExit("operator-fences: core.hooksPath=%s does not chain to %s's own reference-transaction "
                         "hook, so a launcher there would never run. Refusing." % (hooks_path, main))
    home, key = F.keyed_paths(main)
    hooks = os.path.join(common, "hooks")
    target = os.path.join(hooks, "reference-transaction")
    chain = os.path.join(hooks, F.CHAIN_DIR)
    os.makedirs(chain, exist_ok=True)
    state = "off"
    existing = F.read_launcher(target)
    moved = ""
    if existing:
        state = existing.get("STATE", "off")
    elif os.path.exists(target):
        with open(target, encoding="utf-8", errors="replace") as fh:
            body = fh.read()
        slot = F.RECORDER_SLOT if RECORDER_MARKER in body else F.REPOSITORY_SLOT
        dest = os.path.join(chain, slot)
        if os.path.exists(dest):
            with open(dest, encoding="utf-8", errors="replace") as fh:
                if fh.read() != body:
                    raise SystemExit("operator-fences: %s already holds a different hook; refusing to overwrite "
                                     "either one" % dest)
            os.unlink(target)
        else:
            shutil.move(target, dest)
            os.chmod(dest, 0o755)
        moved = slot
    program = copy_program(os.path.join(hooks, "operator-fences"))
    digest = F.file_digest(program)
    holders = " ".join(decl.get("LAND_LEASE_HOLDERS", "").split())
    write_executable(target, launcher_text({
        "STATE": state, "REPO": main, "COMMON": common, "HOME": home, "KEY": key, "HOLDERS": holders,
        "REVIEWS": declared_reviews(decl, main), "ENGINE": ENGINE, "PROGRAM": program, "DIGEST": digest}))
    return {"main": main, "common": common, "state": state, "moved": moved, "chain": chain_members(common),
            "hooks_path": hooks_path}


def cmd_install(opts, repos):
    reg = read_registry()
    entity = resolve_entity(opts, reg)
    decl = declaration(entity)
    unreadable = review_unreadable(decl, os.path.join(entity, "orchestration.config"))
    if unreadable:
        # Before any launcher or registry entry is written: installing now would
        # record "not reviewed" from a line a shell may read as listing it.
        raise SystemExit("operator-fences: REFUSED, nothing installed. %s." % unreadable)
    repos = repos or declared_repos(decl)
    if not repos:
        raise SystemExit("operator-fences: no repositories: pass --repo or declare OPERATOR_FENCES_REPOS in %s"
                         % os.path.join(entity, "orchestration.config"))
    reg["entity"] = entity
    for repo in repos:
        r = install_one(repo, entity, decl)
        # The land command reads "entity" and "reviewed" (workspaces.py). "reviewed" is
        # THE DURABLE RECORD that this repository requires a second review, taken from
        # the declaration now: removing or breaking that declaration later never
        # switches the review off at the land; only the next install does (CEO §113).
        reg["repositories"][r["main"]] = {"common": r["common"], "chain": r["chain"], "installed": F.iso(),
                                          "entity": entity, "reviewed": bool(declared_reviews(decl, r["main"]))}
        say("installed  %s  state=%s  chain=%s%s" % (r["main"], r["state"], ",".join(r["chain"]) or "(empty)",
                                                    "  (moved the existing hook to %s)" % r["moved"] if r["moved"]
                                                    else ""))
    write_registry(reg)
    return 0


def target_repos(opts, repos):
    if repos:
        return [os.path.realpath(os.path.expanduser(r)) for r in repos]
    return sorted(read_registry().get("repositories", {}))


def cmd_set_state(opts, repos, state):
    repos = target_repos(opts, repos)
    if not repos:
        say("operator-fences: nothing is installed, so there is nothing to turn %s." % state)
        return 0 if state == "off" else 1
    if state == "on":
        reg = read_registry()
        entity = resolve_entity(opts, reg)
        if (declaration(entity) or {}).get("OPERATOR_FENCES") != "on":
            say("operator-fences: REFUSED. %s/orchestration.config declares OPERATOR_FENCES=%r. Declare it \"on\" "
                "first; the declaration and the launchers must agree." % (
                    entity, (declaration(entity) or {}).get("OPERATOR_FENCES")))
            return 1
        problems = [p for repo in repos for p in check_one(repo, "on-ready")]
        if problems:
            say("operator-fences: REFUSED to turn the fences on:\n  " + "\n  ".join(problems))
            return 1
    off_authorities = {}
    if state == "off":
        reason = str(opts.get("--reason") or "").strip()
        if not reason:
            say("operator-fences: REFUSED. off requires --reason; review/recovery acceptance remains active.")
            return 1
        # Validate every target before changing any launcher. Authority comes
        # from native session/kernel ancestry and the existing operator policy,
        # never a caller-supplied actor, acknowledgement or environment switch.
        for repo in repos:
            conf, paths = F.launcher_for(repo)
            if not conf:
                continue
            try:
                holder = F._identify_holder(opts, conf)
            except (SystemExit, ValueError) as error:
                say("operator-fences: REFUSED to turn off %s: %s" % (repo, error))
                return 1
            if not holder or holder.get("error"):
                say("operator-fences: REFUSED: "+str((holder or {}).get("error") or "operator authority unavailable"))
                return 1
            bar = F._claim_bars(holder)
            if bar:
                say("operator-fences: REFUSED: "+bar)
                return 1
            off_authorities[repo] = dict(actor=holder, reason=reason, authority="verified operator under existing land-holder policy")
    rc = 0
    for repo in repos:
        paths = F.repo_paths(repo)
        target = os.path.join(paths["common"], "hooks", "reference-transaction") if paths else ""
        conf = F.read_launcher(target) if target else None
        if not conf:
            say("operator-fences: %s has no launcher; skipped." % repo)
            rc = 1 if state == "on" else rc
            continue
        with open(target, encoding="utf-8") as fh:
            text = fh.read()
        if state=="off":
            F.append_jsonl(settings_log_path(),
                dict(off_authorities[repo], at=F.iso(), repository=paths["main"], setting="OPERATOR_FENCES", value="off",
                     previous=conf.get("STATE"), review_enforcement=bool(conf.get("REVIEWS"))))
        write_executable(target, set_state_text(text, state))
        say("%s  %s" % (state.upper(), repo))
    return rc


def check_one(repo, mode, decl=None):
    """Problems with one repository's install, as sentences. mode 'on-ready'
    skips the state check and the holder check (used before turning it on).
    `decl` is the entity's declaration, when known, for the holder comparison."""
    problems = []
    paths = F.repo_paths(repo)
    if not paths:
        return ["%s: not a repository" % repo]
    target = os.path.join(paths["common"], "hooks", "reference-transaction")
    conf = F.read_launcher(target)
    if not conf:
        return ["%s: the launcher is not installed" % repo]
    if not os.access(target, os.X_OK):
        problems.append("%s: the launcher %s is not executable, so git will not run it" % (repo, target))
    program = conf.get("PROGRAM", "")
    if program and os.path.isfile(program) and not os.access(program, os.X_OK):
        problems.append("%s: the fence program %s is not executable" % (repo, program))
    if mode != "on-ready" and conf.get("STATE") != "on":
        problems.append("%s: the fence is off" % repo)
    if F.file_digest(conf.get("PROGRAM", "")) != F.file_digest(PROGRAM_SOURCE):
        problems.append("%s: the installed fence program differs from the engine's; run operator-fences.sh "
                        "install" % repo)
    for name in ("recovery.py", "machine_settings.py"):
        source = os.path.join(HERE, name)
        installed = os.path.join(os.path.dirname(conf.get("PROGRAM") or ""), name)
        if os.path.exists(source) and F.file_digest(installed) != F.file_digest(source):
            problems.append("%s: installed %s differs from the engine; run operator-fences.sh install" % (repo, name))
    ok, hooks_path = chain_reachable(paths["main"])
    if not ok:
        problems.append("%s: core.hooksPath=%s does not reach the launcher" % (repo, hooks_path))
    expected = (read_registry().get("repositories", {}).get(paths["main"]) or {}).get("chain") or []
    missing = [m for m in expected if m not in chain_members(paths["common"])]
    if missing:
        problems.append("%s: chain member(s) %s are gone" % (repo, ", ".join(missing)))
    chain_dir = os.path.join(paths["common"], "hooks", F.CHAIN_DIR)
    not_run = [m for m in expected if m not in missing and not os.access(os.path.join(chain_dir, m), os.X_OK)]
    if not_run:
        problems.append("%s: chain member(s) %s are not executable, so the chain skips them"
                        % (repo, ", ".join(not_run)))
    try:
        home, key = F.keyed_paths(paths["main"])
        if os.path.realpath(home) != os.path.realpath(conf.get("HOME", "")) or key != conf.get("KEY"):
            problems.append("%s: the launcher's lease home %s/%s differs from this environment's %s/%s"
                            % (repo, conf.get("HOME"), conf.get("KEY"), home, key))
    except Exception as error:  # noqa: BLE001
        problems.append("%s: cannot compute the lease home (%s)" % (repo, error))
    if mode != "on-ready":
        problems.extend(holder_problems(repo, conf, decl)[0])
        problems.extend(review_problems(repo, conf, decl))
    return problems


def holder_problems(repo, conf, decl=None):
    """([problem, ...], [note, ...]) about the launcher's declared holders.

    A holder whose declared executable is gone is found BEFORE a land needs it,
    by status and the integrity probe that runs it at session start, rather than
    by the refusal of somebody's commit (2026-09-30: a ChatGPT update moved
    Codex and every Codex land was refused until the path was edited by hand).
      * gone, and nothing in its bundle would pass as it: a PROBLEM, because
        that holder cannot take a lease at all;
      * gone, but a same-named, same-team executable is in its bundle: a NOTE
        naming it, because the fence still recognizes it and nothing is refused,
        and the declaration should catch up;
      * the launcher's holders differ from the declaration's LAND_LEASE_HOLDERS:
        a PROBLEM, because an edited declaration does nothing until `install`
        copies it into the launcher."""
    problems, notes = [], []
    baked = " ".join((conf.get("HOLDERS") or "").split())
    if decl is not None:
        declared = " ".join((decl.get("LAND_LEASE_HOLDERS") or "").split())
        if declared != baked:
            problems.append("%s: the launcher names holders %r but the declaration's LAND_LEASE_HOLDERS is %r; "
                            "run operator-fences.sh install" % (repo, baked, declared))
    for kind, exe in sorted(F.declared_holders(conf).items()):
        if os.path.exists(exe):
            continue
        found = F.bundle_candidates(exe)
        if found:
            notes.append("%s: the declared %s executable %s is gone; the fence recognizes %s in the same bundle by "
                         "its name and signing team. Update LAND_LEASE_HOLDERS to it, then run operator-fences.sh "
                         "install" % (repo, kind, exe, found[0]))
        else:
            problems.append("%s: the declared %s executable %s is gone, and nothing in %s would pass as it, so a "
                            "%s land cannot take the lease. Find where it moved, update LAND_LEASE_HOLDERS, then "
                            "run operator-fences.sh install" % (repo, kind, exe, F.app_bundle(exe) or "its bundle",
                                                                 kind))
    return problems, notes


def cmd_status(opts, repos, declaration_check):
    reg = read_registry()
    repos = target_repos(opts, repos)
    entity = opts.get("--entity") or (os.environ.get("RICHOS_ENTITY_ROOT") or "").strip() or reg.get("entity")
    decl = declaration(os.path.realpath(entity)) if entity else None
    declared = (decl or {}).get("OPERATOR_FENCES")
    lines, problems, notes = [], [], []
    for repo in repos:
        paths = F.repo_paths(repo)
        conf = F.read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction")) if paths else None
        lines.append("%s  launcher=%s  state=%s  chain=%s" % (
            repo, "present" if conf else "ABSENT", (conf or {}).get("STATE", "-"),
            ",".join(chain_members(paths["common"])) if paths else "-"))
    if declaration_check:
        # The integrity probe's question: do the launchers agree with the declaration?
        if declared in (None, "", "off"):
            for repo in repos:
                paths = F.repo_paths(repo)
                conf = F.read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction")) \
                    if paths else None
                if conf and conf.get("STATE") != "off":
                    problems.append("%s: the declaration is off but the launcher is on" % repo)
        else:
            if not repos:
                problems.append("the declaration is on but no repository has the launcher")
            for repo in repos:
                problems.extend(check_one(repo, "status", decl))
    else:
        if declared != "on":
            problems.append("the declaration (%s) says OPERATOR_FENCES=%r" % (entity, declared))
        if not repos:
            problems.append("no repository has the launcher installed")
        for repo in repos:
            problems.extend(check_one(repo, "status", decl))
    if declared == "on":
        for repo in repos:
            paths = F.repo_paths(repo)
            conf = F.read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction")) \
                if paths else None
            if conf:
                notes.extend(holder_problems(repo, conf)[1])
    for line in lines:
        say(line)
    for n in notes:
        say("NOTE  " + n)
    for p in problems:
        say("PROBLEM  " + p)
    say("operator-fences: %s" % ("OK" if not problems else "NOT OK (%d problem%s)" % (
        len(problems), "" if len(problems) == 1 else "s")))
    return 0 if not problems else 1


def cmd_holders_notice(opts, repos):
    """One line per declared land-lease holder that will not be found where its
    launcher declares it, for the session-start banner (engine-status.sh). Reads
    only launchers (the runtime truth), never orchestration.config: the
    declaration-versus-launcher comparison is `status`'s. Repositories are the
    --repo ones, else every one the registry records. Silent, exit 0, when no
    launcher is on or every holder is where it is declared. Never exits non-zero
    for a finding: it reports, the fence decides."""
    for repo in target_repos(opts, repos):
        paths = F.repo_paths(repo)
        conf = F.read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction")) if paths else None
        if not F.fenced(conf):
            continue
        problems, notes = holder_problems(repo, conf)
        for line in problems + notes:
            say(line)
    return 0


def cmd_uninstall(opts, repos):
    reg = read_registry()
    repos = target_repos(opts, repos)
    rc = 0
    for repo in repos:
        paths = F.repo_paths(repo)
        if not paths:
            continue
        hooks = os.path.join(paths["common"], "hooks")
        target = os.path.join(hooks, "reference-transaction")
        if not F.read_launcher(target):
            say("operator-fences: %s has no launcher of ours; nothing removed." % repo)
            continue
        members = chain_members(paths["common"])
        if len(members) > 1:
            say("operator-fences: REFUSED for %s: the chain holds %s, and a single reference-transaction hook "
                "can hold only one. Remove all but one by hand, then run uninstall again." % (repo, ", ".join(members)))
            rc = 1
            continue
        chain = os.path.join(hooks, F.CHAIN_DIR)
        if members:
            os.replace(os.path.join(chain, members[0]), target)
        else:
            os.unlink(target)
        shutil.rmtree(os.path.join(hooks, "operator-fences"), ignore_errors=True)
        try:
            os.rmdir(chain)
        except OSError:
            pass
        reg.get("repositories", {}).pop(paths["main"], None)
        say("uninstalled  %s%s" % (repo, "  (restored %s)" % members[0] if members else ""))
    write_registry(reg)
    return rc


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        say("usage: operator-fences.sh install|on|off|status|uninstall|holders-notice|refresh [--repo <path>]... "
            "[--entity <path>] [--declaration-check] [--reason <off reason>]")
        return 0 if argv else 2
    sub, rest = argv[0], argv[1:]
    repos, opts, i = [], {}, 0
    while i < len(rest):
        a = rest[i]
        if a == "--repo" and i + 1 < len(rest):
            repos.append(rest[i + 1])
            i += 2
        elif a == "--entity" and i + 1 < len(rest):
            opts["--entity"] = rest[i + 1]
            i += 2
        elif a in ("--holder", "--holder-pid") and i+1 < len(rest):
            opts[a] = rest[i+1]
            i += 2
        elif a == "--reason" and i + 1 < len(rest):
            opts[a] = rest[i+1]
            i += 2
        elif a == "--declaration-check":
            opts[a] = True
            i += 1
        else:
            raise SystemExit("operator-fences: unknown argument %r" % a)
    if sub == "install":
        return cmd_install(opts, repos)
    if sub in ("on", "off"):
        return cmd_set_state(opts, repos, sub)
    if sub == "status":
        return cmd_status(opts, repos, bool(opts.get("--declaration-check")))
    if sub == "uninstall":
        return cmd_uninstall(opts, repos)
    if sub == "holders-notice":
        return cmd_holders_notice(opts, repos)
    if sub == "refresh":
        return cmd_refresh(opts, repos)
    raise SystemExit("operator-fences: unknown command %r" % sub)


def cmd_refresh(opts, repos):
    """`refresh --repo <path>`: bring_current, the land command's own refresh, for a caller that is
    not the land command (autocheck's LAND path, verification reliability PRD A9). Run from the
    fence the engine has now, it copies that fence over an installed copy of ANY age: an installed
    copy older than bring_self_current cannot bring itself current (the copy of 2026-10-18,
    6b920e274, has no such step). Exit 1 when a copy cannot be written."""
    if not repos:
        raise SystemExit("operator-fences: refresh needs --repo <path>")
    for repo in repos:
        try:
            replaced = bring_current(repo)
        except OSError as exc:
            say("operator-fences: the installed fence of %s could not be brought current (%s); run "
                "operator-fences.sh install --repo %s" % (repo, exc, repo))
            return 1
        say("refreshed  %s  %s" % (repo, ", ".join(replaced) if replaced else "already current"))
    return 0
