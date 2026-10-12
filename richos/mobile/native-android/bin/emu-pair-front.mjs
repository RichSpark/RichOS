#!/usr/bin/env node
import { cacheDir } from '../../cli/machine-settings.mjs';
// Pair the emulator's app with an ISOLATED test Mac over real HTTPS, with the app unchanged.
//
// The app reaches the Mac by host name, on the platform's default trust store, with nothing
// pinned (platform/HttpsMac.kt). An isolated test Mac (`node richos/mobile/cli/mobile.mjs lab mac`,
// the production Rust phone listener with its own identity and data) serves plain HTTP on
// loopback and expects a TLS front (on a phone: Tailscale Serve). For an EMULATOR this script is
// that front, and it makes the emulator trust it the way a phone trusts a public certificate:
//
//   emu-pair-front.mjs serve --upstream <lab port> --dir <scratch dir> [--port <host port>]
//       A throwaway certificate authority and a `localhost` certificate in <dir> (mode 0700),
//       and an HTTPS server on 127.0.0.1 forwarding to http://127.0.0.1:<lab port>, streams
//       included. Prints one JSON line {ok, port, ca, caName}; runs until SIGTERM or SIGINT, then
//       closes and deletes the key material.
//
//   emu-pair-front.mjs trust --serial <emulator serial> --ca <ca.pem> [--reverse <device>:<host>]
//       ONLY an emulator (the serial must be emulator-NNNN, and a google_apis image that allows
//       `adb root`): adds the CA to the emulator's system store until it reboots (a tmpfs over
//       the Conscrypt APEX store, bound into zygote's and each app's mount namespace, as Android
//       14 reads its roots from the APEX). With --reverse, the device's localhost:<device> reaches
//       the host's 127.0.0.1:<host>, so the pairing origin is https://localhost:<device>.
//
//   emu-pair-front.mjs qr --text-file <file> --out <png> [--scale <px per module>]
//       The pairing link in <file> drawn as a QR PNG (ZXing's own encoder from the Gradle cache,
//       run by the JDK bin/randroid uses), for an injected scan or the emulator's virtual scene.
//       The link is read from a file so the single-use code never appears in a process list.
//
// The lab's origin must then be that address: RICHOS_MOBILE_MAC_ORIGIN=https://localhost:<device>.
// Nothing here touches a physical phone, the host's trust store or the user's Mac identity.
import { execFileSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import { dirname, join } from 'node:path';

function fail(message, code = 1) {
  console.log(JSON.stringify({ ok: false, error: message }));
  process.exit(code);
}

function args(list) {
  const out = {};
  for (let i = 0; i < list.length; i += 2) {
    if (!list[i].startsWith('--') || list[i + 1] === undefined) fail(`expected --name value pairs, got ${list[i]}`);
    out[list[i].slice(2)] = list[i + 1];
  }
  return out;
}

function openssl(...a) {
  return execFileSync('openssl', a, { stdio: ['ignore', 'pipe', 'pipe'] }).toString();
}

function certificates(dir) {
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  chmodSync(dir, 0o700);
  const ca = join(dir, 'ca.pem'), caKey = join(dir, 'ca.key'), key = join(dir, 'localhost.key'), csr = join(dir, 'localhost.csr'), cert = join(dir, 'localhost.pem'), ext = join(dir, 'localhost.ext');
  openssl('req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-keyout', caKey, '-out', ca, '-days', '2',
    '-subj', '/CN=RichConnect emulator test CA (throwaway)', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign');
  openssl('req', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-keyout', key, '-out', csr, '-subj', '/CN=localhost');
  writeFileSync(ext, 'basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n', { mode: 0o600 });
  openssl('x509', '-req', '-in', csr, '-CA', ca, '-CAkey', caKey, '-CAcreateserial', '-out', cert, '-days', '2', '-extfile', ext);
  for (const f of [caKey, key, csr, ext]) chmodSync(f, 0o600);
  const caName = openssl('x509', '-subject_hash_old', '-noout', '-in', ca).trim() + '.0';
  return { ca, caKey, key, cert, caName, secrets: [caKey, key, csr, ext, join(dir, 'ca.srl')] };
}

async function serve(o) {
  const upstream = Number(o.upstream);
  if (!Number.isInteger(upstream) || upstream <= 0) fail('serve needs --upstream <lab port>');
  if (!o.dir) fail('serve needs --dir <scratch dir>');
  const c = certificates(o.dir);
  const server = https.createServer({ key: readFileSync(c.key), cert: readFileSync(c.cert) }, (request, response) => {
    const forward = http.request({ host: '127.0.0.1', port: upstream, path: request.url, method: request.method, headers: request.headers }, result => {
      response.writeHead(result.statusCode, result.headers);
      response.flushHeaders();
      result.pipe(response);
    });
    forward.on('error', () => { if (!response.headersSent) response.writeHead(502); response.end(); });
    request.pipe(forward);
    response.on('close', () => forward.destroy());
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(Number(o.port || 0), '127.0.0.1', resolve); });
  console.log(JSON.stringify({ ok: true, port: server.address().port, ca: c.ca, caName: c.caName }));
  const stop = () => {
    server.closeAllConnections();
    server.close();
    for (const f of c.secrets) rmSync(f, { force: true });
    process.exit(0);
  };
  process.on('SIGTERM', stop);
  process.on('SIGINT', stop);
}

function trust(o) {
  const serial = o.serial || '';
  if (!/^emulator-\d+$/.test(serial)) fail('trust is for an emulator only: --serial emulator-NNNN');
  if (!o.ca) fail('trust needs --ca <ca.pem>');
  const adb = (...a) => execFileSync('adb', ['-s', serial, ...a], { stdio: ['ignore', 'pipe', 'pipe'] }).toString();
  const name = openssl('x509', '-subject_hash_old', '-noout', '-in', o.ca).trim() + '.0';
  adb('root');
  adb('wait-for-device');
  adb('push', o.ca, `/data/local/tmp/${name}`);
  const script = [
    'set -e',
    'S=/system/etc/security/cacerts; A=/apex/com.android.conscrypt/cacerts; T=/data/local/tmp/richconnect-ca',
    'rm -rf $T; mkdir -p -m 700 $T; cp $A/* $T/',
    'mount -t tmpfs tmpfs $S',
    `cp $T/* $S/; cp /data/local/tmp/${name} $S/${name}; rm -rf $T /data/local/tmp/${name}`,
    'chown root:root $S/*; chmod 644 $S/*; chcon u:object_r:system_file:s0 $S/*',
    'mount --bind $S $A',
    'for Z in $(pidof zygote) $(pidof zygote64); do nsenter --mount=/proc/$Z/ns/mnt -- /bin/mount --bind $S $A;',
    '  for P in $(ps -o PID= -P $Z); do nsenter --mount=/proc/$P/ns/mnt -- /bin/mount --bind $S $A || true; done; done',
    `ls $A/${name}`,
  ].join('\n');
  const listed = adb('shell', script).trim();
  let reverse = null;
  if (o.reverse) {
    const [device, host] = o.reverse.split(':').map(Number);
    if (!device || !host) fail('--reverse is <device port>:<host port>');
    adb('reverse', `tcp:${device}`, `tcp:${host}`);
    reverse = { device, host };
  }
  console.log(JSON.stringify({ ok: true, serial, installed: listed.endsWith(name), caName: name, reverse }));
}

function qr(o) {
  if (!o['text-file'] || !o.out) fail('qr needs --text-file <file> --out <png>');
  const gradleHome = process.env.GRADLE_USER_HOME || cacheDir('richos-native-android/gradle', 'GRADLE_USER_HOME');
  const found = execFileSync('find', [join(gradleHome, 'caches/modules-2/files-2.1/com.google.zxing/core'), '-name', 'core-3.5.4.jar'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim().split('\n')[0];
  if (!found) fail(`no ZXing core 3.5.4 in ${gradleHome}; build the app once first`);
  const javaHome = [process.env.RANDROID_JAVA_HOME, process.env.JAVA_HOME, '/Applications/Android Studio.app/Contents/jbr/Contents/Home'].find(h => h && existsSync(join(h, 'bin/java')));
  if (!javaHome) fail('no Java runtime (set RANDROID_JAVA_HOME)');
  const source = join(dirname(o.out), 'QrPng.java');
  writeFileSync(source, `
import com.google.zxing.*; import com.google.zxing.common.BitMatrix; import com.google.zxing.qrcode.QRCodeWriter;
import java.awt.image.BufferedImage; import java.nio.file.*; import java.util.Map;
public class QrPng { public static void main(String[] a) throws Exception {
  String text = Files.readString(Path.of(a[0])).strip(); int scale = Integer.parseInt(a[2]);
  BitMatrix m = new QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, Map.of(EncodeHintType.MARGIN, 4));
  BufferedImage img = new BufferedImage(m.getWidth() * scale, m.getHeight() * scale, BufferedImage.TYPE_INT_RGB);
  for (int y = 0; y < img.getHeight(); y++) for (int x = 0; x < img.getWidth(); x++) img.setRGB(x, y, m.get(x / scale, y / scale) ? 0x0C1322 : 0xFFFFFF);
  javax.imageio.ImageIO.write(img, "png", new java.io.File(a[1])); System.out.println(m.getWidth()); } }
`);
  const modules = execFileSync(join(javaHome, 'bin/java'), ['-cp', found, source, o['text-file'], o.out, String(Number(o.scale || 8))], { stdio: ['ignore', 'pipe', 'pipe'] }).toString().trim();
  rmSync(source, { force: true });
  console.log(JSON.stringify({ ok: true, out: o.out, modules: Number(modules) }));
}

const [command, ...rest] = process.argv.slice(2);
if (command === 'serve') await serve(args(rest));
else if (command === 'trust') trust(args(rest));
else if (command === 'qr') qr(args(rest));
else fail('usage: emu-pair-front.mjs serve --upstream <port> --dir <dir> [--port <p>] | trust --serial <emulator-N> --ca <pem> [--reverse <device>:<host>] | qr --text-file <file> --out <png>', 2);
