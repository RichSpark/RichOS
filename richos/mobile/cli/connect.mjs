import { validateStorage } from './machine-settings.mjs';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

// Domain/account identifiers belong in the operator's private deployment record.
// This artifact contains only the reproducible upload and its source digest.
export function artifact() {
  const source = readFileSync(new URL('../service/connect-bootstrap.mjs', import.meta.url), 'utf8');
  return {
    stage: 'bootstrap',
    sha256: createHash('sha256').update(source).digest('hex'),
    metadata: {
      main_module: 'connect-bootstrap.mjs',
      compatibility_date: '2026-09-21',
      bindings: [],
      // A closed-service rollback must retain the managed database and credential.
      keep_bindings: ['secret_text', 'd1'],
      observability: { enabled: false },
      logpush: false,
      tail_consumers: [],
    },
    modules: [{ name: 'connect-bootstrap.mjs', type: 'application/javascript+module', source }],
    subdomain: { enabled: false, previews_enabled: false },
  };
}

export function managedArtifact(profile) {
  for (const name of ['accountId', 'zoneId']) if (!/^[a-f0-9]{32}$/.test(profile[name])) throw Error('Invalid deployment identifier');
  if (!/^[a-f0-9-]{36}$/.test(profile.databaseId) || !/^[a-z0-9][a-z0-9.-]+\.[a-z]{2,}$/.test(profile.domain)) throw Error('Invalid deployment domain/database');
  if (!Number.isInteger(profile.capacity) || profile.capacity < 1) throw Error('Host capacity must be a whole number of at least 1');
  const push = profile.push || {};
  if (Object.keys(push).some(k=>!['teamId','sandboxKeyId','productionKeyId','topics'].includes(k))) throw Error('Invalid push profile');
  for (const key of ['teamId','sandboxKeyId','productionKeyId']) if (push[key] && !/^[A-Z0-9]{10}$/.test(push[key])) throw Error('Invalid push identifier');
  // Preserved and development iPhone IDs remain valid alongside RichConnect's permanent ID, and
  // `dev.richos.connect.perf` is the team's TEST copy, which pushes under its own ID (CEO 2026-10-03).
  if (push.topics && (!Array.isArray(push.topics) || push.topics.some(t=>!['dev.richos.mobile.loop','dev.richos.mobile.integration','dev.richos.native.ios','dev.richos.connect','dev.richos.connect.perf'].includes(t)))) throw Error('Invalid push topics');
  // Android push: the Firebase project ID and the allowed Android application IDs are identifiers.
  // The service-account key is the Worker secret FCM_SERVICE_ACCOUNT, never part of a profile.
  const fcm = profile.fcm || {};
  if (Object.keys(fcm).some(k=>!['projectId','apps'].includes(k)) || (Object.keys(fcm).length && !fcm.projectId)) throw Error('Invalid FCM profile');
  if (fcm.projectId && !/^[a-z][a-z0-9-]{4,28}[a-z0-9]$/.test(fcm.projectId)) throw Error('Invalid FCM project ID');
  if (fcm.projectId && (!Array.isArray(fcm.apps) || !fcm.apps.length || fcm.apps.some(a=>typeof a !== 'string' || !/^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/.test(a)))) throw Error('Invalid FCM application IDs');
  const names = ['connect-worker.mjs', 'connect/auth.mjs', 'connect/store.mjs', 'connect/provider.mjs', 'connect/lifecycle.mjs', 'connect/apns.mjs', 'connect/fcm.mjs', 'connect/notifications.mjs', 'connect/routing.mjs'];
  return {
    stage: 'managed',
    metadata: {
      main_module: 'connect-worker.mjs', compatibility_date: '2026-09-21',
      keep_bindings: ['secret_text'], observability: { enabled: false }, logpush: false, tail_consumers: [],
      bindings: [
        { name: 'DB', type: 'd1', id: profile.databaseId },
        { name: 'AI', type: 'ai' },
        { name: 'REQUEST_LIMIT', type: 'ratelimit', namespace_id: '18443', simple: { limit: 60, period: 60 } },
        ...Object.entries({ CF_ACCOUNT_ID: profile.accountId, CF_ZONE_ID: profile.zoneId, CONNECT_DOMAIN: profile.domain,
          HOST_CAPACITY: String(profile.capacity),
          ...(push.teamId ? {APNS_TEAM_ID:push.teamId,APNS_TOPICS:(push.topics || []).join(',')} : {}),
          ...(push.sandboxKeyId ? {APNS_SANDBOX_KEY_ID:push.sandboxKeyId} : {}),
          ...(push.productionKeyId ? {APNS_PRODUCTION_KEY_ID:push.productionKeyId} : {}),
          ...(fcm.projectId ? {FCM_PROJECT_ID:fcm.projectId,FCM_APPS:fcm.apps.join(',')} : {}) }).map(([name, text]) => ({ name, type: 'plain_text', text })),
      ],
    },
    modules: names.map(name => {
      const source = readFileSync(new URL('../service/' + name, import.meta.url), 'utf8');
      return { name, type: 'application/javascript+module', source, sha256: createHash('sha256').update(source).digest('hex') };
    }),
    subdomain: { enabled: false, previews_enabled: false },
    schedules: [{ cron: '*/5 * * * *' }],
  };
}

export async function command(action, file) {
  if (action === 'lab-check') {
    if (!file) throw Error('Live proof state requires an explicit file');
    file = validateStorage(file);
    const { spawnSync } = await import('node:child_process');
    const { fileURLToPath } = await import('node:url');
    const run = spawnSync(process.execPath, ['--test', fileURLToPath(new URL('../test/mac-server.test.mjs', import.meta.url))],
      { env: { ...process.env, RICHOS_MOBILE_REMOTE_STATE: file }, encoding: 'utf8', timeout: 120000 });
    if (run.status !== 0) throw Error((run.stdout || '') + (run.stderr || '') + (run.error?.message || ''));
    return { verified: true, output: run.stdout };
  }
  if (action?.startsWith("lab-")) return (await import("../dev/connect-lab.mjs")).lab(action, file);
  if (action === 'managed-artifact' && file) return managedArtifact(JSON.parse(readFileSync(file, 'utf8')));
  if (action !== 'artifact') throw Error('Expected connect artifact or managed-artifact <profile.json>. This command prepares an upload; it does not deploy.');
  return artifact();
}
