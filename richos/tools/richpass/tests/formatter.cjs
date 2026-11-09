'use strict';
// Synthetic raw bytes exercise formatter branches that are rare in full-cost KDF vectors.
const fs=require('fs'),path=require('path'),vm=require('vm'),assert=require('node:assert/strict'),crypto=require('crypto');
const root=path.resolve(__dirname,'..');
const html=fs.readFileSync(path.join(root,'RichPass.html'),'utf8');
let script=html.match(/<script id="richpass-derivation">([\s\S]*?)<\/script>/)[1];
// Test-only instrumentation of an isolated VM copy. The shipping HTML exposes no formatter override.
script=script.replace('return Object.freeze({normalizeDomain','globalThis.fixtureFormat = formatPassword; return Object.freeze({normalizeDomain');
const sandbox=()=>({window:{},btoa:binary=>Buffer.from(binary,'binary').toString('base64'),Uint8Array,TextEncoder});
const current=sandbox();vm.runInNewContext(script,current);
const options = {length:20, symbols:'@'};
const {formatter_vectors: vectors} = JSON.parse(html.match(/<script type="application\/json" id="public-test-vectors">([\s\S]*?)<\/script>/)[1]);
assert.deepEqual(vectors.map(v => v.missing).sort(), ['digit', 'only-at-symbol', 'uppercase']);
for (const vector of vectors) {
    const bytes = Buffer.from(vector.hash_hex, 'hex');
    assert.equal(bytes.length, 32);
    assert.equal(current.fixtureFormat(bytes, options), vector.richpass, vector.missing);
}
assert.equal(crypto.createHash('sha256').update('fictional RichPass formatter 3').digest('hex'),
    vectors.find(v => v.missing === 'digit').hash_hex);

const special = [
    ...vectors.map(v => Buffer.from(v.hash_hex, 'hex')),
    Buffer.from('69a69a69a01a69a69a69a69a69a69a69a69a69a69a69a69a69a69a69a69a69a0', 'hex'),
];
const samples = 10000;
const corpus = crypto.createHash('sha256');
for (let i = 0; i < samples; i++) {
    const bytes = special[i] ?? (i < special.length + 256
        ? Buffer.alloc(32, i - special.length)
        : crypto.createHash('sha256').update('fictional RichPass formatter ' + i).digest());
    const actual = current.fixtureFormat(bytes, options);
    assert.match(actual, /^[A-Za-z0-9@]{20}$/, 'length and alphabet for fixture ' + i);
    assert.equal(actual.split('@').length, 2, 'one configured symbol for fixture ' + i);
    assert.match(actual, /[A-Z]/, 'uppercase for fixture ' + i);
    assert.match(actual, /[0-9]/, 'digit for fixture ' + i);
    assert.doesNotMatch(actual, /[0-9]{3}/, 'digit runs for fixture ' + i);
    corpus.update(actual + '\n');
}
assert.equal(corpus.digest('hex'), '8f15f4e894e206cfe41015563e3847b78d0527fa2c1daeea5084cfcc29202124', 'frozen RichPass formatter outputs');
console.log('PASS ' + samples + ' frozen RichPass formatter fixtures and three public formatter vectors');
