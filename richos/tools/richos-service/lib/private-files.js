/** Files written by the service must remain in real directories outside the product. */
import fs from 'node:fs';
import path from 'node:path';
import { assertEvidenceOutsideProductRepo } from './workspace/privacy.js';

export function privateDirectory(dir, root = dir) {
  const base = assertEvidenceOutsideProductRepo(root);
  const target = path.resolve(dir);
  fs.mkdirSync(base, { recursive: true });
  const physicalBase = fs.realpathSync.native(base);
  const escapes = (relative) => relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative);
  let relative = path.relative(base, target);
  // Accept both an explicit root alias and the physical paths returned by this helper.
  if (escapes(relative)) relative = path.relative(physicalBase, target);
  if (escapes(relative)) {
    throw new Error('storage boundary: directory escapes its root');
  }
  assertEvidenceOutsideProductRepo(target);
  let current = physicalBase;
  for (const part of relative.split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    const entry = fs.lstatSync(current, { throwIfNoEntry: false });
    if (entry && (!entry.isDirectory() || entry.isSymbolicLink())) {
      throw new Error('storage boundary: child directory must not be a link');
    }
    if (!entry) fs.mkdirSync(current);
  }
  return current;
}

export function privateFile(file, root = path.dirname(file)) {
  const dir = privateDirectory(path.dirname(file), root);
  const target = path.join(dir, path.basename(file));
  const entry = fs.lstatSync(target, { throwIfNoEntry: false });
  if (entry && (!entry.isFile() || entry.isSymbolicLink() || entry.nlink !== 1)) {
    throw new Error('storage boundary: output must be a regular unlinked file');
  }
  return target;
}

export function writePrivateFile(file, data, root = path.dirname(file)) {
  const target = privateFile(file, root);
  const fd = fs.openSync(target, fs.constants.O_WRONLY | fs.constants.O_CREAT
    | (fs.constants.O_NOFOLLOW || 0), 0o600);
  try {
    const entry = fs.fstatSync(fd);
    if (!entry.isFile() || entry.nlink !== 1) {
      throw new Error('storage boundary: output must be a regular unlinked file');
    }
    fs.ftruncateSync(fd, 0);
    fs.writeFileSync(fd, data);
  } finally {
    fs.closeSync(fd);
  }
}

/** Refuse pre-existing aliases before a pipeline can read or change any session artifact. */
export function assertPrivateTree(root) {
  const dir = privateDirectory(root);
  function visit(current) {
    for (const name of fs.readdirSync(current)) {
      const file = path.join(current, name);
      try {
        const entry = fs.lstatSync(file);
        if (entry.isDirectory()) {
          if (fs.realpathSync.native(file) !== file) {
            throw new Error('storage boundary: child directory must not be a link');
          }
          visit(file);
        } else if (!entry.isFile() || entry.nlink !== 1) {
          throw new Error('storage boundary: output must be a regular unlinked file');
        }
      } catch (error) {
        // A previous live worker can finish and remove its staging directory
        // while a new launch checks the retained recording.
        if (error.code !== 'ENOENT') throw error;
      }
    }
  }
  visit(dir);
  return dir;
}

/** Native tools write in a fresh private directory, never through existing output files. */
export function withPrivateOutputs(files, run) {
  const targets = files.map((file) => privateFile(file));
  const staging = fs.mkdtempSync(path.join(path.dirname(targets[0]), '.richos-output-'));
  const staged = targets.map((file, i) => path.join(staging, `${i}-${path.basename(file)}`));
  try {
    const result = run(staged);
    for (let i = 0; i < targets.length; i += 1) {
      privateFile(staged[i], staging);
      fs.renameSync(staged[i], privateFile(targets[i]));
    }
    return result;
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}
