#!/usr/bin/env node
'use strict';

// Does this Mac actually serve the probe over HTTPS at its own name, and does the trust step actually
// hand over something an iPhone will accept?
//
// The unit tests prove the certificate is built correctly. This proves the SERVER — that the two
// listeners come up, that the certificate they present validates AT THE NAME THE PHONE WILL USE, and
// that the trust step's three downloads are the real thing. Three independent verifiers are used on
// purpose, because a certificate that satisfies only its own author has been verified by nobody:
//
//   1. Node's own TLS stack, with the CA supplied — chain AND hostname, by the rules RFC 6125 states
//   2. `/usr/bin/openssl s_client` — a second implementation, and the one LibreSSL on this Mac uses
//   3. `security verify-cert` — APPLE'S trust evaluator, the same code path the iPhone takes. This is
//      the only one of the three whose opinion the phone shares.
//
// Every one of them is paired with a NEGATIVE CONTROL. A verifier that says yes to everything says
// nothing, and three of those would be worse than one honest check.
//
// NOTHING TOUCHES THE LOGIN KEYCHAIN. The CA is passed to each verifier as a file. `security
// verify-cert -r` takes an anchor on the command line, which is how Apple's evaluator can be asked
// the question without installing anything on this Mac.
//
// The server runs on scratch ports against a temp state directory, and both are removed however this
// process ends — garbage is always cleaned up (ceo-decisions.md §54).

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const tlsLib = require('node:tls');
const http = require('node:http');
const https = require('node:https');
const { spawn, execFileSync } = require('node:child_process');

const HTTPS_PORT = 9543;
const TRUST_PORT = 9542;
const localnames = require('../lib/localnames.js');
const HOST = localnames.primaryName();

let failures = 0;
function check(label, ok, detail) {
	process.stdout.write(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? `\n        ${detail}` : ''}\n`);
	if (!ok) failures++;
}
function note(line) { process.stdout.write(`        ${line}\n`); }

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'richos-tls-verify-'));
let server = null;
let cleaned = false;
const cleanup = () => {
	if (cleaned) return;
	cleaned = true;
	if (server) {
		try { server.kill('SIGTERM'); } catch { /* already gone */ }
		try { execFileSync('/bin/sleep', ['0.3']); } catch { /* fine */ }
		try { server.kill('SIGKILL'); } catch { /* already gone */ }
	}
	try { fs.rmSync(scratch, { recursive: true, force: true }); } catch { /* nothing else to do */ }
};
process.on('exit', cleanup);
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { cleanup(); process.exit(1); });

function get(lib, options) {
	return new Promise((resolve, reject) => {
		const req = lib.get(options, (res) => {
			const chunks = [];
			res.on('data', (c) => chunks.push(c));
			res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks) }));
		});
		req.on('error', reject);
		req.setTimeout(8000, () => { req.destroy(new Error('timed out')); });
	});
}

function connect(options) {
	return new Promise((resolve) => {
		const socket = tlsLib.connect(options, () => {
			resolve({ authorized: socket.authorized, error: socket.authorizationError, cert: socket.getPeerCertificate(), protocol: socket.getProtocol() });
			socket.end();
		});
		socket.on('error', (err) => resolve({ authorized: false, error: err.message, cert: null, protocol: null }));
		socket.setTimeout(8000, () => { socket.destroy(); resolve({ authorized: false, error: 'timed out', cert: null, protocol: null }); });
	});
}

function run(file, args, input) {
	try {
		return { ok: true, out: execFileSync(file, args, { encoding: 'utf8', input, stdio: ['pipe', 'pipe', 'pipe'] }) };
	} catch (err) {
		return { ok: false, out: `${err.stdout || ''}${err.stderr || ''}`, status: err.status };
	}
}

(async () => {
	// ---- the name has to resolve on this Mac before anything else can be tested ----
	const resolved = run('/usr/bin/dscacheutil', ['-q', 'host', '-a', 'name', HOST]);
	const address = (/ip_address:\s*(\S+)/.exec(resolved.out) || [])[1];
	check(`${HOST} resolves on this Mac with nothing of ours running`, Boolean(address), resolved.out.trim() || 'no answer');
	if (address) note(`${HOST} -> ${address}`);

	server = spawn(process.execPath, [path.join(__dirname, '..', 'server.js')], {
		env: Object.assign({}, process.env, {
			PROBE_STATE_DIR: scratch,
			PROBE_HTTPS_PORT: String(HTTPS_PORT),
			PROBE_TRUST_PORT: String(TRUST_PORT),
			PROBE_PRINT_QR: '0',
			PROBE_BUILD_SHA: 'tls-verify'
		}),
		stdio: ['ignore', 'pipe', 'pipe']
	});
	const serverLog = [];
	server.stdout.on('data', (d) => serverLog.push(d.toString()));
	server.stderr.on('data', (d) => serverLog.push(d.toString()));

	// Wait for the trust listener, which comes up alongside the HTTPS one.
	let up = false;
	for (let i = 0; i < 60 && !up; i++) {
		try {
			const r = await get(http, { host: '127.0.0.1', port: TRUST_PORT, path: '/healthz' });
			up = r.status === 200;
		} catch { await new Promise((r) => setTimeout(r, 250)); }
	}
	check('both listeners came up', up, up ? '' : serverLog.join(''));
	if (!up) { process.stdout.write(`\n=== 1 CHECK FAILED ===\n`); process.exit(1); }

	const caFile = path.join(scratch, 'tls', 'richos-local-ca.crt');
	const leafFile = path.join(scratch, 'tls', 'server.crt');
	const caPem = fs.readFileSync(caFile, 'utf8');

	process.stdout.write('\n=== 1. Node\'s TLS stack, at the name the phone will use ===\n');

	const trusted = await connect({ host: HOST, port: HTTPS_PORT, servername: HOST, ca: [caPem] });
	check(`the certificate validates for ${HOST} — chain and hostname both`,
		trusted.authorized === true, trusted.error || `${trusted.protocol}, subject ${trusted.cert && trusted.cert.subject && trusted.cert.subject.CN}`);
	if (trusted.cert) {
		note(`negotiated ${trusted.protocol}, SAN ${trusted.cert.subjectaltname}`);
	}

	// NEGATIVE CONTROL 1: without the CA, the same connection must be refused. If this passed, every
	// "it validates" above would mean only that the socket opened.
	const untrusted = await connect({ host: HOST, port: HTTPS_PORT, servername: HOST });
	check('NEGATIVE CONTROL: without the CA, the same certificate is REJECTED',
		untrusted.authorized === false, `authorized=${untrusted.authorized}, error=${untrusted.error}`);
	note(`rejected with: ${untrusted.error}`);

	// NEGATIVE CONTROL 2: the right chain at the wrong name must also be refused — that is the check
	// an iPhone does and the one a certificate with only a common name fails.
	const wrongName = await connect({ host: address || '127.0.0.1', port: HTTPS_PORT, servername: 'not-this-mac.local', ca: [caPem] });
	check('NEGATIVE CONTROL: the right certificate at the WRONG name is REJECTED',
		wrongName.authorized === false && /altname|Hostname|does not match/i.test(String(wrongName.error)),
		`error=${wrongName.error}`);

	process.stdout.write('\n=== 2. openssl s_client — a second implementation ===\n');

	// /usr/bin/openssl on purpose: it is LibreSSL and it is the one that is always there. A bare
	// `openssl` would resolve to whatever is first in PATH — on this Mac that is a Homebrew OpenSSL
	// 3.6.4, a different implementation with different flags.
	const sclient = run('/usr/bin/openssl',
		['s_client', '-connect', `${HOST}:${HTTPS_PORT}`, '-servername', HOST, '-CAfile', caFile], 'Q\n');
	const verifyLine = (/Verify return code: (\d+) \(([^)]*)\)/.exec(sclient.out) || []);
	check('openssl s_client verifies the chain, return code 0',
		verifyLine[1] === '0', verifyLine[0] || sclient.out.split('\n').slice(0, 4).join(' | '));
	const chainName = (/subject=.*CN\s*=\s*([^\n,]+)/.exec(sclient.out) || [])[1];
	if (chainName) note(`presented subject CN: ${chainName.trim()}`);

	// NEGATIVE CONTROL: the same command with no CA file must NOT return 0.
	const sclientNoCa = run('/usr/bin/openssl', ['s_client', '-connect', `${HOST}:${HTTPS_PORT}`, '-servername', HOST], 'Q\n');
	const noCaCode = (/Verify return code: (\d+)/.exec(sclientNoCa.out) || [])[1];
	check('NEGATIVE CONTROL: openssl without the CA does NOT return 0', noCaCode !== '0', `return code ${noCaCode}`);

	process.stdout.write('\n=== 3. Apple\'s own trust evaluator — the one the iPhone shares ===\n');

	const apple = run('/usr/bin/security', ['verify-cert', '-c', leafFile, '-r', caFile, '-p', 'ssl', '-s', HOST]);
	check(`security verify-cert accepts the chain for the ssl policy at ${HOST}`,
		/verification successful/i.test(apple.out), apple.out.split('\n')[0]);

	// `security` writes its verdict to stderr as well as stdout and leads with a `---` separator, so
	// the reason is pulled out by pattern rather than by line number.
	const reasonOf = (text) => (/(Cert Verify Result: .*|CSSMERR_[A-Z_]+|.*verification successful.*)/.exec(text) || ['no verdict line'])[0].trim();

	const appleWrong = run('/usr/bin/security', ['verify-cert', '-c', leafFile, '-r', caFile, '-p', 'ssl', '-s', 'not-this-mac.local']);
	check('NEGATIVE CONTROL: Apple\'s evaluator rejects the wrong host name',
		/mismatch/i.test(appleWrong.out) && !/verification successful/i.test(appleWrong.out),
		reasonOf(appleWrong.out));

	const appleNoAnchor = run('/usr/bin/security', ['verify-cert', '-c', leafFile, '-p', 'ssl', '-s', HOST]);
	check('NEGATIVE CONTROL: without the root as an anchor, Apple\'s evaluator rejects it',
		!/verification successful/i.test(appleNoAnchor.out), reasonOf(appleNoAnchor.out));
	note('The last two are what make the first mean something: this root is trusted BECAUSE it was');
	note('handed over as an anchor, never because anything on this Mac already trusted it.');

	process.stdout.write('\n=== 4. The trust step, over plain HTTP ===\n');

	const page = await get(http, { host: HOST, port: TRUST_PORT, path: '/' });
	check('the trust page is served', page.status === 200 && /Check 0/.test(page.body.toString()));
	const html = page.body.toString();
	check('no placeholder survived into the page', !/\{\{[A-Z_]+\}\}/.test(html),
		(/\{\{[A-Z_]+\}\}/.exec(html) || [''])[0]);
	// The label he has to find in Settings must be the CA's real common name, or the instructions send
	// him looking for something that is not on his screen.
	const crypto = require('node:crypto');
	const caName = (/CN=([^\n]+)/.exec(new crypto.X509Certificate(caPem).subject) || [])[1].trim();
	check('the page names the exact label iOS will show', html.includes(caName), `certificate CN is ${JSON.stringify(caName)}`);
	check('the page names the certificate\'s scope, so what he trusts is on screen',
		html.includes('mm1.local') || html.includes(HOST.split('.')[0]));
	check('the page tells him about the red "Not Signed" BEFORE he meets it', /Not Signed/.test(html));
	check('the page tells him what to do if the trust switch is missing entirely', /not there at all/i.test(html));
	check('the page carries no JavaScript at all', !/<script/i.test(html),
		'it has to work before the phone trusts anything, so there is nothing to fail');

	const profile = await get(http, { host: HOST, port: TRUST_PORT, path: '/ca.mobileconfig' });
	check('the profile downloads with the content type that makes iOS install it',
		profile.status === 200 && profile.headers['content-type'] === 'application/x-apple-aspen-config',
		String(profile.headers['content-type']));
	check('the profile is a root-certificate payload and carries the real certificate',
		/com\.apple\.security\.root/.test(profile.body.toString()));
	const embedded = Buffer.from((/<data>([\s\S]*?)<\/data>/.exec(profile.body.toString()) || [])[1].replace(/\s+/g, ''), 'base64');
	check('the certificate inside the profile is byte-for-byte the one the server presents as its root',
		embedded.equals(Buffer.from(new crypto.X509Certificate(caPem).raw)));

	const bare = await get(http, { host: HOST, port: TRUST_PORT, path: '/ca.crt' });
	check('the bare certificate is offered too, for anything that will not take a profile',
		bare.status === 200 && bare.headers['content-type'] === 'application/x-x509-ca-cert');

	// ---- the QR on that page must point at the HTTPS origin, decoded rather than assumed ----
	const qrPng = await get(http, { host: HOST, port: TRUST_PORT, path: '/qr.png' });
	check('the QR image is served as a PNG', qrPng.status === 200 && qrPng.body[1] === 0x50);
	const qrFile = path.join(scratch, 'served-qr.png');
	fs.writeFileSync(qrFile, qrPng.body);
	const swift = run('/usr/bin/xcrun', ['--find', 'swiftc']);
	if (swift.ok) {
		const binary = path.join(scratch, 'qr-decode');
		const compiled = run('swiftc', ['-O', '-o', binary, path.join(__dirname, 'qr-decode.swift')]);
		if (compiled.ok) {
			const decoded = run(binary, [qrFile]);
			const expected = `https://${HOST}:${HTTPS_PORT}/`;
			check('the QR the page serves decodes to the probe\'s own HTTPS origin',
				decoded.ok && decoded.out.trim() === expected,
				decoded.ok ? `${decoded.out.trim()} (expected ${expected})` : decoded.out);
		} else {
			note('swiftc would not compile the decoder, so the served QR was not decoded. Not hidden.');
		}
	} else {
		note('swiftc is absent, so the served QR was not decoded. Not hidden.');
	}

	check('the trust listener serves nothing else', (await get(http, { host: HOST, port: TRUST_PORT, path: '/index.js' })).status === 404);

	process.stdout.write('\n=== 5. The probe itself, over the trusted origin ===\n');

	const agent = new https.Agent({ ca: [caPem] });
	const config = await get(https, { host: HOST, port: HTTPS_PORT, path: '/api/config', agent, servername: HOST });
	const parsed = JSON.parse(config.body.toString());
	check('the probe answers over HTTPS and reports the build that served it',
		config.status === 200 && parsed.buildSha === 'tls-verify', parsed.buildSha);
	check('the server reports the connection as secure from the SOCKET, not from a header',
		parsed.tls && parsed.tls.secure === true, JSON.stringify(parsed.tls));
	check('the page is told which names the certificate covers, so check 0 shows evidence',
		parsed.tls.names.includes(HOST), parsed.tls.names);
	check('the page is told where the trust step lives', parsed.trustUrl === `http://${HOST}:${TRUST_PORT}`, parsed.trustUrl);
	// The SUBJECT LINE only. The log also explains, in prose, that RFC 8292 allows a mailto: — so a
	// naive search of the whole log for "mailto:" fails on the sentence saying why one is not used.
	const subjectLine = (/VAPID subject:\s*(\S+)/.exec(serverLog.join('')) || [])[1] || '';
	check('the VAPID subject is the public project page, never a personal address',
		subjectLine === 'https://github.com/RichSpark/RichOS', `subject is ${JSON.stringify(subjectLine)}`);
	check('nothing in the boot output looks like an email address',
		!/[\w.+-]+@[\w-]+\.[\w.]+/.test(serverLog.join('')),
		(/[\w.+-]+@[\w-]+\.[\w.]+/.exec(serverLog.join('')) || ['none'])[0]);

	const pageOverHttps = await get(https, { host: HOST, port: HTTPS_PORT, path: '/', agent, servername: HOST });
	check('the probe page itself loads over the trusted origin', pageOverHttps.status === 200 && /step-0/.test(pageOverHttps.body.toString()));
	check('check 0 is in the markup', /the hosting step/.test(pageOverHttps.body.toString()));

	process.stdout.write(`\n=== ${failures === 0 ? 'ALL CHECKS PASSED' : `${failures} CHECK(S) FAILED`} ===\n`);
	if (failures) process.stdout.write(`\nserver log:\n${serverLog.join('')}\n`);
	cleanup();
	process.exit(failures ? 1 : 0);
})().catch((err) => {
	check('the harness ran to completion', false, String(err && err.stack || err));
	cleanup();
	process.exit(1);
});
