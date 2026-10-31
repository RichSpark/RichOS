#!/usr/bin/env node
// walk-phone.mjs — the phone of a test VM walk: the actual RichConnect client (richos/mobile/core/client.js
// with the native ports, as mobile/test/mac-server.test.mjs runs it), talking to the Mac in the
// guest over the same API the native apps use. It replaced the guest's Safari, which drove the
// phone web page the Mac no longer serves (retired 2026-10-30).
//
//   walk-phone.mjs pair --state FILE --link 'https://<mac>:8443/#pair=CODE' [--reject]
//       pairs, says "They match" on the phone ("They do not match" with --reject), saves the phone
//       (its key and session) to FILE and prints {"words","matched","device","threads"}. The Mac
//       then activates a matched phone with its own "They match".
//   walk-phone.mjs send --state FILE --thread ID --text TEXT [--seconds N]
//       the saved phone comes back online, selects the conversation, sends TEXT from its composer
//       and waits (at most N s, default 60) for the Mac's receipt; prints {"thread","text"}.
//   walk-phone.mjs notifications --state FILE [--off] [--seconds N]
//       the saved phone switches "Notify me when Rich replies" on (off with --off), as the
//       RichConnect app's Settings do, and prints {"on","status","error"}.
//
// Runs on the host: the Mac's address is its tailnet name with a certificate `tailscale cert`
// issued, so the system's own trust store checks it, exactly as on a phone. FILE holds a test
// key that exists only for that walk; keep it under the walk's --out directory.
import { createRequire } from 'node:module';
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const require = createRequire(import.meta.url);
const { createClient } = require('../../../mobile/core/client.js');
const { createPorts } = require('../../../mobile/platform/native.js');
const release = require('../../../mobile/release-config.json');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

export function phone(saved = {}) {
  const key = saved.key ? createPrivateKey({ key: saved.key, format: 'jwk' }) : generateKeyPairSync('ec', { namedCurve: 'prime256v1' }).privateKey;
  let disk = saved.disk;
  const listeners = new Set(), streams = new Map();
  const emit = event => { for (const fn of listeners) fn(event); };
  const call = async (method, args) => {
    switch (method) {
      case 'configure': return true;
      case 'load': return structuredClone(disk);
      case 'save': disk = structuredClone(args.value); return true;
      case 'recordings': return [];
      // Notifications, as an iPhone that allowed them: the RichConnect app's Settings switch
      // "Notify me when Rich replies" asks the system (pushRequest) and then registers the token
      // the system gave it (pushInfo) with the Mac. The token is a test value for the team's TEST
      // app topic; the Mac in the guest has no RichConnect identity, so it is never delivered.
      case 'pushRequest': return { permission: 'allowed' };
      case 'pushInfo': return { permission: 'allowed', registration: { token: 'a'.repeat(64), environment: 'sandbox', topic: 'dev.richos.connect.perf', previews: false } };
      case 'pushPreview': return true;
      case 'updateInfo': return { version: release.version, build: release.build, osVersion: '16.7.16', appId: null, storefront: null, configured: false };
      case 'hash': return createHash('sha256').update(args.value).digest('hex');
      case 'publicKey': return createPublicKey(key).export({ format: 'jwk' });
      case 'sign': return sign('sha256', Buffer.from(args.input), { key, dsaEncoding: 'ieee-p1363' }).toString('base64url');
      case 'request': {
        const response = await fetch(args.url, { method: args.method, headers: args.headers, body: args.method === 'GET' ? undefined : args.body });
        return { status: response.status, headers: Object.fromEntries(response.headers), body: await response.text() };
      }
      case 'streamClose': streams.get(args.id)?.abort(); streams.delete(args.id); return true;
      case 'streamOpen': {
        const controller = new AbortController(); streams.set(args.id, controller);
        void (async () => {
          try {
            const response = await fetch(args.url, { signal: controller.signal, headers: { Accept: 'text/event-stream' } });
            if (response.status !== 200) throw Error('stream answered ' + response.status);
            emit({ kind: 'stream-open', id: args.id });
            for await (const chunk of response.body) emit({ kind: 'stream-data', id: args.id, data: Buffer.from(chunk).toString('base64') });
          } catch (error) { if (!controller.signal.aborted) emit({ kind: 'stream-error', id: args.id, error: error.message }); }
        })();
        return true;
      }
      default: throw Error('this walk phone has no ' + method);
    }
  };
  const ports = createPorts(call, fn => { listeners.add(fn); return () => listeners.delete(fn); });
  return { ports, saved: () => ({ key: key.export({ format: 'jwk' }), disk }) };
}

async function until(predicate, seconds, what, client) {
  const end = Date.now() + seconds * 1000;
  while (!predicate()) {
    if (Date.now() >= end) throw Error(what + ' within ' + seconds + ' s: ' + JSON.stringify(client.state()).slice(0, 600));
    await pause(100);
  }
}

export async function pair(file, link, matched = true) {
  const device = phone();
  const client = await createClient(device.ports);
  try {
    await client.dispatch({ type: 'pair', link });
    const { words, threads, device: id } = { ...client.state(), device: device.saved().disk.api.deviceId };
    await client.dispatch({ type: 'confirm-pair', matched });
    writeFileSync(file, JSON.stringify(device.saved()), { mode: 0o600 });
    return { words, matched, device: id, threads: (threads || []).map(t => ({ id: t.id, title: t.title })) };
  } finally { client.close(); }
}

export async function send(file, thread, text, seconds = 60) {
  const device = phone(JSON.parse(readFileSync(file, 'utf8')));
  const client = await createClient(device.ports);
  try {
    await until(() => client.state().online, seconds, 'the phone did not come online', client);
    if (client.state().selectedThreadId !== thread) {
      // Choosing a conversation reconnects the phone's stream to it.
      await client.dispatch({ type: 'select-thread', threadId: thread });
      await until(() => client.state().online, seconds, 'the phone did not come back online on ' + thread, client);
    }
    await client.dispatch({ type: 'compose', text });
    await client.dispatch({ type: 'send' });
    await until(() => client.state().outbox.length === 0, seconds, 'the Mac did not take the message', client);
    writeFileSync(file, JSON.stringify(device.saved()), { mode: 0o600 });
    return { thread, text };
  } finally { client.close(); }
}

/// The phone switches "Notify me when Rich replies" on (or off with on=false) and waits for its
/// registration to reach the Mac. The Mac records it before it tries the notification service, so
/// a service it cannot reach (the guest has no RichConnect identity) still leaves the Mac knowing.
export async function notifications(file, on = true, seconds = 60) {
  const device = phone(JSON.parse(readFileSync(file, 'utf8')));
  const client = await createClient(device.ports);
  try {
    await until(() => client.state().online, seconds, 'the phone did not come online', client);
    await client.dispatch({ type: on ? 'notifications-enable' : 'notifications-disable' });
    const settled = () => !['registering', 'disabling'].includes(client.state().notifications.status);
    await until(settled, seconds, 'the phone did not finish switching notifications ' + (on ? 'on' : 'off'), client);
    writeFileSync(file, JSON.stringify(device.saved()), { mode: 0o600 });
    return { on, status: client.state().notifications.status, error: client.state().notifications.error };
  } finally { client.close(); }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const { positionals, values } = parseArgs({ allowPositionals: true, options: {
    state: { type: 'string' }, link: { type: 'string' }, reject: { type: 'boolean' }, thread: { type: 'string' },
    text: { type: 'string' }, seconds: { type: 'string' }, off: { type: 'boolean' } } });
  const verb = positionals[0];
  try {
    if (!values.state) throw Error('--state FILE is required');
    let result;
    if (verb === 'pair' && values.link) result = await pair(values.state, values.link, !values.reject);
    else if (verb === 'send' && values.thread && values.text) result = await send(values.state, values.thread, values.text, Number(values.seconds || 60));
    else if (verb === 'notifications') result = await notifications(values.state, !values.off, Number(values.seconds || 60));
    else throw Error('usage: walk-phone.mjs pair --state FILE --link URL [--reject] | send --state FILE --thread ID --text TEXT [--seconds N] | notifications --state FILE [--off] [--seconds N]');
    console.log(JSON.stringify(result));
    process.exit(0);
  } catch (error) { console.error('walk-phone: ' + error.message); process.exit(1); }
}
