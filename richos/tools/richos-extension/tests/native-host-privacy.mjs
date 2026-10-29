/**
 * Refuse, before Chrome starts, a native-host run that would put a macOS privacy dialog on screen.
 *
 * Why (2026-10-08). At 23:40:28Z on 2026-10-07 the CEO's Mac showed
 * `"node" would like to access files on a removable volume` and it stayed there, unanswered, until
 * he looked at the screen 1 h 45 min later. The unified log names the asker: node pid 1518, the
 * native host that Chrome for Testing (pid 1427, started 3 s earlier by native-transport-e2e.mjs)
 * launched through the shell launcher, reading
 * /Volumes/E1TB/tmp/claude/echo-opus-extclean1/main-tree/richos/tools/richos-service/package.json
 * from a test copy of the repository on the external SSD (`AUTHREQ_PROMPTING ...
 * service=kTCCServiceSystemPolicyRemovableVolumes, subject=Sub:{.../node/26.10.0_1/bin/node}
 * Resp:{... node ...}`, then `System Policy: node(1518) deny(4) file-read-data ...`).
 *
 * Chrome starts a native host with its responsibility disclaimed, so macOS judges the host as its
 * own privacy subject (the node binary), not as Terminal or Claude Code, whose permissions every
 * other test inherits. Node had no Removable Volumes permission, so its first read on the SSD
 * asked. The gate never asked before because it runs this test from a worktree on the internal
 * disk with TMPDIR in /var/folders; only a checkout or TMPDIR on /Volumes reaches it.
 *
 * So: when anything the host reads lies under /Volumes, the subject's grant is read from the
 * user's TCC database (read-only; with no Full Disk Access the read simply fails, never asks), and
 * anything but "allowed" refuses with the way to grant it once. Nothing here ever triggers a
 * dialog itself.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export const REMOVABLE_SERVICE = 'kTCCServiceSystemPolicyRemovableVolumes';

function real(p) {
  try { return fs.realpathSync(p); } catch { return path.resolve(p); }
}

/** The paths (resolved) that sit on a volume other than the startup disk. */
export function removablePaths(paths) {
  return [...new Set(paths.filter(Boolean).map(real))].filter((p) => p.startsWith('/Volumes/'));
}

/**
 * Put what Chrome and the host it launches read OFF the removable volume, so no privacy permission
 * is asked for or needed (a nightly started from the app cannot read the TCC database, and Full Disk
 * Access is not something a user is asked to give). `dirs` maps a name to a directory; a directory
 * already off a removable volume comes back as it is, and one on a removable volume is copied under
 * `into` (without node_modules and .git) and the copy's path comes back. `into` must itself be off a
 * removable volume or this throws: staging onto the volume would move nothing.
 */
export function stageOffRemovable({ dirs, into }) {
  fs.mkdirSync(into, { recursive: true });
  if (removablePaths([into]).length) {
    throw new Error(`cannot stage off a removable volume into ${real(into)}: it is on one too (set TMPDIR to a folder on the internal disk)`);
  }
  const staged = {};
  for (const [name, dir] of Object.entries(dirs)) {
    if (!removablePaths([dir]).length) { staged[name] = dir; continue; }
    const copy = path.join(into, path.basename(dir));
    fs.cpSync(dir, copy, { recursive: true, filter: (source) => !/(^|\/)(node_modules|\.git)$/.test(source) });
    staged[name] = copy;
  }
  return staged;
}

/** 'allowed' | 'not allowed' | 'unknown (<why>)' for one path-identified subject. */
export function removableGrant(subject, home = os.homedir()) {
  const db = path.join(home, 'Library', 'Application Support', 'com.apple.TCC', 'TCC.db');
  if (!fs.existsSync(db)) return `unknown (no TCC database at ${db})`;
  const literal = `'${String(subject).replaceAll("'", "''")}'`;
  const query = `select auth_value from access where service='${REMOVABLE_SERVICE}' and client=${literal};`;
  const out = spawnSync('/usr/bin/sqlite3', ['-readonly', db, query], { encoding: 'utf8' });
  if (out.status !== 0) return `unknown (the TCC database could not be read: ${(out.stderr || out.error || '').toString().trim().slice(0, 120)})`;
  const values = out.stdout.trim().split('\n').filter(Boolean);
  return values.includes('2') ? 'allowed' : 'not allowed';
}

/**
 * null when the run cannot ask for anything; otherwise the refusal text.
 *   paths:    every path the Chrome-launched host reads (checkout, scratch dir, model)
 *   subjects: the executables macOS may judge as the host (node; the launcher interpreter)
 */
export function removableVolumeRefusal({ paths, subjects, home = os.homedir() }) {
  const removable = removablePaths(paths);
  if (!removable.length) return null;
  const missing = [...new Set(subjects.filter(Boolean).map(real))]
    .map((subject) => ({ subject, grant: removableGrant(subject, home) }))
    .filter((row) => row.grant !== 'allowed');
  if (!missing.length) return null;
  return [
    'REFUSED before starting Chrome: the native host Chrome launches is its own macOS privacy subject,',
    `and it would read ${removable.join(', ')} on a removable volume, which puts a`,
    '"would like to access files on a removable volume" dialog on the screen and holds the host until someone answers.',
    ...missing.map((row) => `  ${row.subject}: Removable Volumes permission ${row.grant}`),
    'Run it from a checkout on the internal disk with TMPDIR on the internal disk, or grant it once:',
    'System Settings > Privacy & Security > Files & Folders > node > Removable Volumes',
    '(or Full Disk Access > + > the executable named above), then run again.',
  ].join('\n');
}
