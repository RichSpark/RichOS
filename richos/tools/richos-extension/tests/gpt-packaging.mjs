import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Worker as Thread } from 'node:worker_threads';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
const zipPath = new URL('../modules/gpt-exporter/lib/jszip.min.js', import.meta.url);
const zipCode = await readFile(zipPath, 'utf8');
const zipContext = vm.createContext({ setTimeout, clearTimeout, setImmediate, clearImmediate, Uint8Array, Uint16Array, Uint32Array, ArrayBuffer, Blob });
vm.runInContext(zipCode, zipContext);
// Browser Worker facade runs the real packaging source in another thread.
globalThis.Worker = class {
  constructor(url) {
    this.thread = new Thread(`const { parentPort } = require('node:worker_threads');
      const vm = require('node:vm'), fs = require('node:fs');
      const context = vm.createContext({ setImmediate, clearImmediate, Blob, Uint8Array, Uint16Array, Uint32Array, ArrayBuffer, atob, setTimeout, clearTimeout });
      context.self = context;
      context.postMessage = data => parentPort.postMessage(data);
      context.importScripts = () => vm.runInContext(fs.readFileSync(${JSON.stringify(fileURLToPath(zipPath))}, 'utf8'), context);
      vm.runInContext(fs.readFileSync(${JSON.stringify(fileURLToPath(url))}, 'utf8'), context);
      parentPort.on('message', data => context.onmessage({ data }));`, { eval: true });
    this.thread.on('message', data => this.onmessage?.({ data }));
    this.thread.on('error', error => this.onerror?.(error));
  }
  postMessage(data) { this.thread.postMessage(data); }
  terminate() { return this.thread.terminate(); }
};
const { busy, handleExportFile } = await import('../modules/gpt-exporter/offscreen.js');
test('actual packaging worker compresses Unicode project paths without modifying content', { timeout: 10000 }, async () => {
  const files = [{ filename: '中文/Tëster.md', content: '# Text\nRepeated '.repeat(500) }, { filename: 'backup.json', content: '{"ok":true}' }];
  const result = handleExportFile({ module: 'gptExporter', action: 'create-zip-blob-url', files });
  assert.equal(busy(), true);
  const packed = await result;
  assert.equal(packed.success, true);
  const bytes = new Uint8Array(await (await fetch(packed.url)).arrayBuffer());
  const zip = await zipContext.JSZip.loadAsync(bytes);
  for (const file of files) assert.equal(await zip.file(file.filename).async('string'), file.content);
  assert.ok(bytes.length < files[0].content.length);
  assert.equal(handleExportFile({ module: 'callCapture', action: 'revoke-blob-url', url: packed.url }), undefined);
  await handleExportFile({ module: 'gptExporter', action: 'revoke-blob-url', url: packed.url });
  assert.equal(busy(), false);
});
test('plain text and base64 file assembly preserve exact bytes', { timeout: 10000 }, async () => {
  for (const input of [{ content: 'Hello 中文', mimeType: 'text/plain' }, { content: 'AAECAw==', isBase64: true, mimeType: 'application/octet-stream' }]) {
    const result = await handleExportFile({ module: 'gptExporter', action: 'create-blob-url', ...input });
    assert.equal(result.success, true);
    const bytes = Buffer.from(await (await fetch(result.url)).arrayBuffer());
    assert.deepEqual(bytes, input.isBase64 ? Buffer.from([0, 1, 2, 3]) : Buffer.from(input.content));
    await handleExportFile({ module: 'gptExporter', action: 'revoke-blob-url', url: result.url });
  }
  assert.equal(busy(), false);
});
