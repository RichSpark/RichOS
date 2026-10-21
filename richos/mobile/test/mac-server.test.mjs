import { validateStorage } from '../cli/machine-settings.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import { readFileSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { startMac } from '../dev/mac-server.mjs';
const require = createRequire(import.meta.url);
const { createScratch } = require('./storage.cjs');
const { createClient } = require('../core/client.js');
const { createPorts } = require('../platform/native.js');
// The native bridge reports the shipped version and build, from their one source.
const release = require('../release-config.json');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

test('actual mobile client pairs, signs, receives and resumes against the production Rust stack', { timeout: 360000 }, async t => {
  const cache = createScratch('mac-proof');
  let server, client;
  t.after(async () => { client?.close(); await server?.close(); rmSync(cache, { recursive: true, force: true }); });
  const remoteFile = process.env.RICHOS_MOBILE_REMOTE_STATE;
  if (remoteFile) validateStorage(remoteFile);
  // A FILE RICH ATTACHED (round 36 screen 3): the owned lab attaches the round's two made-up files
  // under each reply (RICHOS_MOBILE_LAB_ATTACH, mobile/dev/lab-files.py), as Rich's attach_file does.
  const attach = remoteFile ? null : spawnSync('python3', [fileURLToPath(new URL('../dev/lab-files.py', import.meta.url)), join(cache, 'files')],
    { encoding: 'utf8' });
  if (attach) assert.equal(attach.status, 0, attach.stderr);
  const attached = attach ? attach.stdout.trim().split(':') : [];
  if (attach) process.env.RICHOS_MOBILE_LAB_ATTACH = attach.stdout.trim();
  try { server = remoteFile ? null : await startMac(cache); } finally { delete process.env.RICHOS_MOBILE_LAB_ATTACH; }
  const publicRoute = !!remoteFile || process.env.RICHOS_MOBILE_USE_PUBLIC_ROUTE === '1';
  const state = remoteFile ? JSON.parse(readFileSync(remoteFile, 'utf8')) : server.state;
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const listeners = new Set(), streams = new Map(), events = [], requests = [];
  let disk;
  const voiceFile = process.env.RICHOS_MOBILE_VOICE_FIXTURE;
  const voicePhrase = process.env.RICHOS_MOBILE_VOICE_PHRASE;
  if (voiceFile) { validateStorage(voiceFile); if (!voicePhrase) throw Error('Voice proof requires a WAV and expected phrase'); }
  let replyAudio, played = false;
  // THE ROW THAT CAN BEAT THE RECEIPT. The voice proof holds the upload's answer, as a slow
  // network does, until the Mac's own `message` row for the recording has arrived on the stream,
  // then records that row as the wire carried it and as the phone holds it. It must already be a
  // voice row: until the Mac marked a phone voice note on its turn (`voiceNote`, phone/rows.rs
  // `message_kind`), this row said kind "text" and the phone drew a text bubble until the receipt.
  const pendingFrames = new Map(), voiceEchoes = [];
  let voiceClientId = null, beforeReceipt = null;
  const readFrames = (id, chunk) => {
    const parts = ((pendingFrames.get(id) || '') + Buffer.from(chunk).toString('utf8')).split('\n\n');
    pendingFrames.set(id, parts.pop());
    for (const frame of parts) {
      const lines = frame.split('\n');
      const event = lines.find(line => line.startsWith('event:'))?.slice(6).trim();
      const data = lines.filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
      if (event !== 'message' || !data) continue;
      const row = JSON.parse(data);
      if (row.role === 'ceo' && voiceClientId && row.client_id === voiceClientId) voiceEchoes.push(row);
    }
  };
  const local = url => {
    assert.equal(new URL(url).origin, state.origin);
    if (publicRoute) return url;
    return `http://127.0.0.1:${state.port}${new URL(url).pathname}${new URL(url).search}`;
  };
  const emit = event => { events.push(event.kind); for (const fn of listeners) fn(event); };
  const call = async (method, args) => {
    switch (method) {
      case 'configure': assert.equal(args.origin, state.origin); return true;
      case 'load': return structuredClone(disk);
      case 'save': disk = structuredClone(args.value); return true;
      case 'recordings': return voiceFile ? [{id:'voice-proof',seconds:10,codec:'wav16k',sampleRate:16000}] : [];
      case 'recordHash': assert.equal(args.id,'voice-proof'); return createHash('sha256').update(readFileSync(voiceFile)).digest('hex');
      case 'replyPlay': assert.equal(args.id,'reply-proof'); assert(replyAudio.length>44); played=true; return true;
      case 'updateInfo': return { version: release.version, build: release.build, osVersion: '16.7.16', appId: null, storefront: null, configured: false };
      case 'hash': return createHash('sha256').update(args.value).digest('hex');
      case 'publicKey': return publicKey.export({ format: 'jwk' });
      case 'sign': return sign('sha256', Buffer.from(args.input), { key: privateKey, dsaEncoding: 'ieee-p1363' }).toString('base64url');
      case 'request': {
        requests.push(args);
        const body = args.body?.recordingFile ? readFileSync(voiceFile) : args.body;
        const upload = voiceFile && args.method === 'POST' && new URL(args.url).searchParams.get('kind') === 'voice';
        if (upload) voiceClientId = new URL(args.url).searchParams.get('client_id');
        const response = await fetch(local(args.url), { method: args.method, headers: args.headers, body: args.method === 'GET' ? undefined : body });
        if (upload && response.status === 200 && !beforeReceipt) {
          const held = Date.now() + 30000;
          while (!voiceEchoes.length && Date.now() < held) await pause(25);
          const frame = voiceEchoes[0] ?? null;
          while (frame && !client.state().messages.some(row => row.id === frame.id) && Date.now() < held) await pause(25);
          beforeReceipt = { frame, shown: frame ? client.state().messages.find(row => row.id === frame.id) ?? null : null };
        }
        if (new URL(args.url).pathname.startsWith('/api/audio/') && response.status===200) {
          replyAudio=Buffer.from(await response.arrayBuffer());
          assert.equal(replyAudio.subarray(0,4).toString(),'RIFF');
          return {status:200,headers:{},body:'',audioId:'reply-proof'};
        }
        return { status: response.status, headers: Object.fromEntries(response.headers), body: await response.text() };
      }
      case 'streamClose': streams.get(args.id)?.abort(); streams.delete(args.id); return true;
      case 'streamOpen': {
        const controller = new AbortController(); streams.set(args.id, controller);
        void (async () => {
          try {
            const response = await fetch(local(args.url), { signal: controller.signal });
            assert.equal(response.status, 200);
            emit({ kind: 'stream-open', id: args.id });
            for await (const chunk of response.body) {
              if (voiceFile) readFrames(args.id, chunk);
              emit({ kind: 'stream-data', id: args.id, data: Buffer.from(chunk).toString('base64') });
            }
          } catch (error) { if (!controller.signal.aborted) { t.diagnostic(error.message); emit({ kind: 'stream-error', id: args.id, error: error.message }); } }
        })();
        return true;
      }
      default: throw Error(method);
    }
  };
  const ports = createPorts(call, fn => { listeners.add(fn); return () => listeners.delete(fn); });
  const wait = async predicate => {
    const deadline = Date.now() + (voiceFile ? 60000 : publicRoute ? 30000 : 10000);
    while (!predicate() && Date.now() < deadline) await pause(25);
    assert(predicate(), JSON.stringify({ state: client.state(), events }));
  };
  if (publicRoute) {
    let ready = false;
    const deadline = Date.now() + 30000;
    while (!ready && Date.now() < deadline) {
      try { ready = (await fetch(local(state.origin + '/api/challenge'),{signal:AbortSignal.timeout(5000)})).headers.has('x-richos-challenge'); } catch {}
      if (!ready) await pause(200);
    }
    assert(ready,'managed tunnel did not become ready');
  }
  assert.equal((await fetch(local(state.origin + "/api/messages"), { method: 'POST', body: '{}' })).status, 404);
  client = await createClient(ports);
  await client.dispatch({ type: 'pair', link: state.pairLink });
  assert.equal(client.state().words, state.words);
  await client.dispatch({ type: 'confirm-pair', matched: true });
  await wait(() => client.state().online);
  const message = 'Rust integration ' + crypto.randomUUID();
  await client.dispatch({ type: 'compose', text: message });
  await client.dispatch({ type: 'send' });
  const reply = () => client.state().messages.find(row => row.role === 'rich' && row.text === 'ack: ' + message && row.complete);
  // The streamed reply may beat the HTTP receipt. Require both positive facts.
  await wait(() => reply() && client.state().outbox.length === 0);
  assert.equal(client.state().outbox.length, 0);
  const sent = requests.find(row => row.url.endsWith('/api/messages') && row.headers.Authorization);
  assert(sent);
  const challengeResponse = await fetch(local(state.origin + "/api/challenge"));
  const challenge = challengeResponse.headers.get('x-richos-challenge');
  const input = `${challenge}\nPOST\n/api/messages\n${createHash('sha256').update(sent.body).digest('hex')}`;
  const wrong = await call('sign', { input: input + 'wrong' });
  const signed = await call('sign', { input });
  const headers = signature => ({ 'Content-Type': 'application/json', Authorization: `RichOS-Device ${disk.api.deviceId}.${challenge}.${signature}` });
  assert.equal((await fetch(local(sent.url), { method: 'POST', body: sent.body, headers: headers(wrong) })).status, 404);
  const duplicate = await fetch(local(sent.url), { method: 'POST', body: sent.body, headers: headers(signed) });
  assert.equal(duplicate.status, 200); assert.equal((await duplicate.json()).duplicate, true);
  if (attached.length) {
    // Its card is in the conversation, with the PDF's pages as the Mac's own reader counts them,
    // and its bytes come over on ONE signed GET of the production file route; unsigned, nothing.
    const cards = () => { try { return JSON.parse(readFileSync(join(state.data, 'timeline.json'), 'utf8')).items.filter(i => i.kind === 'rich_file'); } catch { return []; } };
    await wait(() => cards().length >= 2);
    const pdf = cards().find(i => i.file.name === 'Acme-counteroffer.pdf');
    assert(pdf && pdf.file.exists && pdf.file.meta.endsWith(' · 2 pages'), JSON.stringify(cards()));
    const path = `/api/messages?kind=file&file_id=${encodeURIComponent(pdf.id)}&thread_id=${encodeURIComponent(pdf.threadId)}`;
    const fileChallenge = (await fetch(local(state.origin + '/api/challenge'))).headers.get('x-richos-challenge');
    // A GET has no body: its hash is the empty string (RequestSigning.bodyHashHex).
    const fileSignature = await call('sign', { input: `${fileChallenge}\nGET\n${path}\n` });
    const got = await fetch(local(state.origin + path), { headers: { Authorization: `RichOS-Device ${disk.api.deviceId}.${fileChallenge}.${fileSignature}` } });
    assert.equal(got.status, 200);
    assert.deepEqual(Buffer.from(await got.arrayBuffer()), readFileSync(attached[0]), 'the bytes of the attached file');
    assert.equal((await fetch(local(state.origin + path))).status, 404, 'an unsigned ask gets nothing');
  }
  client.close();
  if (server) {
    await server.restart();
    let challenge;
    const recoveryDeadline = Date.now() + 30000;
    while (!challenge && Date.now() < recoveryDeadline) {
      try { challenge = (await fetch(local(state.origin + '/api/challenge'),{signal:AbortSignal.timeout(5000)})).headers.get('x-richos-challenge'); } catch {}
      if (!challenge) await pause(200);
    }
    assert(challenge,'Mac/tunnel did not recover within 30 seconds');
    const input = `${challenge}\nPOST\n/api/messages\n${createHash('sha256').update(sent.body).digest('hex')}`;
    const signature = await call('sign',{input});
    const result = await fetch(local(sent.url),{method:'POST',body:sent.body,headers:{'Content-Type':'application/json',Authorization:`RichOS-Device ${disk.api.deviceId}.${challenge}.${signature}`}});
    assert.equal(result.status,200); assert.equal((await result.json()).duplicate,true,'Mac restart lost the delivery receipt');
  }
  client = await createClient(ports);
  assert(reply(), 'reply survives client process state restoration');
  const previous = events.filter(kind => kind === 'stream-open').length;
  await wait(() => client.state().online && events.filter(kind => kind === 'stream-open').length > previous);
  const second = message + ' after restart';
  await client.dispatch({ type: 'compose', text: second }); await client.dispatch({ type: 'send' });
  await wait(() => client.state().messages.some(row => row.role === 'rich' && row.text === 'ack: ' + second && row.complete));
  if (voiceFile) {
    assert(client.state().canVoice,'The real Mac speech model must be available');
    await client.dispatch({type:'record-refresh'});
    await client.dispatch({type:'record-send',id:'voice-proof'});
    const spoken = () => client.state().messages.find(row=>row.role==='rich' && row.complete && row.text.toLowerCase().includes(voicePhrase.toLowerCase()));
    await wait(() => spoken() && client.state().outbox.length===0);
    assert.equal(client.state().messages.filter(row=>row.role==='ceo' && (row.kind==='voice' || row.text.toLowerCase().includes(voicePhrase.toLowerCase()))).length,1,'Transcription must replace the voice placeholder');
    assert(beforeReceipt?.frame, `The Mac's row for the recording did not reach the phone before its receipt was released: ${JSON.stringify(beforeReceipt)}`);
    assert.equal(beforeReceipt.frame.kind, 'voice', `The row that beat the receipt must be a voice row: ${JSON.stringify(beforeReceipt.frame)}`);
    assert.equal(beforeReceipt.shown?.kind, 'voice', `The phone must hold it as a voice row before the receipt: ${JSON.stringify(beforeReceipt.shown)}`);
    assert.equal(beforeReceipt.frame.from_microphone, false, 'from_microphone stays the desk voice mode');
    assert(voiceEchoes.every(row => row.kind === 'voice'), `Every row for the recording is a voice row: ${JSON.stringify(voiceEchoes)}`);
    t.diagnostic(`Before the receipt the phone had ${JSON.stringify({ id: beforeReceipt.frame.id, kind: beforeReceipt.frame.kind, client_id: beforeReceipt.frame.client_id, shown: beforeReceipt.shown.kind })}; rows for the recording on the stream: ${voiceEchoes.length}, kinds ${JSON.stringify(voiceEchoes.map(row => row.kind))}`);
    await client.dispatch({type:'reply-play',id:spoken().id});
    await wait(()=>played);
    t.diagnostic('Actual WAV passed signed upload, local Mac transcription, durable intake, gated Rich projection and WAV reply generation; phone microphone and speaker are separate physical checks');
  }
  client.close();
  if (remoteFile) {
    const timeline = JSON.parse(readFileSync(join(state.data, 'timeline.json'), 'utf8'));
    assert.equal(timeline.items.filter(row => row.kind === 'user_message' && row.text === message).length, 1);
    return;
  }
  await server.close();
  assert.deepEqual(await server.exited, [0, null]);
  const timeline = JSON.parse(readFileSync(join(cache, 'mac-timeline.json'), 'utf8'));
  assert.equal(timeline.items.filter(row => row.kind === 'user_message' && row.text === message).length, 1);
  assert(timeline.items.some(row => row.kind === 'rich_message' && row.text === 'ack: ' + second));
  if (!publicRoute) await assert.rejects(fetch(local(state.origin + "/api/challenge")));
  else {
    const stoppedResponse = await fetch(local(state.origin + '/api/challenge'));
    assert(!stoppedResponse.headers.has('x-richos-challenge'),'stopped helper still reaches a phone listener');
  }
});


test('manual Mac fixture retains pairing state after explicit shutdown and refuses to overwrite it', { timeout: 360000 }, async t => {
  if (process.env.RICHOS_MOBILE_REMOTE_STATE) { t.skip('Remote proof does not own the running Mac fixture'); return; }
  const cache = createScratch('manual-mac-proof');
  let server;
  t.after(async () => { await server?.close(); rmSync(cache, { recursive: true, force: true }); });
  server = await startMac(cache, { manual: true });
  assert.equal(server.state.data, join(cache, 'manual-data'));
  await server.close();
  assert.deepEqual(await server.exited, [0, null]);
  assert(existsSync(join(server.state.data, 'lab-owner')));
  assert(existsSync(server.state.ca));
  assert(existsSync(join(server.state.data, 'ledger.jsonl')));
  await assert.rejects(startMac(cache, { manual: true }), /existing test data is preserved/);
});
