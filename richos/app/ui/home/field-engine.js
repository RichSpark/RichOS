// ============================================================================================
// PORTED, NOT AUTHORED. This file is `richos-hq/design/mockups/rounds/round-11.1/shared/engine.js`
// (sha256 02ca5b6c…) carried into the app. Design rounds are FROZEN and append-only, so the
// original was never touched; the previous commit vendored it byte for byte and this commit's
// diff is the complete list of what the port changed. Every change is marked `PORT:` and there
// are five of them, no more:
//
//   1. Every DOM id it reaches for now carries a `home-` prefix, and lookups start from the
//      `#home` root rather than from the document. Ids are document-global, and the app shell
//      already owns `#stage`.
//   2. A pause()/resume() seam, so the loop stops when the CEO is in the app UI and comes back
//      without rebuilding anything. Exported on `window.__loro`.
//   3. `running` guards on the three WINDOW-level listeners (mouseup, keydown, resize) — the
//      mockup was the only thing on the page and could assume every event was its own.
//   4. `getComputedStyle(ROOT)` instead of `document.body` for `--serif`/`--sans`: the home
//      screen's type tokens are scoped to `#home` so they cannot leak into the app's own.
//   5. Nothing else. No tuning, no palette, no geometry, no timing.
//   6. A `.catch` on the IIFE, added 2026-09-04 — see the foot of this file. The engine
//      throws on a machine whose WebGL will not serve it, and a throw inside an async IIFE
//      is a rejected promise nobody is holding. It stays a throw; it now also lands
//      somewhere `home.js` can see it. No behavior in the picture changes.
//
// WHAT IS NOT PORTED, said plainly rather than left to be discovered: the dataset under this is
// the round's SYNTHETIC one, not the CEO's loro. See the header of field-data.js.
// ============================================================================================

// RichOS home screen — round 6.4: the loro as the stylised brain.
//
// Basis: round-3/v1 (the top-left block, the workforce list, the hush under both, hover spotlight, drag,
// click, wheel, learning sparks, share-safe labels) — with the bottom text and buttons and everything they
// drove (ask, chips, answers, the replay) removed. The force worker is gone too: it was the source of the
// micro-jitter. Motion here is deterministic: a strand-following bloom from the core outward, then
// critically damped springs that snap to exact rest. Nothing moves unless something moved it.
//
// Picture: round-5.2/v4's traced brain. Every loro object sits on one of the picture's strands; every link
// is a curve that follows the flow, rendered through a glow pass. The river is the 4,800 sources.
//
// prepare/TYPE_WORD/mulberry32/clamp come from ../shared/prep.js; window.VARIANT tunes a version.
(async () => {
'use strict';

const V = Object.assign({
  name: 'v1',
  glDpr: 1,            // fibre canvas resolution multiplier
  glow: 1.0,           // glow layer gain
  glowSigma: 2.2,      // blur sigma in quarter-res pixels
  fibreGain: 1.0,      // sharp fibre gain
  riverGain: 1.0,
  nodeGain: 1.0,
  packetShare: 0.08,   // fraction of links carrying a light packet
  riverPackets: 240,
  packetSpeed: 1.0,
  parallax: 0,         // px of parallax at full tilt
  idleDrift: 0,        // px of slow idle drift of the tilt (needs parallax)
  ribbons: false,      // relationship links as tapered ribbons
  ribbonWidth: 1.0,
  hairGain: 1.0,       // membership fibres gain (ribbons mode dims them)
  constellation: false,// hover lights the cluster, click blooms it
  nodeScale: 1,
  leafWhite: 0.5,      // how white the small motes are
  bloom: 1,            // bloom duration multiplier
  twinkle: 0.12,       // slow amplitude of mote twinkle
  breathe: 1,          // light breathing amplitude
  proxAmp: 8,          // px of pointer pull
  zGain: 0,            // depth brightness: fibres and motes in front brighter (parallax versions)
  packetGain: 1,
  clickZoom: 1.7,
  waveAmp: 9,
  coreRays: true,
  corePulseOnLand: true,
  bgClass: '',
}, window.VARIANT || {});

// PORT: scoped to the home screen's own root. In the mockup this page was the only thing
// on screen; in the app the shell below it owns ids of its own (`#stage` is both a mockup
// canvas holder and the app's conversation column), so every id here carries a `home-`
// prefix and every lookup starts from `#home` rather than from the document.
const ROOT = document.getElementById('home');
// A MISSING ROOT IS NOT AN ERROR, it is an absent surface, and the difference matters here.
// These four files are injected asynchronously; a page that navigates away, or a test fixture
// that replaces the document, can execute this one against a document that no longer has a
// home screen in it. Throwing made that an UNHANDLED REJECTION inside an async IIFE, which
// nothing was holding — it surfaced as a console error in two acceptance suites that had
// never heard of this file. There is simply nothing to draw, so nothing is drawn.
if (!ROOT) return;
const $ = (s) => ROOT.querySelector(s);
const glc = $('#home-gl'), ov = $('#home-overlay'), ctx = ov.getContext('2d'), bk = $('#home-bokeh');
const gl = glc.getContext('webgl', { antialias: true, alpha: true, premultipliedAlpha: true, preserveDrawingBuffer: false });
if (!gl) throw new Error('WebGL unavailable');
gl.getExtension('OES_element_index_uint');

let W = innerWidth, H = innerHeight, DPR = Math.min(2, devicePixelRatio || 1), GD = V.glDpr;
let fboDirty = true;
function resize() {
  W = innerWidth; H = innerHeight; DPR = Math.min(2, devicePixelRatio || 1);
  glc.width = Math.round(W * GD); glc.height = Math.round(H * GD); ov.width = W * DPR; ov.height = H * DPR;
  bk.width = W; bk.height = H;
  fboDirty = true; quietDirty = true; homeCam();
  drawBokeh();
}

// ---------------- data ----------------
const loro = window.MATURE_LORO;
const D = prepare(loro, window.REF, V);
const { N, L, S, nodes, domains, CORE } = D;
const rnd = mulberry32(6401);

// ---------------- camera ----------------
const cam = { x: CORE.x, y: 500, s: 0.9 }, camT = { x: CORE.x, y: 500, s: 0.9 };
let home = { x: 500, y: 500, s: 0.9 };
function homeCam() { home = { x: 500, y: 500, s: Math.max(W / 1600, H / 1000) }; }
homeCam(); Object.assign(cam, home); Object.assign(camT, home);
const tilt = { x: 0, y: 0 }, tiltT = { x: 0, y: 0 };
function toScreen(wx, wy, z) { return [(wx - cam.x) * cam.s + W / 2 + tilt.x * (z || 0), (wy - cam.y) * cam.s + H / 2 + tilt.y * (z || 0)]; }
function toWorld(sx, sy) { return [(sx - W / 2) / cam.s + cam.x, (sy - H / 2) / cam.s + cam.y]; }
function nodeScreen(i) { return toScreen(pos[i * 2], pos[i * 2 + 1], D.z[i]); }

// ---------------- state ----------------
const pos = new Float32Array(N * 2); pos.set(D.home);
const off = new Float32Array(N * 2), vel = new Float32Array(N * 2), tgt = new Float32Array(N * 2);
const active = new Uint8Array(N); let activeList = [];
const nAlpha = new Float32Array(N);               // bloom fade-in
const fade = new Float32Array(N).fill(1), lit = new Float32Array(N), glow = new Float32Array(N), flash = new Float32Array(N);
const isNbr = new Uint8Array(N), inDom = new Uint8Array(N);
const linkFlash = new Float32Array(L), linkLit = new Float32Array(L);
const twPhase = new Float32Array(N); for (let i = 0; i < N; i++) twPhase[i] = rnd() * Math.PI * 2;
const kind = new Float32Array(N);
for (let i = 0; i < N; i++) { const n = nodes[i]; kind[i] = D.radius[i] < 2.2 ? 0 : n.type === 'decision' ? 1 : (n.type === 'commitment' && n.status !== 'met') ? 2 : 0; }
// rest colours: motes are whitish, hubs carry their strand's colour
const restCol = new Float32Array(N * 3), vividCol = new Float32Array(N * 3);
for (let i = 0; i < N; i++) {
  const w = V.leafWhite * (1 - Math.min(1, D.degree[i] / 8)) * (D.isLight[i] ? 0 : 1);
  for (let c = 0; c < 3; c++) { const v = D.col[i * 3 + c]; restCol[i * 3 + c] = v + (1 - v) * w; vividCol[i * 3 + c] = Math.min(1, v * 0.6 + 0.4); }
}
const domCss = domains.map((d, di) => `rgb(${(D.domCol[di * 3] * 255) | 0},${(D.domCol[di * 3 + 1] * 255) | 0},${(D.domCol[di * 3 + 2] * 255) | 0})`);

const T0 = performance.now();
let bloomEnd = 0; for (let i = 0; i < N; i++) bloomEnd = Math.max(bloomEnd, (D.delay[i] + D.dur[i]) * V.bloom);
let blooming = true, geomDirty = true;

// ---------------- WebGL ----------------
function shader(type, src) { const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) + '\n' + src); return s; }
function program(vs, fs) { const p = gl.createProgram(); gl.attachShader(p, shader(gl.VERTEX_SHADER, vs)); gl.attachShader(p, shader(gl.FRAGMENT_SHADER, fs)); gl.linkProgram(p); if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p)); return p; }
const QUIET = `
  uniform vec4 u_quiet[8]; uniform float u_qf; uniform vec4 u_qbox[2];
  float quietK(vec2 scr) {
    bool out0 = scr.x < u_qbox[0].x || scr.y < u_qbox[0].y || scr.x > u_qbox[0].z || scr.y > u_qbox[0].w;
    bool out1 = scr.x < u_qbox[1].x || scr.y < u_qbox[1].y || scr.x > u_qbox[1].z || scr.y > u_qbox[1].w;
    if (out0 && out1) return 1.0;
    float k = 1.0;
    for (int r = 0; r < 8; r++) {
      vec2 d = max(max(u_quiet[r].xy - scr, scr - u_quiet[r].zw), vec2(0.0));
      k = min(k, smoothstep(0.0, u_qf, length(d)));
    }
    return k;
  }`;
const CAMV = `
  uniform vec3 u_cam; uniform vec2 u_view; uniform vec2 u_cursor; uniform vec2 u_prox; uniform vec2 u_tilt;
  vec2 toScr(vec2 p, float z) {
    vec2 scr = (p - u_cam.xy) * u_cam.z + u_view * 0.5 + u_tilt * z;
    vec2 d = u_cursor - scr; float l = length(d);
    float f = smoothstep(u_prox.x, 0.0, l); f = f * f * (3.0 - 2.0 * f);
    return scr + (l > 0.001 ? d / l : vec2(0.0)) * f * u_prox.y;
  }
  vec4 toClip(vec2 scr) { vec2 c = scr / u_view * 2.0 - 1.0; c.y = -c.y; return vec4(c, 0.0, 1.0); }`;

const fibreProg = program(`
  attribute vec2 a_pos; attribute float a_z; attribute vec4 a_col; varying vec4 v_col; varying vec2 v_scr; ${CAMV} uniform float u_zgain;
  void main(){ vec2 scr = toScr(a_pos, a_z); gl_Position = toClip(scr); v_col = a_col * (1.0 + u_zgain * a_z); v_scr = scr; }`,
  `precision mediump float; varying vec4 v_col; varying vec2 v_scr; uniform float u_gain; ${QUIET}
  void main(){ gl_FragColor = v_col * (u_gain * quietK(v_scr)); }`);
const ribbonProg = program(`
  attribute vec2 a_pos; attribute vec2 a_nrm; attribute float a_hw; attribute float a_u; attribute float a_z; attribute vec4 a_col;
  varying vec4 v_col; varying vec2 v_scr; varying float v_edge; varying float v_u; ${CAMV} uniform float u_dpr;
  void main(){ vec2 scr = toScr(a_pos, a_z) + a_nrm * a_hw; gl_Position = toClip(scr); v_col = a_col; v_scr = scr; v_edge = sign(a_hw); v_u = a_u; }`,
  `precision mediump float; varying vec4 v_col; varying vec2 v_scr; varying float v_edge; varying float v_u; uniform float u_gain; uniform float u_time; ${QUIET}
  void main(){ float e = 1.0 - v_edge * v_edge; float sheen = 0.72 + 0.28 * sin(v_u * 7.0 - u_time * 0.45 + v_col.a * 20.0); gl_FragColor = v_col * (e * e * sheen * u_gain * quietK(v_scr)); }`);
const nodeProg = program(`
  attribute vec2 a_pos; attribute float a_size; attribute vec4 a_col; attribute float a_glow; attribute float a_kind; attribute float a_light; attribute float a_phase; attribute float a_z;
  varying vec4 v_col; varying float v_r; varying float v_glow; varying float v_kind; varying float v_ps; varying vec2 v_scr; varying float v_lr;
  ${CAMV} uniform float u_dpr; uniform float u_homeS; uniform float u_time; uniform float u_breathe; uniform float u_zgain;
  void main(){
    vec2 scr = toScr(a_pos, a_z); gl_Position = toClip(scr);
    float sc = sqrt(u_cam.z / u_homeS) * u_dpr;
    float r = a_size * sc;
    float lr = a_light * (1.0 + u_breathe * 0.13 * sin(u_time * (0.9 + fract(a_phase * 0.37) * 0.7) + a_phase)) * sc;
    float ps = max(r * 2.0 + 3.0 * u_dpr + a_glow * (14.0 * u_dpr + r * 2.0), lr * 2.6);
    gl_PointSize = ps; v_ps = ps; v_r = r; v_col = a_col * (1.0 + u_zgain * a_z); v_glow = a_glow; v_kind = a_kind; v_scr = scr; v_lr = lr;
  }`, `precision highp float;
  varying vec4 v_col; varying float v_r; varying float v_glow; varying float v_kind; varying float v_ps; varying vec2 v_scr; varying float v_lr;
  uniform float u_dpr; ${QUIET}
  void main(){
    vec2 p = (gl_PointCoord - 0.5) * v_ps; float d = length(p);
    float q = mix(0.04, 1.0, quietK(v_scr + p / u_dpr));
    float aa = 0.9;
    float disc = 1.0 - smoothstep(v_r - aa, v_r + aa, d);
    if (v_kind > 0.5 && v_kind < 1.5) { float core = 1.0 - smoothstep(v_r * 0.52 - aa, v_r * 0.52 + aa, d); float ring = smoothstep(v_r * 0.76 - aa, v_r * 0.76 + aa, d) * (1.0 - smoothstep(v_r - aa, v_r + aa, d)); disc = max(core, ring); }
    else if (v_kind > 1.5) { float inner = smoothstep(v_r * 0.5 - aa, v_r * 0.5 + aa, d); disc = disc * max(inner, 0.35); }
    float halo = v_glow * pow(max(0.0, 1.0 - d / (v_ps * 0.5)), 2.2) * 0.55;
    vec3 rgb = v_col.rgb * disc + v_col.rgb * halo;
    float a = v_col.a * disc;
    if (v_lr > 0.0) {
      float g = exp(-(d * d) / (v_lr * v_lr * 0.55));
      float core = 1.0 - smoothstep(v_lr * 0.16, v_lr * 0.30, d);
      vec3 lc = mix(v_col.rgb, vec3(1.0), core * 0.95);
      rgb += lc * (g * 0.92 + core * 0.4) * v_col.a;
    }
    gl_FragColor = vec4(rgb * q, a * q);
  }`);
// the core: a soft star drawn in GL so the hush feathers it per fragment like everything else
const coreProg = program(`attribute vec2 a_uv; uniform vec2 u_c; uniform float u_r; uniform vec2 u_view; varying vec2 v_p; varying vec2 v_scr;
  void main(){ vec2 scr = u_c + a_uv * u_r; v_p = a_uv; v_scr = scr; vec2 c = scr / u_view * 2.0 - 1.0; c.y = -c.y; gl_Position = vec4(c, 0.0, 1.0); }`,
  `precision mediump float; varying vec2 v_p; varying vec2 v_scr; uniform float u_gain; uniform float u_core; ${QUIET}
  void main(){ float d = length(v_p); float g = exp(-d * d * 3.4); float core = 1.0 - smoothstep(u_core * 0.75, u_core * 1.15, d);
    vec3 col = mix(vec3(0.42, 0.86, 0.6), vec3(1.0), g * g * 0.7 + core); float a = (g * 1.05 + core) * u_gain * quietK(v_scr);
    gl_FragColor = vec4(col * a, a); }`);
const quadProg = program(`attribute vec2 a_uv; varying vec2 v_uv; void main(){ v_uv = a_uv; gl_Position = vec4(a_uv * 2.0 - 1.0, 0.0, 1.0); }`,
  `precision mediump float; varying vec2 v_uv; uniform sampler2D u_tex; uniform vec2 u_dir; uniform float u_w[7];
  void main(){ vec4 c = texture2D(u_tex, v_uv) * u_w[0];
    for (int i = 1; i < 7; i++) { c += (texture2D(u_tex, v_uv + u_dir * float(i)) + texture2D(u_tex, v_uv - u_dir * float(i))) * u_w[i]; }
    gl_FragColor = c; }`);
const compProg = program(`attribute vec2 a_uv; varying vec2 v_uv; varying vec2 v_scr; uniform vec2 u_view; void main(){ v_uv = a_uv; v_scr = vec2(a_uv.x, 1.0 - a_uv.y) * u_view; gl_Position = vec4(a_uv * 2.0 - 1.0, 0.0, 1.0); }`,
  `precision mediump float; varying vec2 v_uv; varying vec2 v_scr; uniform sampler2D u_tex; uniform float u_gain; ${QUIET}
  void main(){ vec4 c = texture2D(u_tex, v_uv); gl_FragColor = c * (u_gain * quietK(v_scr)); }`);

const buf = (data, usage, target) => { const b = gl.createBuffer(); gl.bindBuffer(target || gl.ARRAY_BUFFER, b); gl.bufferData(target || gl.ARRAY_BUFFER, data, usage); return b; };
const loc = (p, n) => gl.getAttribLocation(p, n), uni = (p, n) => gl.getUniformLocation(p, n);
function attr(b, l, size, type, norm, stride, offset) { gl.bindBuffer(gl.ARRAY_BUFFER, b); gl.enableVertexAttribArray(l); gl.vertexAttribPointer(l, size, type || gl.FLOAT, !!norm, stride || 0, offset || 0); }
const U = (p) => ({ cam: uni(p, 'u_cam'), view: uni(p, 'u_view'), cursor: uni(p, 'u_cursor'), prox: uni(p, 'u_prox'), tilt: uni(p, 'u_tilt'), quiet: uni(p, 'u_quiet'), qf: uni(p, 'u_qf'), qbox: uni(p, 'u_qbox'), gain: uni(p, 'u_gain'), dpr: uni(p, 'u_dpr'), homeS: uni(p, 'u_homeS'), time: uni(p, 'u_time'), breathe: uni(p, 'u_breathe'), tex: uni(p, 'u_tex'), dir: uni(p, 'u_dir'), w: uni(p, 'u_w'), zgain: uni(p, 'u_zgain') });
const FU = U(fibreProg), RU = U(ribbonProg), NU = U(nodeProg), QU = U(quadProg), CU = U(compProg);
const FA = { pos: loc(fibreProg, 'a_pos'), z: loc(fibreProg, 'a_z'), col: loc(fibreProg, 'a_col') };
const RA = { pos: loc(ribbonProg, 'a_pos'), nrm: loc(ribbonProg, 'a_nrm'), hw: loc(ribbonProg, 'a_hw'), u: loc(ribbonProg, 'a_u'), z: loc(ribbonProg, 'a_z'), col: loc(ribbonProg, 'a_col') };
const NA = { pos: loc(nodeProg, 'a_pos'), size: loc(nodeProg, 'a_size'), col: loc(nodeProg, 'a_col'), glow: loc(nodeProg, 'a_glow'), kind: loc(nodeProg, 'a_kind'), light: loc(nodeProg, 'a_light'), phase: loc(nodeProg, 'a_phase'), z: loc(nodeProg, 'a_z') };
const QA = loc(quadProg, 'a_uv'), CA = loc(compProg, 'a_uv');
const CQ = { uv: loc(coreProg, 'a_uv'), c: uni(coreProg, 'u_c'), r: uni(coreProg, 'u_r'), view: uni(coreProg, 'u_view'), gain: uni(coreProg, 'u_gain'), core: uni(coreProg, 'u_core'), quiet: uni(coreProg, 'u_quiet'), qf: uni(coreProg, 'u_qf'), qbox: uni(coreProg, 'u_qbox') };
const bQuad = buf(new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
const bQuadC = buf(new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
// the core's rays: 14 lines in world units, turning once every 40 s, drawn with the fibre program
const rayAngles = []; for (let i = 0; i < 14; i++) rayAngles.push([rnd() * Math.PI * 2, 40 + rnd() * 90, 0.35 + rnd() * 0.3]);
const rayPos = new Float32Array(28 * 2), rayZ = new Float32Array(28), rayCol = new Uint8Array(28 * 4);
const bRayPos = buf(rayPos, gl.DYNAMIC_DRAW), bRayZ = buf(rayZ, gl.STATIC_DRAW), bRayCol = buf(rayCol, gl.DYNAMIC_DRAW);
let corePulse = 0;
function drawCoreGL(t) {
  const bloomK = clamp(t / 1.2, 0, 1);
  const breathe = 1 + 0.035 * Math.sin(t * 1.5) + corePulse * 0.35;
  corePulse *= 0.95; if (corePulse < 0.01) corePulse = 0;
  const k = cam.s / home.s;
  // rays
  const rot = t * Math.PI * 2 / 40;
  for (let i = 0; i < 14; i++) {
    const [a0, l, w] = rayAngles[i]; const a = a0 + rot, ca = Math.cos(a), sa = Math.sin(a);
    rayPos[i * 4] = CORE.x + ca * 4; rayPos[i * 4 + 1] = CORE.y + sa * 4; rayPos[i * 4 + 2] = CORE.x + ca * l * bloomK; rayPos[i * 4 + 3] = CORE.y + sa * l * bloomK;
    const aa = w * bloomK * 255, o = i * 8;
    rayCol[o] = 230 / 255 * aa; rayCol[o + 1] = aa; rayCol[o + 2] = 235 / 255 * aa; rayCol[o + 3] = aa;
    rayCol[o + 4] = 0; rayCol[o + 5] = 0; rayCol[o + 6] = 0; rayCol[o + 7] = 0; // fades to nothing at the tip
  }
  gl.blendFunc(gl.ONE, gl.ONE);
  gl.useProgram(fibreProg); setCommonU(FU); gl.uniform1f(FU.gain, 1.0);
  gl.bindBuffer(gl.ARRAY_BUFFER, bRayPos); gl.bufferSubData(gl.ARRAY_BUFFER, 0, rayPos); gl.enableVertexAttribArray(FA.pos); gl.vertexAttribPointer(FA.pos, 2, gl.FLOAT, false, 0, 0);
  attr(bRayZ, FA.z, 1);
  gl.bindBuffer(gl.ARRAY_BUFFER, bRayCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, rayCol); gl.enableVertexAttribArray(FA.col); gl.vertexAttribPointer(FA.col, 4, gl.UNSIGNED_BYTE, true, 0, 0);
  if (V.coreRays) gl.drawArrays(gl.LINES, 0, 28);
  // the star
  const [sx, sy] = toScreen(CORE.x, CORE.y, 0);
  const R = 104 * k * breathe * bloomK;
  gl.useProgram(coreProg); attr(bQuadC, CQ.uv, 2);
  gl.uniform2f(CQ.c, sx, sy); gl.uniform1f(CQ.r, R); gl.uniform2f(CQ.view, W, H); gl.uniform1f(CQ.gain, 1.0); gl.uniform1f(CQ.core, 8 * k * bloomK / Math.max(1, R));
  gl.uniform4fv(CQ.quiet, quietRects); gl.uniform1f(CQ.qf, QUIET_FEATHER); gl.uniform4fv(CQ.qbox, quietBox);
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
}
let setCommonU = null;

// ---- link fibres: SEG+1 points per link, indexed lines ----
const SEG = 10, LP = SEG + 1, LV = L * LP;
const lpts = new Float32Array(LV * 2), lz = new Float32Array(LV), lcol = new Uint8Array(LV * 4);
const lidx = new Uint32Array(L * SEG * 2);
for (let k = 0; k < L; k++) for (let s = 0; s < SEG; s++) { lidx[(k * SEG + s) * 2] = k * LP + s; lidx[(k * SEG + s) * 2 + 1] = k * LP + s + 1; }
for (let k = 0; k < L; k++) { const zs = D.z[D.ls[k]], zt = D.z[D.lt[k]]; for (let p = 0; p < LP; p++) lz[k * LP + p] = zs + (zt - zs) * p / SEG; }
const bLPos = buf(lpts, gl.DYNAMIC_DRAW), bLZ = buf(lz, gl.STATIC_DRAW), bLCol = buf(lcol, gl.DYNAMIC_DRAW), bLIdx = buf(lidx, gl.STATIC_DRAW, gl.ELEMENT_ARRAY_BUFFER);
// the link's own tangent at each end, oriented toward the other end; cross-cluster links bend toward the core
function buildLinks() {
  const tan = D.tan, ls = D.ls, lt = D.lt, bend = D.lbend, cx = CORE.x, cy = CORE.y;
  for (let k = 0; k < L; k++) {
    const s = ls[k], t = lt[k];
    const x0 = pos[s * 2], y0 = pos[s * 2 + 1], x3 = pos[t * 2], y3 = pos[t * 2 + 1];
    const dx = x3 - x0, dy = y3 - y0, len = Math.hypot(dx, dy) || 1;
    let tsx = tan[s * 2], tsy = tan[s * 2 + 1]; if (tsx * dx + tsy * dy < 0) { tsx = -tsx; tsy = -tsy; }
    let ttx = tan[t * 2], tty = tan[t * 2 + 1]; if (ttx * dx + tty * dy < 0) { ttx = -ttx; tty = -tty; }
    const h = len * 0.36, b = bend[k];
    let x1 = x0 + tsx * h, y1 = y0 + tsy * h, x2 = x3 - ttx * h, y2 = y3 - tty * h;
    if (b > 0) { x1 += (cx - x1) * b; y1 += (cy - y1) * b; x2 += (cx - x2) * b; y2 += (cy - y2) * b; }
    const o = k * LP * 2;
    for (let p = 0; p <= SEG; p++) {
      const u = p / SEG, v = 1 - u, a = v * v * v, bb = 3 * v * v * u, c = 3 * v * u * u, d = u * u * u;
      lpts[o + p * 2] = a * x0 + bb * x1 + c * x2 + d * x3; lpts[o + p * 2 + 1] = a * y0 + bb * y1 + c * y2 + d * y3;
    }
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, bLPos); gl.bufferSubData(gl.ARRAY_BUFFER, 0, lpts);
}
// ---- ribbons (relationship links) ----
const ribbonLinks = []; for (let k = 0; k < L; k++) if (D.lclass[k] >= 1) ribbonLinks.push(k);
const RL = V.ribbons ? ribbonLinks.length : 0, RVP = LP * 2 + 2, RV = RL * RVP;
const rbuf = new Float32Array(RV * 7), rcol = new Uint8Array(RV * 4);
const bR = buf(rbuf, gl.DYNAMIC_DRAW), bRCol = buf(rcol, gl.DYNAMIC_DRAW);
function buildRibbons() {
  if (!RL) return;
  for (let r = 0; r < RL; r++) {
    const k = ribbonLinks[r], o = k * LP * 2, base = r * RVP;
    const w = D.linkW[k] * (1.0 + 0.9 * D.lhero[k]) * V.ribbonWidth * 1.35;
    for (let p = 0; p <= SEG; p++) {
      const pa = Math.max(0, p - 1), pb = Math.min(SEG, p + 1);
      let dx = lpts[o + pb * 2] - lpts[o + pa * 2], dy = lpts[o + pb * 2 + 1] - lpts[o + pa * 2 + 1]; const l = Math.hypot(dx, dy) || 1; dx /= l; dy /= l;
      const u = p / SEG, taper = Math.pow(Math.sin(u * Math.PI), 0.45) * 0.85 + 0.15, hw = w * taper;
      const x = lpts[o + p * 2], y = lpts[o + p * 2 + 1], z = lz[k * LP + p];
      for (let side = 0; side < 2; side++) {
        let vi = base + 1 + p * 2 + side; if (p === 0 && side === 0) vi = base; // duplicate first
        const q = vi * 7; rbuf[q] = x; rbuf[q + 1] = y; rbuf[q + 2] = -dy; rbuf[q + 3] = dx; rbuf[q + 4] = side ? hw : -hw; rbuf[q + 5] = u; rbuf[q + 6] = z;
        if (p === 0 && side === 0) { const q2 = (base + 1) * 7; for (let c = 0; c < 7; c++) rbuf[q2 + c] = rbuf[q + c]; }
      }
    }
    const last = (base + RVP - 1) * 7, prev = (base + RVP - 2) * 7; for (let c = 0; c < 7; c++) rbuf[last + c] = rbuf[prev + c];
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, bR); gl.bufferSubData(gl.ARRAY_BUFFER, 0, rbuf);
}
// ---- the river: 4,800 source fibres on the stem strands (static geometry) ----
let SVN = 0; const srcStart = new Int32Array(S + 1);
for (let si = 0; si < S; si++) { srcStart[si] = SVN; SVN += D.strands[D.srcStrand[si]].n; } srcStart[S] = SVN;
const spts = new Float32Array(SVN * 2), sz = new Float32Array(SVN), scol = new Uint8Array(SVN * 4);
const sidxArr = []; const srcAlpha = new Float32Array(S);
{
  for (let si = 0; si < S; si++) {
    const st = D.strands[D.srcStrand[si]], n = st.n, P = st.P, base = srcStart[si], perp = D.srcPerp[si], zz = (rnd() - 0.5) * 0.8;
    const hero = st.lum > 0.62 && rnd() < 0.35;
    // ten sources share each stem strand: together they carry one of v4's strokes
    srcAlpha[si] = (hero ? 0.07 : 0.026) + rnd() * 0.02;
    for (let p = 0; p < n; p++) {
      const pa = Math.max(0, p - 1), pb = Math.min(n - 1, p + 1);
      let dx = P[pb * 2] - P[pa * 2], dy = P[pb * 2 + 1] - P[pa * 2 + 1]; const l = Math.hypot(dx, dy) || 1; dx /= l; dy /= l;
      spts[(base + p) * 2] = P[p * 2] - dy * perp; spts[(base + p) * 2 + 1] = P[p * 2 + 1] + dx * perp; sz[base + p] = zz;
      if (p) { sidxArr.push(base + p - 1, base + p); }
    }
  }
}
const sidx = new Uint32Array(sidxArr);
const bSPos = buf(spts, gl.STATIC_DRAW), bSZ = buf(sz, gl.STATIC_DRAW), bSCol = buf(scol, gl.DYNAMIC_DRAW), bSIdx = buf(sidx, gl.STATIC_DRAW, gl.ELEMENT_ARRAY_BUFFER);
let srcColDirty = true;
function buildSourceColors(dimAll) {
  for (let si = 0; si < S; si++) {
    const st = D.strands[D.srcStrand[si]], base = srcStart[si], n = st.n, dc = D.srcDomain[si] * 3;
    const node = D.srcNode[si];
    const a = srcAlpha[si] * V.riverGain * (dimAll ? 0.45 : 1) * (inDom.length && domHover >= 0 ? (D.srcDomain[si] === domHover ? 1.4 : 0.5) : 1);
    for (let p = 0; p < n; p++) {
      const u = p / (n - 1);                                   // 0 = brain end
      const t = clamp(u * 3, 0, 2.999), kk = Math.floor(t), f = t - kk;
      const c0 = st.cols[kk], c1 = st.cols[kk + 1];
      // the strand's own colour toward the river, its domain's colour toward the brain
      const m = 1 - Math.pow(1 - u, 2.2) * 0.45;
      const r = (D.domCol[dc] * (1 - m) + (c0[0] + (c1[0] - c0[0]) * f) * m);
      const g = (D.domCol[dc + 1] * (1 - m) + (c0[1] + (c1[1] - c0[1]) * f) * m);
      const b = (D.domCol[dc + 2] * (1 - m) + (c0[2] + (c1[2] - c0[2]) * f) * m);
      const q = (base + p) * 4, aa = a * (0.55 + 0.45 * Math.min(1, u * 4)) * (u > 0.92 ? (1 - u) / 0.08 : 1);
      scol[q] = r * aa * 255; scol[q + 1] = g * aa * 255; scol[q + 2] = b * aa * 255; scol[q + 3] = aa * 255;
    }
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, bSCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, scol);
  srcColDirty = false;
}
// ---- nodes ----
const nodeCol = new Float32Array(N * 4);
const bNPos = buf(pos, gl.DYNAMIC_DRAW), bNCol = buf(nodeCol, gl.DYNAMIC_DRAW), bNGlow = buf(glow, gl.DYNAMIC_DRAW);
const nSize = new Float32Array(N); for (let i = 0; i < N; i++) nSize[i] = D.radius[i] * V.nodeScale;
const bNSize = buf(nSize, gl.STATIC_DRAW), bNKind = buf(kind, gl.STATIC_DRAW), bNLight = buf(D.lightR, gl.STATIC_DRAW), bNPhase = buf(D.lightPhase, gl.STATIC_DRAW), bNZ = buf(D.z, gl.STATIC_DRAW);
const litIdx = new Uint32Array(N); const bLitIdx = gl.createBuffer();
// ---- packets ----
const packets = [];
{
  const cand = []; for (let k = 0; k < L; k++) { const s = D.ls[k], t = D.lt[k]; const act = (D.activity[s] + D.activity[t]) * 0.5; if (D.lphase[k] < V.packetShare * (0.5 + act)) cand.push(k); }
  for (const k of cand) { const s = D.ls[k], t = D.lt[k]; packets.push({ k, p: rnd(), dur: (6 + rnd() * 8) / V.packetSpeed, dir: D.degree[s] >= D.degree[t] ? 1 : -1 }); }
}
const river = [];
for (let i = 0; i < V.riverPackets; i++) river.push({ si: Math.floor(rnd() * S), u: rnd(), speed: (0.05 + rnd() * 0.05) * V.packetSpeed });
const PK = packets.length + river.length;
const pkPos = new Float32Array(PK * 2), pkCol = new Float32Array(PK * 4), pkSize = new Float32Array(PK), pkGlow = new Float32Array(PK).fill(0.7), pkZero = new Float32Array(PK), pkZ = new Float32Array(PK);
const bPkPos = buf(pkPos, gl.DYNAMIC_DRAW), bPkCol = buf(pkCol, gl.DYNAMIC_DRAW), bPkSize = buf(pkSize, gl.DYNAMIC_DRAW), bPkGlow = buf(pkGlow, gl.STATIC_DRAW), bPkZero = buf(pkZero, gl.STATIC_DRAW), bPkZ = buf(pkZ, gl.DYNAMIC_DRAW);
// ---- glow FBOs ----
let fbo = null;
function makeFbo(w, h) {
  const tex = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  const fb = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER, fb); gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  return { tex, fb, w, h };
}
function ensureFbo() {
  if (!fboDirty && fbo) return;
  const w = Math.max(64, Math.round(W / 4)), h = Math.max(64, Math.round(H / 4));
  if (fbo) { for (const f of [fbo.a, fbo.b]) { gl.deleteTexture(f.tex); gl.deleteFramebuffer(f.fb); } }
  fbo = { a: makeFbo(w, h), b: makeFbo(w, h), w, h };
  fboDirty = false;
}
const blurW = new Float32Array(7); { let s = 0; for (let i = 0; i < 7; i++) { blurW[i] = Math.exp(-(i * i) / (2 * V.glowSigma * V.glowSigma)); s += i ? 2 * blurW[i] : blurW[i]; } for (let i = 0; i < 7; i++) blurW[i] /= s; }

// ---------------- the quiet rectangles under the top-left block and the workforce ----------------
const QUIET_PAD = 14, QUIET_FEATHER = 120, QUIET_N = 8;   // a wider feather than round-3/v1: the fibres are dense enough to show a rectangle otherwise
const quietRects = new Float32Array(QUIET_N * 4), quietBox = new Float32Array(8);
const quietRect = { x0: 0, y0: 0, x1: 0, y1: 0 };
let quietDirty = true;
// APP: THE TICKER IS NOT A QUIET GROUP, AND THAT IS THE CEO'S RULING OF 2026-09-17.
//
//   "On the home screen, when one of the temporary text lines appears at the bottom of the text
//    block on the right ... it overlays a rectangular shadow over the entire text area and that
//    shadow area always stays there afterwards which is blocking a part of the loro visual.
//    Remove this oversized shadow overlay ... instead of this 'global overlay' that affects the
//    entire text block, make it so that only the temporary text lines have a drop shadow. And
//    make sure that none of that shadow is left behind after the temporary text line
//    disappears."
//
// WHAT HE SAW, MEASURED — 1440x900, the right half of the viewport (720x900 = 648,000 px),
// `quietAt()` sampled on a 2px lattice, before any line had landed / while one was up / two
// seconds after it had gone:
//
//     touched (k < 0.99)   156,640 -> 261,672 -> 259,624 px
//     erased  (k <= 0.01)   57,564 -> 118,000 -> 116,912 px
//
// The hush MORE THAN DOUBLED when the line appeared and gave back 1.8% of that when it left.
// Two independent reasons, and removing the group is what answers both: `landSpark` raised
// `quietDirty` when the line went up and nothing raised it when the line came down, AND this
// pass reads `textContent`, which the line keeps forever — `ticker.classList.remove('on')` only
// takes the OPACITY to zero, so a rect recomputed after the fade would have been the same rect.
//
// The line's legibility was what the rect bought. It is bought instead by the line's own drop
// shadow, `#home-live .ticker` in `home.css`, which is sized to the glyphs and leaves with
// them. Its ratios are measured on the rendered frame in `tests/home.js` — the ticker is a
// contrast row there now, at 4.5:1, in the `on` state where a reader can see it.
//
// SEVEN GROUPS NOW, AND `QUIET_N` STAYS 8. The unused slot keeps its `-1e4` fill, which makes
// `quietK`'s distance enormous and its `smoothstep` exactly 1 — the shader loop is a fixed
// `for (int r = 0; r < 8; r++)` in GLSL ES 1.0 and cannot be shortened anyway.
function computeQuiet() {
  const groups = [Array.from(document.querySelectorAll('#home-brand > *'))].concat(Array.from(document.querySelectorAll('#home-signals .sig')).map(sig => Array.from(sig.children)))
    .concat([[$('#home-live .cap'), $('#home-working')]]);
  quietRects.fill(-1e4);
  let ux0 = Infinity, uy0 = Infinity, ux1 = -Infinity, uy1 = -Infinity;
  groups.slice(0, QUIET_N).forEach((els, g) => {
    const topLeft = g < 6;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    els.forEach(el => {
      if (!el || !el.textContent.trim()) return;
      const r = el.getBoundingClientRect(); if (r.width === 0) return;
      if (r.left < x0) x0 = r.left; if (r.top < y0) y0 = r.top; if (r.right > x1) x1 = r.right; if (r.bottom > y1) y1 = r.bottom;
    });
    if (x0 === Infinity) return;
    quietRects[g * 4] = x0 - QUIET_PAD; quietRects[g * 4 + 1] = y0 - QUIET_PAD; quietRects[g * 4 + 2] = x1 + QUIET_PAD; quietRects[g * 4 + 3] = y1 + QUIET_PAD;
    if (!topLeft) return;
    if (x0 < ux0) ux0 = x0; if (y0 < uy0) uy0 = y0; if (x1 > ux1) ux1 = x1; if (y1 > uy1) uy1 = y1;
  });
  if (ux0 !== Infinity) { quietRect.x0 = ux0 - QUIET_PAD; quietRect.y0 = uy0 - QUIET_PAD; quietRect.x1 = ux1 + QUIET_PAD; quietRect.y1 = uy1 + QUIET_PAD; }
  quietBox.fill(-1e4);
  for (let b = 0; b < 2; b++) {
    let bx0 = Infinity, by0 = Infinity, bx1 = -Infinity, by1 = -Infinity;
    for (let g = b ? 6 : 0; g < (b ? QUIET_N : 6); g++) { const o = g * 4; if (quietRects[o + 2] < 0) continue; if (quietRects[o] < bx0) bx0 = quietRects[o]; if (quietRects[o + 1] < by0) by0 = quietRects[o + 1]; if (quietRects[o + 2] > bx1) bx1 = quietRects[o + 2]; if (quietRects[o + 3] > by1) by1 = quietRects[o + 3]; }
    if (bx0 === Infinity) continue;
    quietBox[b * 4] = bx0 - QUIET_FEATHER; quietBox[b * 4 + 1] = by0 - QUIET_FEATHER; quietBox[b * 4 + 2] = bx1 + QUIET_FEATHER; quietBox[b * 4 + 3] = by1 + QUIET_FEATHER;
  }
  // Recomputed HERE and only here, on the same signal and at the same cadence: the banner's
  // box moves when the window does, and reading it per frame would force a layout 60 times a
  // second for an answer that changes on `resize`. Same discipline as `readTickerZone`.
  computeBlockers();
  quietDirty = false;
}
function quietAt(sx, sy) {
  let k = 1;
  for (let g = 0; g < QUIET_N; g++) {
    const o = g * 4;
    const dx = Math.max(quietRects[o] - sx, sx - quietRects[o + 2], 0), dy = Math.max(quietRects[o + 1] - sy, sy - quietRects[o + 3], 0);
    const kk = clamp(Math.hypot(dx, dy) / QUIET_FEATHER, 0, 1);
    if (kk < k) k = kk;
  }
  return k * k * (3 - 2 * k);
}
// APP: WHERE A CAPTION MAY NOT GO — and it is a DIFFERENT list from the hush, for the same
// reason the ticker zone is a different list from the hush.
//
// THE DEFECT THIS CLOSES (the 2026-09-17 candidate-.2 walk, §4 defect #4, reproduced here
// before it was fixed). At the app's own documented minimum window, 1024x700, the picture
// wrote "INFRASTRUCTUR" — the final E missing. Nothing clipped it at the canvas edge; over a
// 12x11 cursor sweep at that size NOT ONE label left the viewport. `clearQuiet()` erased it.
// The numbers, read off the running page:
//
//     the right column's erase rect   x0 = 775.27   (#home-live .cap / #home-working, +14 pad)
//     INFRASTRUCTURE's own box        x1 = 801.36   -> 26.09px of the label inside the erase
//     its CENTER                      sx = 692.38   -> 82.9px clear of the rect
//     quietAt(sx, sy)                      0.77     -> far above the 0.18 gate, so: drawn
//
// The gate was sampled at the label's MIDDLE and the erase is applied to the label's BOX.
// A label wider than about 68px can therefore pass a gate its own tail fails, and then have
// that tail rubbed out by a pass that runs after it. Same sweep, same size: 15 node labels
// were drawn UNDER the opaque first-run banner and 106 were drawn across a domain name.
//
// So the box is what is tested now, against three kinds of rect:
//
//   * the ERASE rects, because a caption drawn where the picture is about to be rubbed out is
//     a caption that will be cut mid-glyph. These are `quietRects` themselves — the very
//     array `clearQuiet()` paints — never a second copy of the same idea.
//   * the FIRST-RUN BANNER, which is opaque DOM (`rgba(12,19,34,0.97)`) sitting over the
//     canvas. A label under it is not dimmed, it is gone.
//   * the temporary line's zone, which `inTickerZone` already owned and still does.
//
// THE BANNER IS NOT MADE A QUIET GROUP, and the free eighth slot is not the reason to make it
// one. A quiet group hushes the stars for QUIET_FEATHER = 120px around itself, and the CEO's
// ruling of 2026-09-17 was about exactly that: *"it overlays a rectangular shadow over the
// entire text area ... Remove this oversized shadow overlay"*. Putting a 120px feathered hush
// around the banner would answer this defect by committing the one he had just ruled out.
// Nothing here changes a pixel of the picture's light; it only decides where a caption goes.
let blockRects = [];
function computeBlockers() {
  blockRects = [];
  for (let g = 0; g < QUIET_N; g++) {
    const o = g * 4; if (quietRects[o + 2] < 0) continue;
    blockRects.push([quietRects[o], quietRects[o + 1], quietRects[o + 2], quietRects[o + 3]]);
  }
  const note = $('#home-note');
  if (note && !note.hidden) {
    const r = note.getBoundingClientRect();
    if (r.width > 0) blockRects.push([r.left, r.top, r.right, r.bottom]);
  }
}
const hitsRect = (b, x0, y0, x1, y1) => x0 < b[2] && x1 > b[0] && y0 < b[3] && y1 > b[1];
/// `also` is a list of `{x0, y0, x1, y1}` boxes that are not chrome — the domain names ALREADY
/// PLACED this frame. Two names could overlap each other freely before it existed, which is
/// audit-7 row 13's second half: `CAPITAL 558` was sitting under `LEGAL & RISK` on the CEO's
/// home screen, and had been since audit-6. The chrome rects are kept in `blockRects` because
/// they are re-read per frame from the DOM; a label's box is only known part-way through the
/// label pass, so it arrives as an argument rather than being pushed into that array.
function blockedBox(x0, y0, x1, y1, also) {
  if (x0 < 0 || x1 > W || y0 < 0 || y1 > H) return true;
  for (let k = 0; k < blockRects.length; k++) if (hitsRect(blockRects[k], x0, y0, x1, y1)) return true;
  if (also) {
    for (let k = 0; k < also.length; k++) {
      const a = also[k];
      if (hitsRect([a.x0, a.y0, a.x1, a.y1], x0, y0, x1, y1)) return true;
    }
  }
  return inTickerZone(x0, y0, x1, y1);
}
// THE SMALLEST SIDEWAYS MOVE THAT CLEARS EVERYTHING, or `null` for "there isn't one".
//
// Horizontal only, and that is not laziness: every rect above is a tall column or a wide band
// at the top, so the axis with room in it is x. A label that cannot fit fades out instead —
// which is this file's own rule ("A label is either legible or absent"), and clipping was
// always the third state that rule says does not exist.
//
// 64px is the budget. The largest shift any label actually asked for at 1024x700 was
// INFRASTRUCTURE's 26.09px, so the budget is 2.45x the worst measured case and still small
// against a domain cluster, which spans 150-250px on screen at that size: the name stays over
// its own stars. It is a budget rather than an unbounded solve because an unbounded one would
// happily park a domain name over somebody else's cluster to keep it on screen.
const LABEL_NUDGE_MAX = 64;
function clearingShift(x0, y0, x1, y1, also) {
  if (!blockedBox(x0, y0, x1, y1, also)) return 0;
  const tries = [];
  for (let k = 0; k < blockRects.length; k++) {
    const b = blockRects[k];
    if (!hitsRect(b, x0, y0, x1, y1)) continue;
    tries.push(b[0] - x1, b[2] - x0);
  }
  // The same two candidate moves for a name already on screen: just clear of its left edge, or
  // just clear of its right. A domain name is 100-250px wide, so one of these is usually well
  // inside the 64px budget and the other is not — and when neither is, `null` fades this label
  // rather than printing it on top of somebody else's, which is this file's own rule ("A label
  // is either legible or absent").
  if (also) {
    for (let k = 0; k < also.length; k++) {
      const a = also[k];
      if (!hitsRect([a.x0, a.y0, a.x1, a.y1], x0, y0, x1, y1)) continue;
      tries.push(a.x0 - x1, a.x1 - x0);
    }
  }
  if (tickerZone && inTickerZone(x0, y0, x1, y1)) tries.push(tickerZone[0] - x1, tickerZone[2] - x0);
  tries.push(-x0, W - x1);
  tries.sort((a, b) => Math.abs(a) - Math.abs(b));
  for (const s of tries) {
    if (Math.abs(s) > LABEL_NUDGE_MAX) continue;
    if (!blockedBox(x0 + s, y0, x1 + s, y1, also)) return s;
  }
  return null;
}
// A NAME-ON-NAME COLLISION IS RESOLVED UP OR DOWN FIRST, and the numbers are why.
//
// `clearingShift` is horizontal-only for a good reason that does not hold here: every rect in
// `blockRects` is a tall column or a wide band at the top, so against CHROME the axis with room
// in it is x. Two domain names are neither. Measured on the shipped composition at 1024x700 —
// the `CAPITAL 558` / `LEGAL & RISK` pair audit-7 row 13 names, at cursor 380,90:
//
//     CAPITAL      [370.2, 404.7 -> 478.7, 446.7]
//     LEGAL & RISK [391.2, 435.5 -> 556.3, 477.5]
//     horizontal overlap  87.5px       vertical overlap  11.2px
//
// So sideways it needs 87.5px, which is past the 64px budget and would fade the name; a 12px
// lift clears it outright. The whole 132-position sweep, measured three ways at 1024x700 —
// domain names on screen per cursor position, and overlapping pairs found:
//
//     as it shipped (no name-vs-name test at all)   7.28 names   189 overlaps
//     horizontal-only                               5.82 names     0 overlaps
//     lift first, then horizontal                   6.87 names     0 overlaps
//
// Horizontal-only bought the clean screen by fading 1.46 names per position, and nearly all of
// those were collisions a few pixels of height would have settled. 1400x880 moves the same way:
// 11.38 with overlaps, 10.36 clean with the lift.
//
// 22px IS THE LIFT BUDGET. The line box is 42px tall (`y0 = sy - 14`, `y1 = sy + 28`, the name
// and the count beneath it), so 22 is just over half of it — enough to separate two names that
// are stacked, and far short of moving a name off the cluster it belongs to, which spans
// 150-250px on screen at this size. It is tried only against the OTHER NAMES: a lift into the
// chrome is refused by `blockedBox` on the next line like any other position.
const LABEL_LIFT_MAX = 22;
function clearingNudge(x0, y0, x1, y1, also) {
  if (!blockedBox(x0, y0, x1, y1, also)) return { dx: 0, dy: 0 };
  if (also && also.length) {
    const tries = [];
    for (let k = 0; k < also.length; k++) {
      const a = also[k];
      if (!hitsRect([a.x0, a.y0, a.x1, a.y1], x0, y0, x1, y1)) continue;
      tries.push(a.y0 - y1, a.y1 - y0);
    }
    tries.sort((p, q) => Math.abs(p) - Math.abs(q));
    for (const dy of tries) {
      if (Math.abs(dy) > LABEL_LIFT_MAX) continue;
      if (!blockedBox(x0, y0 + dy, x1, y1 + dy, also)) return { dx: 0, dy };
    }
  }
  const dx = clearingShift(x0, y0, x1, y1, also);
  return dx === null ? null : { dx, dy: 0 };
}
// The rows themselves are erased completely; the erase feathers out around them, so a big soft thing
// (the core, a ripple) that drifts under the block fades rather than being cut by a rectangle.
function clearQuiet() {
  ctx.save(); ctx.globalCompositeOperation = 'destination-out';
  for (let g = 0; g < QUIET_N; g++) {
    const o = g * 4; if (quietRects[o + 2] < 0) continue;
    const x0 = quietRects[o], y0 = quietRects[o + 1], w = quietRects[o + 2] - x0, h = quietRects[o + 3] - y0;
    ctx.filter = 'blur(14px)'; ctx.fillStyle = '#000'; ctx.fillRect(x0 - 6, y0 - 6, w + 12, h + 12);
    ctx.filter = 'none'; ctx.fillRect(x0, y0, w, h);
  }
  ctx.restore();
}

// ---------------- motion: bloom, then springs ----------------
const tmp4 = new Float32Array(4);
function ease(x) { return 1 - Math.pow(1 - x, 3.2); }
function stepBloom(t) {
  let moving = false;
  for (let i = 0; i < N; i++) {
    const d = D.delay[i] * V.bloom, du = D.dur[i] * V.bloom;
    const x = (t - d) / du;
    if (x >= 1) { if (nAlpha[i] < 1) { nAlpha[i] = 1; pos[i * 2] = D.home[i * 2]; pos[i * 2 + 1] = D.home[i * 2 + 1]; moving = true; } continue; }
    moving = true;
    if (x <= 0) { nAlpha[i] = 0; pos[i * 2] = CORE.x; pos[i * 2 + 1] = CORE.y; continue; }
    const e = ease(x);
    const s = D.strands[D.nStrand[i]], u = D.nU[i], perp = D.nPerp[i];
    D.strandAt(s, u * e, tmp4);
    const px = tmp4[0] - tmp4[3] * perp * e, py = tmp4[1] + tmp4[2] * perp * e;
    D.strandAt(s, u, tmp4);
    const hx = tmp4[0] - tmp4[3] * perp, hy = tmp4[1] + tmp4[2] * perp;
    pos[i * 2] = px + (D.home[i * 2] - hx) * e; pos[i * 2 + 1] = py + (D.home[i * 2 + 1] - hy) * e;
    nAlpha[i] = clamp(x / 0.3, 0, 1);
  }
  return moving;
}
function activate(i) { if (!active[i]) { active[i] = 1; activeList.push(i); } }
const waves = [];
function stepSprings(dt) {
  // wave displacement: a ring travelling outward, nodes pushed along it, back to rest behind it
  const now = performance.now();
  for (let w = waves.length - 1; w >= 0; w--) {
    const wv = waves[w]; const age = (now - wv.t0) / 1000; const R = wv.c * age;
    if (R > wv.maxR) { waves.splice(w, 1); continue; }
    const env = Math.exp(-age / wv.tau);
    for (let i = 0; i < N; i++) {
      const dx = D.home[i * 2] - wv.x, dy = D.home[i * 2 + 1] - wv.y, r = Math.hypot(dx, dy);
      const ph = (r - R) / wv.w; if (ph > 2.2 || ph < -2.2) continue;
      const bump = Math.exp(-ph * ph) * env * wv.A;
      const ir = 1 / (r || 1);
      activate(i); tgt[i * 2] += dx * ir * bump; tgt[i * 2 + 1] += dy * ir * bump;
      glow[i] = Math.max(glow[i], Math.exp(-ph * ph) * env * 0.9);
    }
  }
  let moved = false;
  const k = 90, c = 2 * Math.sqrt(k) * 1.0, kd = 420, cd = 2 * Math.sqrt(kd) * 1.05;
  const keep = [];
  for (const i of activeList) {
    const o = i * 2, isDrag = i === dragging;
    const kk = isDrag ? kd : k, cc = isDrag ? cd : c;
    for (let a = 0; a < 2; a++) {
      const acc = kk * (tgt[o + a] - off[o + a]) - cc * vel[o + a];
      vel[o + a] += acc * dt; off[o + a] += vel[o + a] * dt;
    }
    const still = Math.abs(off[o]) < 0.02 && Math.abs(off[o + 1]) < 0.02 && Math.abs(vel[o]) < 0.5 && Math.abs(vel[o + 1]) < 0.5 && tgt[o] === 0 && tgt[o + 1] === 0;
    if (still) { off[o] = 0; off[o + 1] = 0; vel[o] = 0; vel[o + 1] = 0; active[i] = 0; }
    else keep.push(i);
    pos[o] = D.home[o] + off[o]; pos[o + 1] = D.home[o + 1] + off[o + 1];
    moved = true;
  }
  activeList = keep;
  // targets are rebuilt every frame by whoever holds them (drag, waves, blooms)
  for (const i of activeList) { tgt[i * 2] = 0; tgt[i * 2 + 1] = 0; }
  return moved;
}
function pushWave(x, y, A) { waves.push({ x, y, t0: performance.now(), c: 560, w: 55, tau: 1.6, A: A || V.waveAmp, maxR: 1400 }); }

// ---------------- interaction state ----------------
const cursor = { x: -9999, y: -9999, inside: false };
let hover = -1, selected = -1, dragging = -1, dragMoved = 0, panning = false, panStart = null, downAt = 0;
let dragNbr = null; let domHover = -1, domSel = -1, domLabelRects = [];
let clusterSel = -1;
const ripples = [], sparks = [];
let ticker = $('#home-ticker'), tickerTimer = 0;

// APP: THE TEMPORARY LINE IS A NO-LABEL ZONE, AND THAT IS NOT THE SAME THING AS A QUIET RECT.
//
// The quiet rectangles did two jobs at once, and only one of them was the CEO's complaint. They
// hush the STARS (`quietK` in the shaders), and they also keep the picture's OWN TEXT out of the
// chrome's area — `drawOverlay` drops a domain label where `quietAt() < 0.18` and a node label
// where it is `< 0.6`. Taking the ticker out of the quiet pass therefore did more than remove
// the wash he saw: it let the picture write INFRASTRUCTURE and TALENT, 16px ink with their own
// 0.98 halo, straight through the line. Measured on twelve consecutive frames with the line up
// and the shadow already in place: the line's ink read 1.19-1.34:1 and its gold run 1.00-1.02:1
// against those glyphs — a floor failure in every frame, not a rare one, and one no text shadow
// can answer because the thing behind the text is other text.
//
// So the two jobs are separated. NOTHING IS DIMMED HERE — this zone changes no pixel of the
// picture's light. It is only a place the picture does not put a CAPTION while a temporary line
// is using it, and it is null whenever the line is not up.
//
// Read at the three moments the answer can change and cached, never per frame: a
// `getBoundingClientRect()` inside `drawOverlay` would force a layout on every one of 60 frames
// a second. It is cleared on `transitionend` rather than on the class change, because the line
// fades over `opacity 0.6s` and is still readable for all of it — the exact duration is the
// stylesheet's to own, not a 600 typed here.
let tickerZone = null;
function readTickerZone() {
  if (!ticker || !ticker.classList.contains('on')) { tickerZone = null; return; }
  const r = ticker.getBoundingClientRect();
  if (r.width === 0) { tickerZone = null; return; }
  tickerZone = [r.left - 8, r.top - 6, r.right + 8, r.bottom + 6];
}
const inTickerZone = (x0, y0, x1, y1) => !!tickerZone && x0 < tickerZone[2] && x1 > tickerZone[0] && y0 < tickerZone[3] && y1 > tickerZone[1];
if (ticker) ticker.addEventListener('transitionend', (e) => { if (e.propertyName === 'opacity') readTickerZone(); });
let lastIngest = 0, nextIngestGap = 4000;

function setNeighbours(i) {
  isNbr.fill(0); linkLit.fill(0);
  if (i < 0) return;
  if (V.constellation) {
    const c = D.nodeCluster[i]; for (const j of D.byCluster[c]) isNbr[j] = 1;
    for (let k = 0; k < L; k++) if (D.nodeCluster[D.ls[k]] === c && D.nodeCluster[D.lt[k]] === c) linkLit[k] = 0.7;
    for (let k = D.nlinkStart[i]; k < D.nlinkStart[i + 1]; k++) linkLit[D.nlink[k]] = 1;
    for (let k = D.adjStart[i]; k < D.adjStart[i + 1]; k++) isNbr[D.adj[k]] = 1;
    return;
  }
  for (let k = D.adjStart[i]; k < D.adjStart[i + 1]; k++) isNbr[D.adj[k]] = 1;
  for (let k = D.nlinkStart[i]; k < D.nlinkStart[i + 1]; k++) linkLit[D.nlink[k]] = 1;
}
function screenRadius(i) { return Math.max(D.radius[i] * V.nodeScale, D.isLight[i] ? D.lightR[i] * 0.6 : 0) * Math.sqrt(cam.s / home.s); }
function hitTest(sx, sy) {
  let best = -1, bestD = 1e9;
  for (let i = 0; i < N; i++) {
    const [x, y] = nodeScreen(i);
    const dx = x - sx, dy = y - sy; const r = screenRadius(i) + 5;
    const d2 = dx * dx + dy * dy;
    if (d2 < r * r && d2 - screenRadius(i) * 6 < bestD && quietAt(x, y) > 0.3) { bestD = d2 - screenRadius(i) * 6; best = i; }
  }
  return best;
}
function domainHit(sx, sy) {
  for (const r of domLabelRects) if (sx >= r.x0 && sx <= r.x1 && sy >= r.y0 && sy <= r.y1) return r.d;
  return -1;
}
function setDomHover(d) {
  if (d === domHover) return;
  domHover = d; inDom.fill(0);
  if (d >= 0) for (let i = 0; i < N; i++) if (D.nodeDomain[i] === d) inDom[i] = 1;
  srcColDirty = true;
}

ov.addEventListener('mousemove', (e) => {
  cursor.x = e.clientX; cursor.y = e.clientY; cursor.inside = true;
  if (dragging >= 0) {
    dragMoved += Math.abs(e.movementX) + Math.abs(e.movementY);
    return;
  }
  if (panning) {
    cam.x -= e.movementX / cam.s; cam.y -= e.movementY / cam.s; camT.x = cam.x; camT.y = cam.y;
    return;
  }
  // a territory name is aimed at deliberately: inside its box, it wins over the motes beneath it
  const dh = domainHit(e.clientX, e.clientY);
  const h = dh >= 0 ? -1 : hitTest(e.clientX, e.clientY);
  if (h !== hover) { hover = h; if (selected < 0) setNeighbours(h); }
  setDomHover(dh);
  ov.className = h >= 0 || dh >= 0 ? 'point' : '';
});
ov.addEventListener('mouseleave', () => { cursor.inside = false; cursor.x = -9999; cursor.y = -9999; if (dragging < 0 && !panning) { hover = -1; if (selected < 0) setNeighbours(-1); setDomHover(-1); } });
ov.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  downAt = performance.now(); dragMoved = 0;
  const h = domainHit(e.clientX, e.clientY) >= 0 ? -1 : hitTest(e.clientX, e.clientY);
  if (h >= 0) {
    dragging = h; ov.className = 'grabbing';
    // the neighbourhood comes along: first degree at half, second at a fifth
    const w = new Map(); w.set(h, 1);
    for (let k = D.adjStart[h]; k < D.adjStart[h + 1]; k++) { const j = D.adj[k]; if (!w.has(j)) w.set(j, 0.5); }
    const first = Array.from(w.keys());
    for (const j of first) { if (j === h) continue; for (let k = D.adjStart[j]; k < D.adjStart[j + 1]; k++) { const m = D.adj[k]; if (!w.has(m)) w.set(m, 0.2); if (w.size > 420) break; } }
    dragNbr = w;
  }
  else if (domainHit(e.clientX, e.clientY) < 0) { panning = true; panStart = [e.clientX, e.clientY]; ov.className = 'grabbing'; }
  else { panStart = [e.clientX, e.clientY]; }
});
addEventListener('mouseup', (e) => {
  if (!running) return;   // PORT: the home screen is put away; this pointer belongs to the app UI
  if (dragging >= 0) {
    const i = dragging; dragging = -1; dragNbr = null;
    if (dragMoved < 5 && performance.now() - downAt < 400) clickNode(i);
    ov.className = hover >= 0 ? 'point' : '';
  } else if (panning) {
    panning = false; ov.className = '';
    const moved = Math.hypot(e.clientX - panStart[0], e.clientY - panStart[1]);
    if (moved < 5 && performance.now() - downAt < 400) clickBackground(e.clientX, e.clientY);
  } else if (panStart) {
    const d = domainHit(e.clientX, e.clientY); panStart = null;
    if (d >= 0 && performance.now() - downAt < 400) clickDomain(d);
  }
});
ov.addEventListener('wheel', (e) => {
  e.preventDefault();
  const f = Math.pow(1.25, -e.deltaY / 100);
  const ns = clamp(camT.s * f, home.s * 0.7, 8);
  const [wx, wy] = [(e.clientX - W / 2) / camT.s + camT.x, (e.clientY - H / 2) / camT.s + camT.y];
  camT.x = wx - (e.clientX - W / 2) / ns; camT.y = wy - (e.clientY - H / 2) / ns; camT.s = ns;
}, { passive: false });
addEventListener('keydown', (e) => { if (running && e.key === 'Escape') releaseAll(); });   // PORT: `running &&` — Escape belongs to whatever surface is up

function clickNode(i) {
  selected = i; hover = i; setNeighbours(i);
  ripples.push({ x: pos[i * 2], y: pos[i * 2 + 1], t0: performance.now(), hue: domCss[D.nodeDomain[i]] });
  pushWave(pos[i * 2], pos[i * 2 + 1], V.waveAmp);
  flash[i] = 1.4;
  if (V.constellation) { clusterSel = D.nodeCluster[i]; }
  camT.x = pos[i * 2]; camT.y = pos[i * 2 + 1]; camT.s = Math.max(cam.s, home.s * V.clickZoom);
}
function clickBackground(sx, sy) {
  const [wx, wy] = toWorld(sx, sy);
  if (Math.hypot(wx - CORE.x, wy - CORE.y) < 40) { // the core: a pulse through the whole brain
    pushWave(CORE.x, CORE.y, V.waveAmp * 1.3); corePulse = 1; ripples.push({ x: CORE.x, y: CORE.y, t0: performance.now(), hue: 'rgb(235,255,240)' });
    return;
  }
  if (selected >= 0 || domSel >= 0 || clusterSel >= 0) { releaseAll(); return; }
  ripples.push({ x: wx, y: wy, t0: performance.now(), hue: 'rgb(230,221,204)' });
  pushWave(wx, wy, V.waveAmp * 0.6);
  if (Math.abs(camT.s - home.s) > 0.01 || Math.abs(camT.x - home.x) > 1 || Math.abs(camT.y - home.y) > 1) { camT.x = home.x; camT.y = home.y; camT.s = home.s; }
}
function clickDomain(d) {
  domSel = d; setDomHover(d);
  let cx = 0, cy = 0, c = 0; for (let i = 0; i < N; i++) if (D.nodeDomain[i] === d) { cx += pos[i * 2]; cy += pos[i * 2 + 1]; c++; }
  camT.x = cx / c; camT.y = cy / c; camT.s = home.s * 1.75;
  ripples.push({ x: cx / c, y: cy / c, t0: performance.now(), hue: domCss[d] });
}
function releaseAll() {
  selected = -1; hover = -1; setNeighbours(-1); domSel = -1; clusterSel = -1; setDomHover(-1);
  camT.x = home.x; camT.y = home.y; camT.s = home.s;
}

// ---------------- idle learning: sources landing in the loro ----------------
const recentSources = loro.sources.filter(s => s.ingestedDay >= loro.meta.horizonDays - 21 && D.index.has(s.primaryNodeId));
const SOURCE_WORD = { document: 'a document', meeting: 'a meeting', call: 'a call', 'email-thread': 'an email thread', report: 'a report', deck: 'a deck', spreadsheet: 'a spreadsheet', chat: 'a chat', 'web-page': 'a web page', contract: 'a contract' };
function ingest(now) {
  const s = recentSources[Math.floor(rnd() * recentSources.length)];
  const i = D.index.get(s.primaryNodeId);
  let who = activeSpecialists.findIndex(sp => sp.domains.includes(s.domain));
  if (who < 0) who = Math.floor(rnd() * activeSpecialists.length);
  const row = document.getElementById(`home-spc-${who}`);
  const rr = row ? row.getBoundingClientRect() : null;
  const from = rr ? [rr.left - 8, rr.top + rr.height / 2] : [W - 60 - rnd() * 120, 44];
  const dot = row && row.querySelector('.dot'); if (dot) { dot.classList.add('hit'); setTimeout(() => dot.classList.remove('hit'), 900); }
  sparks.push({ i, from, t0: now, dur: 1250, col: domCss[D.nodeDomain[i]], src: s, who });
}
function landSpark(sp, now) {
  const i = sp.i;
  flash[i] = 1.6;
  for (let k = D.adjStart[i]; k < D.adjStart[i + 1]; k++) { const j = D.adj[k]; flash[j] = Math.max(flash[j], 0.5); }
  for (let k = D.nlinkStart[i]; k < D.nlinkStart[i + 1]; k++) linkFlash[D.nlink[k]] = 1;
  if (V.corePulseOnLand) corePulse = Math.max(corePulse, 0.6);
  ticker.innerHTML = `${activeSpecialists[sp.who] ? activeSpecialists[sp.who].codename + ' learned from ' : 'Learned from '}${SOURCE_WORD[sp.src.kind] || sp.src.kind} · <i>${domains[D.nodeDomain[i]].label}</i> → ${sp.src.derivedCount} new memor${sp.src.derivedCount === 1 ? 'y' : 'ies'}`;
  // APP: no `quietDirty` here any more. The ticker is not a quiet group (see `computeQuiet`),
  // so nothing about this line moves a rectangle; `setSignals` below raises the flag itself
  // when a number it writes actually changes width.
  ticker.classList.add('on'); readTickerZone();
  clearTimeout(tickerTimer); tickerTimer = setTimeout(() => ticker.classList.remove('on'), 2600);
  // ...AND ONLY WHERE THE NUMBER IS ILLUSTRATIVE. Over his own corpus the counters are his
  // record's and the animation adds nothing to them (see `landed`'s note) — the line still
  // lands, still flashes its own source in the ticker, and does not move his totals.
  if (SYNTHETIC_FIELD) { landed.sources += 1; landed.memories += sp.src.derivedCount; rememberLanded(); }
  setSignals(signalsNow(), true);
}

// ---------------- HUD: the top-left block and the workforce ----------------
const B = loro.bragSignals;
const fmt = (n) => n.toLocaleString('en-US');
const USER = window.RICHOS_USER || { name: 'you' };
const activeSpecialists = loro.specialists.filter(sp => sp.active);
// WHAT THE PICTURE HAS WATCHED ITSELF LEARN, and it used to go BACKWARDS every launch —
// audit-7 row 12, carried from audit-6 #5. Ray read 7,673 memories and 4,853 sources before a
// relaunch and 7,520 / 4,804 after it, because these two counters climb as the field lands its
// learning lines and then started again from zero on the next boot. A number that only ever
// grows while he watches and drops the moment he reopens the app reads as memory being LOST.
//
// TWO DIFFERENT DATASETS AND TWO DIFFERENT ANSWERS, which is the whole of the fix:
//
//   * THE DEMONSTRATION. The counters are the round's own and the card on screen declares them
//     illustrative. The ticking up is the approved treatment and is kept; what is added is that
//     it is remembered, so the number he sees is never lower than the number he last saw. The
//     delta persists under a key that names the dataset, so a different corpus never inherits
//     it, and it is exactly the sum of the lines he watched land — nothing is invented and no
//     growth curve is guessed at.
//
//   * HIS OWN LORO. `home_field.rs` compiles his real corpus and `meta.synthetic === false`
//     says so. There the animation must ADD NOTHING AT ALL: `memories` is a count of his own
//     records, and an ornament incrementing it would put a number on his home screen that
//     nothing on his disk supports. That is a worse defect than the one being fixed and it was
//     shipping too — the same engine draws both datasets, and `landed` was added to the total
//     either way.
//
// `localStorage` is the store, as it is for every other per-window preference here, and the
// same caveat applies: it survives a relaunch, which is the defect, and is lost if the webview's
// storage is cleared, which costs one launch of accumulated illustration.
const SYNTHETIC_FIELD = !(loro && loro.meta && loro.meta.synthetic === false);
const LANDED_KEY = 'richos.homeField.landed.' + ((loro && loro.meta && loro.meta.name) || 'unknown');
const landed = (() => {
  const zero = { sources: 0, memories: 0 };
  if (!SYNTHETIC_FIELD) return zero;
  try {
    const raw = window.localStorage.getItem(LANDED_KEY);
    if (!raw) return zero;
    const v = JSON.parse(raw);
    // Non-negative finite integers or nothing: a value some other version wrote must not be
    // able to make the HUD count down, which is the defect this exists to close.
    const take = (n) => (typeof n === 'number' && isFinite(n) && n >= 0 ? Math.floor(n) : 0);
    return { sources: take(v && v.sources), memories: take(v && v.memories) };
  } catch (e) {
    return zero; // storage denied, or something unreadable. The picture starts from the corpus.
  }
})();
function rememberLanded() {
  if (!SYNTHETIC_FIELD) return;
  try {
    window.localStorage.setItem(LANDED_KEY, JSON.stringify(landed));
  } catch (e) {
    /* Then the counters restart next launch, which is the old behavior rather than a new fault. */
  }
}
// APP: 0/0 IS `NaN`, AND `fmt(NaN)` PUTS THE WORD "NaN" IN THE HUD.
//
// The round's dataset states `ceoMinutesSaved` and `tasksHandledWithoutCeo`, so this was always
// a division of two real numbers. A customer's loro states NEITHER — it holds no tasks and no
// minutes — so `home_field.rs` sends 0 for both and says so in `meta.absent`, and the line
// "of your attention saved" would read `NaN h`. Zero saved hours is what a corpus that knows of
// no tasks knows; `NaN` is a defect wearing a number's clothes.
//
// IDENTICAL OVER THE ROUND'S DATA: `tasksHandledWithoutCeo` is 1,596 there (read out of
// `home/field-data.js`, not remembered), so the guard is truthy and the expression evaluated is
// the one that was approved, character for character.
const hoursSaved = (tasksNoCeo) => (B.tasksHandledWithoutCeo ? Math.round(B.ceoMinutesSaved / 60 * tasksNoCeo / B.tasksHandledWithoutCeo) : 0);
function signalsNow() {
  return { specialists: B.specialistsManaged, active: B.specialistsActiveNow, tasks: B.tasksHandledWithoutCeo, sources: B.sourcesUnderstood + landed.sources,
    decisions: B.decisionsRemembered, lessons: B.lessonsAccumulated, hours: hoursSaved(B.tasksHandledWithoutCeo), memories: N + landed.memories, months: B.monthsWorkingTogether };
}
const shown = {}, vEl = {};
function setSignals(v, flashIt) {
  const put = (k, text) => {
    const el = vEl[k]; if (!el || shown[k] === text) return;
    shown[k] = text; el.textContent = text; quietDirty = true;
    if (flashIt) { el.classList.add('tick'); setTimeout(() => el.classList.remove('tick'), 1100); }
  };
  put('specialists', fmt(v.specialists)); put('active', fmt(v.active)); put('tasks', fmt(v.tasks)); put('sources', fmt(v.sources));
  put('decisions', fmt(v.decisions)); put('lessons', fmt(v.lessons)); put('hours', fmt(v.hours)); put('memories', fmt(v.memories)); put('months', fmt(v.months));
}
{
  // APP: NOT OVER HIS TEAM (product hunt 2026-10-15, finding 7). When `home.js` has put the
  // user's own team and numbers in these columns it says so, and the dataset's lines are not
  // written over them; `setSignals` then finds no counter to write.
  if (!window.RICHOS_HOME_HUD_OWNED) {
  $('#home-owner').textContent = `for ${USER.name}`;
  $('#home-brand-line').innerHTML = `loro · <span class="v" data-k="months"></span> months · <b><span class="v" data-k="memories"></span> memories</b>`;
  const Vv = (k) => `<span class="v" data-k="${k}"></span>`;
  const sig = (n, small, l) => `<div class="sig"><div class="n">${n}${small ? `<small>${small}</small>` : ''}</div><div class="l">${l}</div></div>`;
  $('#home-signals').innerHTML =
    sig(Vv('specialists'), `· ${Vv('active')} working now`, 'AI specialists') +
    sig(Vv('tasks'), '', 'tasks handled without you') +
    sig(Vv('sources'), '', 'sources understood') +
    sig(Vv('decisions'), `· ${Vv('lessons')} lessons`, 'decisions remembered') +
    sig(`${Vv('hours')} h`, '', 'of your attention saved');
  ROOT.querySelectorAll('.v[data-k]').forEach(el => { vEl[el.dataset.k] = el; });
  setSignals(signalsNow(), false);
  $('#home-working').innerHTML = activeSpecialists.map((sp, i) => `<li id="home-spc-${i}">${sp.codename} <span class="fn">${sp.function}</span><span class="dot"></span></li>`).join('');
  }
  // APP: `readTickerZone()` — the line re-wraps and moves with the window, so the zone is
  // re-read here for the same reason `quietDirty` is raised here.
  addEventListener('resize', () => { if (!running) return; quietDirty = true; readTickerZone(); resize(); });   // PORT: a resize while put away is re-read by resume()
}

// ---------------- bokeh (drawn once) ----------------
function drawBokeh() {
  const c = bk.getContext('2d'); c.clearRect(0, 0, W, H);
  const r2 = mulberry32(77);
  const BOK = [[1, .35, .2], [1, .55, .2], [1, .3, .5], [.5, 1, .4], [.3, .6, 1], [1, .8, .3]];
  for (let i = 0; i < 90; i++) {
    const col = BOK[Math.floor(r2() * BOK.length)], big = r2() < 0.3;
    const x = r2() * W, y = r2() * H, r = (big ? 14 + r2() * 18 : 4 + r2() * 8) * (W / 1440);
    const g = c.createRadialGradient(x, y, 0, x, y, r);
    const a = big ? 0.10 : 0.22;
    g.addColorStop(0, `rgba(${col[0] * 255 | 0},${col[1] * 255 | 0},${col[2] * 255 | 0},${a})`); g.addColorStop(0.7, `rgba(${col[0] * 255 | 0},${col[1] * 255 | 0},${col[2] * 255 | 0},${a * 0.7})`); g.addColorStop(1, `rgba(${col[0] * 255 | 0},${col[1] * 255 | 0},${col[2] * 255 | 0},0)`);
    c.fillStyle = g; c.beginPath(); c.arc(x, y, r, 0, Math.PI * 2); c.fill();
  }
}
resize();

// ---------------- frame ----------------
const domCentroid = new Float32Array(domains.length * 4);
let frames = 0, lastNow = performance.now();
let running = true, rafId = 0;   // PORT: see pause()/resume() at the foot of this file
let motionFrames = 0;
function frame() {
  const now = performance.now();
  const dt = Math.min(0.05, (now - lastNow) / 1000); lastNow = now;
  const t = (now - T0) / 1000;
  frames++;

  // camera
  cam.x += (camT.x - cam.x) * 0.1; cam.y += (camT.y - cam.y) * 0.1; cam.s += (camT.s - cam.s) * 0.1;
  if (Math.abs(camT.x - cam.x) < 0.02 && Math.abs(camT.y - cam.y) < 0.02 && Math.abs(camT.s - cam.s) < 1e-4) { cam.x = camT.x; cam.y = camT.y; cam.s = camT.s; }
  // parallax tilt follows the pointer, drifts when idle
  if (V.parallax) {
    if (cursor.inside) { tiltT.x = (cursor.x - W / 2) / (W / 2) * V.parallax; tiltT.y = (cursor.y - H / 2) / (H / 2) * V.parallax; }
    else { tiltT.x = Math.sin(t * 0.21) * V.idleDrift; tiltT.y = Math.cos(t * 0.17) * V.idleDrift * 0.6; }
    tilt.x += (tiltT.x - tilt.x) * 0.035; tilt.y += (tiltT.y - tilt.y) * 0.035;
  }
  if (quietDirty) computeQuiet();

  // idle: learning lands
  if (!blooming && t > 7 && now - lastIngest > nextIngestGap) { lastIngest = now; nextIngestGap = 7000 + rnd() * 5000; ingest(now); }

  // motion
  let moved = false;
  if (blooming) { moved = stepBloom(t); if (t > bloomEnd + 0.05) { blooming = false; for (let i = 0; i < N; i++) { nAlpha[i] = 1; pos[i * 2] = D.home[i * 2]; pos[i * 2 + 1] = D.home[i * 2 + 1]; } moved = true; } }
  else {
    if (dragging >= 0 && dragNbr) {
      const [wx, wy] = toWorld(cursor.x, cursor.y);
      const ddx = wx - D.home[dragging * 2], ddy = wy - D.home[dragging * 2 + 1];
      dragNbr.forEach((w, j) => { activate(j); tgt[j * 2] += ddx * w; tgt[j * 2 + 1] += ddy * w; });
    }
    if (clusterSel >= 0) {
      const ids = D.byCluster[clusterSel]; let cx = 0, cy = 0; for (const j of ids) { cx += D.home[j * 2]; cy += D.home[j * 2 + 1]; } cx /= ids.length; cy /= ids.length;
      for (const j of ids) { activate(j); tgt[j * 2] += (D.home[j * 2] - cx) * 0.16; tgt[j * 2 + 1] += (D.home[j * 2 + 1] - cy) * 0.16; }
    }
    if (activeList.length || waves.length) moved = stepSprings(dt);
  }
  if (moved) { geomDirty = true; motionFrames++; }

  // drop hover if the node drifted from under the cursor
  if (hover >= 0 && selected < 0 && dragging < 0 && cursor.inside) {
    const [sx, sy] = nodeScreen(hover);
    if (Math.hypot(sx - cursor.x, sy - cursor.y) > screenRadius(hover) + 6) { hover = hitTest(cursor.x, cursor.y); setNeighbours(hover); }
  }

  // ---- per-node targets ----
  const src = selected >= 0 ? selected : hover;
  const hoverActive = src >= 0, domActive = domHover >= 0;
  for (let i = 0; i < N; i++) {
    let tf = 1, tl = 0, g = 0;
    const nb = hoverActive && (i === src || isNbr[i]);
    if (hoverActive) tf = nb ? 1 : 0.28;
    else if (domActive) tf = inDom[i] ? 1 : 0.42;
    if (nb) { tl = 1; g = Math.max(g, i === src ? 1 : 0.25); }
    if (domActive && inDom[i] && !hoverActive) g = Math.max(g, 0.12);
    if (D.flags[i] & 4) g = Math.max(g, 0.22 * (0.5 + 0.5 * Math.sin(t * 1.2 + twPhase[i])));
    if (flash[i] > 0.01) { g = Math.max(g, flash[i]); tl = Math.max(tl, Math.min(1, flash[i])); flash[i] *= 0.93; } else flash[i] = 0;
    fade[i] += (tf - fade[i]) * 0.16;
    lit[i] += (tl - lit[i]) * 0.2;
    glow[i] += (g - glow[i]) * (g > glow[i] ? 0.35 : 0.1);
    if (Math.abs(fade[i] - tf) < 0.002) fade[i] = tf;
    if (Math.abs(lit[i] - tl) < 0.002) lit[i] = tl;
    if (glow[i] < 0.003 && g === 0) glow[i] = 0;
    const f = fade[i] * nAlpha[i], l = lit[i], o = i * 3, q = i * 4;
    const tw = D.isLight[i] ? 1 : 1 - V.twinkle * (0.5 + 0.5 * Math.sin(t * (0.7 + 0.5 * (twPhase[i] / 6.28)) + twPhase[i]));
    const r = restCol[o] + (vividCol[o] - restCol[o]) * l, gg = restCol[o + 1] + (vividCol[o + 1] - restCol[o + 1]) * l, b = restCol[o + 2] + (vividCol[o + 2] - restCol[o + 2]) * l;
    const a = f * (D.isLight[i] ? 0.9 : 0.42 + 0.3 * Math.min(1, D.degree[i] / 10) + 0.28 * l) * tw * V.nodeGain;
    nodeCol[q] = r * a; nodeCol[q + 1] = gg * a; nodeCol[q + 2] = b * a; nodeCol[q + 3] = a;
  }
  // ---- link colours ----
  for (let k = 0; k < L; k++) {
    const s = D.ls[k], tt = D.lt[k], o = k * LP * 4;
    const fs = fade[s] * nAlpha[s], ft = fade[tt] * nAlpha[tt];
    let a = Math.min(fs, ft) * D.lalpha[k] * (D.lclass[k] === 0 ? V.hairGain : 1);
    const ll = linkLit[k], fl = linkFlash[k];
    let wr = 0, wg = 0, wb = 0, wm = 0; // white admixture for lit / flashing fibres
    if (ll > 0) { a = Math.max(a, 0.9 * ll * Math.max(fs, ft)); wm = 0.55 * ll; }
    if (fl > 0.01) { a = Math.max(a, 0.85 * fl); wm = Math.max(wm, 0.5 * fl); linkFlash[k] *= 0.95; } else if (fl) linkFlash[k] = 0;
    if (domActive && !hoverActive && D.nodeDomain[s] === domHover && D.nodeDomain[tt] === domHover) a *= 1.5;
    a *= V.fibreGain;
    const c0 = s * 3, c1 = tt * 3;
    for (let p = 0; p <= SEG; p++) {
      const u = p / SEG;
      let r = D.col[c0] + (D.col[c1] - D.col[c0]) * u, g = D.col[c0 + 1] + (D.col[c1 + 1] - D.col[c0 + 1]) * u, b = D.col[c0 + 2] + (D.col[c1 + 2] - D.col[c0 + 2]) * u;
      if (wm) { r += (1 - r) * wm; g += (1 - g) * wm; b += (1 - b) * wm; }
      const q = o + p * 4, aa = Math.min(1, a) * 255;
      lcol[q] = r * aa; lcol[q + 1] = g * aa; lcol[q + 2] = b * aa; lcol[q + 3] = aa;
    }
  }
  if (RL) {
    for (let r = 0; r < RL; r++) {
      const k = ribbonLinks[r], base = r * RVP, o = k * LP * 4;
      for (let v = 0; v < RVP; v++) { const p = clamp(Math.floor((v - 1) / 2), 0, SEG); const q = (base + v) * 4, oq = o + p * 4; rcol[q] = lcol[oq] * 0.85; rcol[q + 1] = lcol[oq + 1] * 0.85; rcol[q + 2] = lcol[oq + 2] * 0.85; rcol[q + 3] = Math.max(1, Math.min(255, lcol[oq + 3] * 0.85)); }
    }
  }
  if (geomDirty) { buildLinks(); buildRibbons(); geomDirty = false; }
  if (srcColDirty) buildSourceColors(hoverActive);
  // ---- packets ----
  {
    let n = 0;
    for (const pk of packets) {
      pk.p += dt / pk.dur; if (pk.p >= 1) pk.p -= 1;
      const u = pk.dir > 0 ? pk.p : 1 - pk.p, fp = u * SEG, p0 = Math.min(SEG - 1, Math.floor(fp)), f = fp - p0, o = pk.k * LP * 2;
      pkPos[n * 2] = lpts[o + p0 * 2] + (lpts[o + p0 * 2 + 2] - lpts[o + p0 * 2]) * f; pkPos[n * 2 + 1] = lpts[o + p0 * 2 + 1] + (lpts[o + p0 * 2 + 3] - lpts[o + p0 * 2 + 1]) * f;
      pkZ[n] = lz[pk.k * LP + p0];
      const s = D.ls[pk.k], tt = D.lt[pk.k], a = Math.min(fade[s], fade[tt]) * Math.min(nAlpha[s], nAlpha[tt]) * (0.55 + 0.45 * Math.sin(pk.p * Math.PI)) * V.packetGain;
      const c = D.col, cs = s * 3;
      pkCol[n * 4] = (c[cs] * 0.4 + 0.6) * a; pkCol[n * 4 + 1] = (c[cs + 1] * 0.4 + 0.6) * a; pkCol[n * 4 + 2] = (c[cs + 2] * 0.4 + 0.6) * a; pkCol[n * 4 + 3] = a;
      pkSize[n] = 1.1 + 0.5 * D.lhero[pk.k]; n++;
    }
    for (const rv of river) {
      rv.u -= dt * rv.speed; if (rv.u <= 0) { rv.u = 1; rv.si = Math.floor(rnd() * S); }
      const base = srcStart[rv.si], cnt = srcStart[rv.si + 1] - base, fp = rv.u * (cnt - 1), p0 = Math.min(cnt - 2, Math.floor(fp)), f = fp - p0;
      pkPos[n * 2] = spts[(base + p0) * 2] + (spts[(base + p0 + 1) * 2] - spts[(base + p0) * 2]) * f; pkPos[n * 2 + 1] = spts[(base + p0) * 2 + 1] + (spts[(base + p0 + 1) * 2 + 1] - spts[(base + p0) * 2 + 1]) * f;
      pkZ[n] = sz[base + p0];
      const a = (blooming ? 0 : 1) * (hoverActive ? 0.5 : 1) * 0.85 * V.packetGain * Math.min(1, rv.u * 6) * Math.min(1, (1 - rv.u) * 8);
      const dc = D.srcDomain[rv.si] * 3;
      pkCol[n * 4] = (D.domCol[dc] * 0.35 + 0.65) * a; pkCol[n * 4 + 1] = (D.domCol[dc + 1] * 0.35 + 0.65) * a; pkCol[n * 4 + 2] = (D.domCol[dc + 2] * 0.35 + 0.65) * a; pkCol[n * 4 + 3] = a;
      pkSize[n] = 1.3; n++;
    }
  }

  // ---- draw ----
  ensureFbo();
  const proxAmp = cursor.inside && dragging < 0 && !panning ? V.proxAmp : 0;
  const setCommon = (Uu) => {
    gl.uniform3f(Uu.cam, cam.x, cam.y, cam.s); gl.uniform2f(Uu.view, W, H); gl.uniform2f(Uu.cursor, cursor.x, cursor.y); gl.uniform2f(Uu.prox, 150, proxAmp); gl.uniform2f(Uu.tilt, tilt.x, tilt.y);
    gl.uniform4fv(Uu.quiet, quietRects); gl.uniform1f(Uu.qf, QUIET_FEATHER); gl.uniform4fv(Uu.qbox, quietBox);
    if (Uu.zgain) gl.uniform1f(Uu.zgain, V.zGain);
  };
  setCommonU = setCommon;
  const drawFibres = (gain) => {
    gl.useProgram(fibreProg); setCommon(FU);
    // river
    gl.uniform1f(FU.gain, gain * (domActive ? 1 : 1));
    attr(bSPos, FA.pos, 2); attr(bSZ, FA.z, 1); attr(bSCol, FA.col, 4, gl.UNSIGNED_BYTE, true);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, bSIdx); gl.drawElements(gl.LINES, sidx.length, gl.UNSIGNED_INT, 0);
    // links
    gl.uniform1f(FU.gain, gain);
    attr(bLPos, FA.pos, 2); attr(bLZ, FA.z, 1);
    gl.bindBuffer(gl.ARRAY_BUFFER, bLCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, lcol); gl.enableVertexAttribArray(FA.col); gl.vertexAttribPointer(FA.col, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, bLIdx); gl.drawElements(gl.LINES, lidx.length, gl.UNSIGNED_INT, 0);
    if (RL) {
      gl.useProgram(ribbonProg); setCommon(RU); gl.uniform1f(RU.gain, gain); gl.uniform1f(RU.time, t); gl.uniform1f(RU.dpr, GD);
      gl.bindBuffer(gl.ARRAY_BUFFER, bR); const st = 7 * 4;
      gl.enableVertexAttribArray(RA.pos); gl.vertexAttribPointer(RA.pos, 2, gl.FLOAT, false, st, 0);
      gl.enableVertexAttribArray(RA.nrm); gl.vertexAttribPointer(RA.nrm, 2, gl.FLOAT, false, st, 8);
      gl.enableVertexAttribArray(RA.hw); gl.vertexAttribPointer(RA.hw, 1, gl.FLOAT, false, st, 16);
      gl.enableVertexAttribArray(RA.u); gl.vertexAttribPointer(RA.u, 1, gl.FLOAT, false, st, 20);
      gl.enableVertexAttribArray(RA.z); gl.vertexAttribPointer(RA.z, 1, gl.FLOAT, false, st, 24);
      gl.bindBuffer(gl.ARRAY_BUFFER, bRCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, rcol); gl.enableVertexAttribArray(RA.col); gl.vertexAttribPointer(RA.col, 4, gl.UNSIGNED_BYTE, true, 0, 0);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, RV);
    }
  };
  // glow pass: fibres at quarter resolution, blurred, added back
  gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE);
  gl.bindFramebuffer(gl.FRAMEBUFFER, fbo.a.fb); gl.viewport(0, 0, fbo.w, fbo.h);
  gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT);
  drawFibres(1.0);
  gl.disable(gl.BLEND);
  gl.useProgram(quadProg); attr(bQuad, QA, 2); gl.uniform1fv(QU.w, blurW); gl.uniform1i(QU.tex, 0); gl.activeTexture(gl.TEXTURE0);
  gl.bindFramebuffer(gl.FRAMEBUFFER, fbo.b.fb); gl.bindTexture(gl.TEXTURE_2D, fbo.a.tex); gl.uniform2f(QU.dir, 1 / fbo.w, 0); gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  gl.bindFramebuffer(gl.FRAMEBUFFER, fbo.a.fb); gl.bindTexture(gl.TEXTURE_2D, fbo.b.tex); gl.uniform2f(QU.dir, 0, 1 / fbo.h); gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.viewport(0, 0, glc.width, glc.height);
  gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT);
  gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE);
  gl.useProgram(compProg); attr(bQuad, CA, 2); gl.uniform2f(CU.view, W, H); gl.uniform4fv(CU.quiet, quietRects); gl.uniform1f(CU.qf, QUIET_FEATHER); gl.uniform4fv(CU.qbox, quietBox);
  gl.uniform1i(CU.tex, 0); gl.bindTexture(gl.TEXTURE_2D, fbo.a.tex); gl.uniform1f(CU.gain, V.glow * 0.28); gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  // sharp fibres
  drawFibres(0.9);
  // nodes
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl.useProgram(nodeProg); setCommon(NU);
  gl.uniform1f(NU.dpr, GD); gl.uniform1f(NU.homeS, home.s); gl.uniform1f(NU.time, t); gl.uniform1f(NU.breathe, V.breathe);
  gl.bindBuffer(gl.ARRAY_BUFFER, bNPos); gl.bufferSubData(gl.ARRAY_BUFFER, 0, pos); gl.enableVertexAttribArray(NA.pos); gl.vertexAttribPointer(NA.pos, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, bNCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, nodeCol); gl.enableVertexAttribArray(NA.col); gl.vertexAttribPointer(NA.col, 4, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, bNGlow); gl.bufferSubData(gl.ARRAY_BUFFER, 0, glow); gl.enableVertexAttribArray(NA.glow); gl.vertexAttribPointer(NA.glow, 1, gl.FLOAT, false, 0, 0);
  attr(bNSize, NA.size, 1); attr(bNKind, NA.kind, 1); attr(bNLight, NA.light, 1); attr(bNPhase, NA.phase, 1); attr(bNZ, NA.z, 1);
  gl.drawArrays(gl.POINTS, 0, N);
  let nl = 0; for (let i = 0; i < N; i++) if (lit[i] > 0.05 || glow[i] > 0.05 || D.isLight[i]) litIdx[nl++] = i;
  if (nl) { gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, bLitIdx); gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, litIdx.subarray(0, nl), gl.DYNAMIC_DRAW); gl.drawElements(gl.POINTS, nl, gl.UNSIGNED_INT, 0); }
  // packets
  if (PK) {
    gl.bindBuffer(gl.ARRAY_BUFFER, bPkPos); gl.bufferSubData(gl.ARRAY_BUFFER, 0, pkPos); gl.enableVertexAttribArray(NA.pos); gl.vertexAttribPointer(NA.pos, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, bPkCol); gl.bufferSubData(gl.ARRAY_BUFFER, 0, pkCol); gl.enableVertexAttribArray(NA.col); gl.vertexAttribPointer(NA.col, 4, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, bPkSize); gl.bufferSubData(gl.ARRAY_BUFFER, 0, pkSize); gl.enableVertexAttribArray(NA.size); gl.vertexAttribPointer(NA.size, 1, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, bPkZ); gl.bufferSubData(gl.ARRAY_BUFFER, 0, pkZ); gl.enableVertexAttribArray(NA.z); gl.vertexAttribPointer(NA.z, 1, gl.FLOAT, false, 0, 0);
    attr(bPkGlow, NA.glow, 1); attr(bPkZero, NA.kind, 1); attr(bPkZero, NA.light, 1); attr(bPkZero, NA.phase, 1);
    gl.drawArrays(gl.POINTS, 0, PK);
  }
  drawCoreGL(t);

  drawOverlay(now, t, src, hoverActive);
  if (frames % 30 === 0) debugState();
  // PORT: the loop is now cancellable. In the mockup this screen was the whole page and
  // the loop ran forever; in the app it is one of two surfaces, and a WebGL loop drawing
  // 7,500 points behind an opaque conversation view is heat for nothing.
  rafId = running ? requestAnimationFrame(frame) : 0;
}

// ---------------- overlay: the core, domain names, labels, ripples, sparks ----------------
const SERIF = getComputedStyle(ROOT).getPropertyValue('--serif'), SANS = getComputedStyle(ROOT).getPropertyValue('--sans');
// round-11.1: the text drawn INSIDE the picture is text, so it meets the same floors as the chrome.
// INK is the ruled light; HALO is the ruled ground, struck heavy enough that the glyphs sit on the
// ground rather than on whatever the nebula is doing behind them — that halo is what the contrast
// ratio is measured against, and it is why a label stays readable over the brightest part of a
// hover-lit cluster. TINT pulls a cluster's own hue toward the light so the hue still reads as a
// cue without the label failing AA (Revenue's red was 4.16:1 on its own).
const INK = '223,228,238', HALO = '10,15,28';
// A label is either legible or absent. Round 6.4 faded labels toward the chrome and toward the edge
// of the cursor's reach, which produced ink that looked fine and measured 3.26:1 on the rendered
// frame. Here the alpha a label is DRAWN at never goes below LABEL_FLOOR; below the gate it is not
// drawn at all, and the appearing/vanishing is an eased 180 ms transition rather than a pop, so no
// resting state is ever under the floor.
const LABEL_FLOOR = 0.86, LABEL_EASE = 0.18;
let domEase = null, nodeEase = null, lastEase = 0, shownLabels = [], nodeLabelRects = [];
function easeTo(store, k, target, dt) {
  const cur = store[k] || 0;
  const next = cur + (target - cur) * Math.min(1, dt / LABEL_EASE);
  store[k] = Math.abs(next - target) < 0.004 ? target : next;
  return store[k];
}
const tintCache = {};
function tint(css) {
  if (tintCache[css]) return tintCache[css];
  const m = css.match(/[\d.]+/g).map(Number);
  const k = 0.6;
  return (tintCache[css] = [Math.round(m[0] * (1 - k) + 223 * k), Math.round(m[1] * (1 - k) + 228 * k), Math.round(m[2] * (1 - k) + 238 * k)].join(','));
}
function shortLabel(i) { return TYPE_WORD[nodes[i].type]; }
function fullLabel(i) { return `${TYPE_WORD[nodes[i].type]} · ${domains[D.nodeDomain[i]].label}`; }
function drawOverlay(now, t, src, hoverActive) {
  ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  ctx.clearRect(0, 0, W, H);
  if (!domEase) { domEase = new Float32Array(domains.length); nodeEase = new Float32Array(N); }
  const edt = lastEase ? Math.min(0.05, (now - lastEase) / 1000) : LABEL_EASE; lastEase = now;

  // domain names at their live centroids
  domCentroid.fill(0);
  for (let i = 0; i < N; i++) { if (nAlpha[i] < 0.5) continue; const d = D.nodeDomain[i] * 4; domCentroid[d] += pos[i * 2]; domCentroid[d + 1] += pos[i * 2 + 1]; domCentroid[d + 2]++; domCentroid[d + 3] += D.z[i]; }
  ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
  domLabelRects = [];
  for (let d = 0; d < domains.length; d++) {
    const c = domCentroid[d * 4 + 2]; if (c < 8) continue;
    const [sx, sy] = toScreen(domCentroid[d * 4] / c, domCentroid[d * 4 + 1] / c, domCentroid[d * 4 + 3] / c);
    const near = cursor.inside ? clamp(1 - Math.hypot(sx - cursor.x, sy - cursor.y) / 210, 0, 1) : 0;
    const q = quietAt(sx, sy);
    // APP: the font and the measure move ABOVE the ease so the temporary line can be tested
    // against this label's REAL box — the same box pushed to `domLabelRects` below — rather
    // than against its middle with a guessed pad. Feeding the zone into the ease target, and
    // not `continue`, is what makes a blocked label FADE the way a hushed one does.
    const size = 16 + 2 * near + (domHover === d ? 1 : 0);
    ctx.font = `${size}px ${SERIF}`;
    ctx.letterSpacing = '0.22em';
    const label = domains[d].label.toUpperCase();
    const half = ctx.measureText(label).width / 2 + 8;
    // APP: the WHOLE BOX is tested now, against every rect a caption may not touch, and a
    // label that needs only a small sideways move gets one instead of being rubbed out by
    // `clearQuiet()` a few lines later. `shift === null` means no move inside the budget
    // clears it, which fades the label exactly as the ticker zone already did — one fewer
    // name on screen, never half a name. See `clearingShift` for the measured numbers.
    // ...AND AGAINST THE NAMES ALREADY PLACED THIS FRAME (audit-7 row 13). `domLabelRects` is
    // filled as this loop runs, so passing it in tests each name against every name before it.
    // The order is the dataset's own domain order, which is fixed, so which name holds its place
    // and which one moves is deterministic rather than frame-dependent.
    const nudge = clearingNudge(sx - half, sy - 14, sx + half, sy + 28, domLabelRects);
    const blocked = nudge === null;
    // Everything the label draws hangs off `lx`/`ly`, including the count beneath it and the box
    // exported to `domLabelRects`, so the moved label and the rect that claims its space cannot
    // come apart. `ly` is new with the lift (`clearingNudge`) and is 0 for every move against
    // the chrome, which stays horizontal-only for the reason written above `clearingShift`.
    const lx = sx + (nudge ? nudge.dx : 0);
    const ly = sy + (nudge ? nudge.dy : 0);
    let a = easeTo(domEase, d, q < 0.18 || blocked ? 0 : (0.78 + 0.20 * near * near) * Math.min(1, LABEL_FLOOR + (q - 0.18) * 0.9), edt);
    if (hoverActive) a *= 0.86;
    if (domHover >= 0) a = blocked ? 0 : domHover === d ? 1 : 0.86;
    a *= clamp((t - 2.5) / 1.5, 0, 1);
    if (a < 0.05) continue;
    ctx.save(); ctx.translate(lx, ly);
    ctx.lineJoin = 'round'; ctx.lineWidth = 5; ctx.strokeStyle = `rgba(${HALO},${0.98 * a})`;
    ctx.strokeText(label, 0, 0);
    ctx.fillStyle = `rgba(${INK},${a})`;
    ctx.fillText(label, 0, 0);
    ctx.letterSpacing = '0.08em';
    ctx.font = `14px ${SANS}`;
    ctx.lineWidth = 4.5; ctx.strokeText(fmt(c), 0, 19);
    ctx.fillStyle = `rgba(${INK},${a * 0.88})`;
    ctx.fillText(fmt(c), 0, 19);
    ctx.restore();
    // APP: `half` is `measureText(label).width / 2 + 8`, taken above with the same font and the
    // same letter-spacing — the identical box, measured once instead of twice.
    //
    // `pad` is the 8 inside that `half`: the margin between the glyphs and the edge of the box
    // this label claims. It is exported because the difference matters to anyone measuring a
    // clipped label — an overlap of 6px eats margin and an overlap of 22px eats the final E,
    // and a count that cannot tell those apart cannot say which one happened. The engine keeps
    // the WHOLE box clear, margin included, because the 2.5px halo `strokeText` lays down under
    // the ink is what buys the label its contrast ratio.
    if (a > 0.15) domLabelRects.push({ d, x0: lx - half, x1: lx + half, y0: ly - 14, y1: ly + 28, pad: 8 });
  }
  ctx.letterSpacing = '0px';
  // node labels: which
  const labels = [];
  if (src >= 0) {
    labels.push([src, 1, true]);
    const nb = []; for (let k = D.adjStart[src]; k < D.adjStart[src + 1]; k++) nb.push(D.adj[k]);
    nb.sort((a, b) => D.degree[b] - D.degree[a]).slice(0, 12).forEach(j => labels.push([j, 0.9, false, false]));
  }
  if (cursor.inside && src < 0 && domHover < 0 && !blooming) {
    const cand = [];
    for (let i = 0; i < N; i++) { if (D.degree[i] < 14) continue; const [sx, sy] = nodeScreen(i); const d = Math.hypot(sx - cursor.x, sy - cursor.y); if (d < 130) cand.push([i, d]); }
    cand.sort((a, b) => a[1] - b[1]).slice(0, 5).forEach(([i, d]) => labels.push([i, LABEL_FLOOR + (1 - LABEL_FLOOR) * (1 - d / 130), false, true]));
  }
  ctx.textAlign = 'center'; ctx.textBaseline = 'top';
  // APP: `placed` is now ONLY the node labels this pass actually drew, and everything a node
  // label must keep off is `taken` beside it.
  //
  // It used to be one array with the ticker zone seeded at index 0 and `.slice(tickerZone ? 1
  // : 0)` at the bottom to take the seed back out — a correct trick that does not survive a
  // second seed, and this pass needs three more: the erase rects, the first-run banner, and
  // the DOMAIN NAMES. That last one is candidate-.2 defect #4's third symptom: over a 12x11
  // cursor sweep at 1024x700, 106 node labels were drawn across a domain name, because this
  // list carried no domain rect to collide with. `domLabelRects` is built a few lines above,
  // in this same pass, so it is this frame's truth rather than the last frame's.
  const seen = new Set(), placed = [], wanted = new Set(labels.map(l => l[0]));
  const taken = domLabelRects.map(r => [r.x0, r.y0, r.x1, r.y1]);
  for (let k = 0; k < shownLabels.length; k++) { const i = shownLabels[k]; if (!wanted.has(i)) easeTo(nodeEase, i, 0, edt); }
  shownLabels = [];
  for (const [i, aT, primary, withTerritory] of labels) {
    if (seen.has(i)) continue; seen.add(i);
    const a = easeTo(nodeEase, i, aT, edt);
    if (a < 0.05) continue;
    shownLabels.push(i);
    const [sx, sy] = nodeScreen(i);
    if (sx < -50 || sy < -20 || sx > W + 50 || sy > H + 20) continue;
    if (quietAt(sx, sy) < 0.6) continue;
    const r = screenRadius(i);
    const fs = primary ? 16 : 14;
    ctx.font = `${primary ? 500 : 400} ${fs}px ${SANS}`;
    const text = primary || withTerritory ? fullLabel(i) : shortLabel(i);
    const y = sy + r + 5;
    const tw = ctx.measureText(text).width + 6, x0 = sx - tw / 2, x1 = sx + tw / 2, y0 = y - 2, y1 = y + fs + 3;
    if (placed.some(b => x0 < b[2] && x1 > b[0] && y0 < b[3] && y1 > b[1])) continue;
    if (taken.some(b => x0 < b[2] && x1 > b[0] && y0 < b[3] && y1 > b[1])) continue;
    // The erase rects, the banner, the ticker zone and the viewport, in one test. A node
    // label is summoned by the cursor and leaves with it, so there is no nudge here and no
    // need of one: skipping is the behavior this loop already had for a collision, and one
    // chip fewer under the pointer is not a name the screen has lost.
    if (blockedBox(x0, y0, x1, y1)) continue;
    placed.push([x0, y0, x1, y1]);
    ctx.lineWidth = 4.5; ctx.strokeStyle = `rgba(${HALO},${0.98 * a})`; ctx.lineJoin = 'round';
    ctx.strokeText(text, sx, y);
    ctx.fillStyle = primary ? `rgba(${tint(domCss[D.nodeDomain[i]])},${a})` : `rgba(${INK},${a})`;
    ctx.fillText(text, sx, y);
  }
  // APP: every entry of `placed` is a node label that was drawn, and nothing else is in there
  // any more — the places a label MAY NOT GO live in `taken` and `blockRects`. Exporting a
  // no-go rect as a label would tell every reader of this list that the picture had written a
  // caption over the temporary line, which is the exact condition the zone exists to prevent.
  // `pad: 3` is the 6 in `measureText(text).width + 6`, split across the two sides — the same
  // glyphs-to-box margin `pad: 8` records for a domain name.
  nodeLabelRects = placed.map(([x0, y0, x1, y1]) => ({ x0, y0, x1, y1, pad: 3 }));
  // ripples
  for (let k = ripples.length - 1; k >= 0; k--) {
    const rp = ripples[k]; const p = (now - rp.t0) / 1100; if (p >= 1) { ripples.splice(k, 1); continue; }
    const [sx, sy] = toScreen(rp.x, rp.y, 0); const rr = (10 + 160 * Math.pow(p, 0.6)) * Math.sqrt(cam.s / home.s);
    ctx.beginPath(); ctx.arc(sx, sy, rr, 0, Math.PI * 2); ctx.strokeStyle = rp.hue.replace('rgb', 'rgba').replace(')', `,${0.5 * (1 - p)})`); ctx.lineWidth = 1.5 * (1 - p) + 0.4; ctx.stroke();
  }
  // sparks: sources travelling into the loro
  for (let k = sparks.length - 1; k >= 0; k--) {
    const sp = sparks[k]; const p = (now - sp.t0) / sp.dur;
    if (p >= 1) { landSpark(sp, now); sparks.splice(k, 1); continue; }
    const [tx, ty] = nodeScreen(sp.i);
    const [fx, fy] = sp.from; const cx = (fx + tx) / 2 + (fy - ty) * 0.35, cy = (fy + ty) / 2 + (tx - fx) * 0.35;
    const e = p * p * (3 - 2 * p);
    const at = (u) => [(1 - u) * (1 - u) * fx + 2 * (1 - u) * u * cx + u * u * tx, (1 - u) * (1 - u) * fy + 2 * (1 - u) * u * cy + u * u * ty];
    ctx.beginPath(); for (let j = 0; j <= 16; j++) { const u = Math.max(0, e - 0.12) + (e - Math.max(0, e - 0.12)) * j / 16; const [x, y] = at(u); j ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
    ctx.strokeStyle = sp.col.replace('rgb', 'rgba').replace(')', ',0.55)'); ctx.lineWidth = 1.2; ctx.stroke();
    const [x, y] = at(e); ctx.beginPath(); ctx.arc(x, y, 2.6, 0, Math.PI * 2); ctx.fillStyle = sp.col; ctx.fill();
    ctx.beginPath(); ctx.arc(x, y, 7, 0, Math.PI * 2); ctx.fillStyle = sp.col.replace('rgb', 'rgba').replace(')', ',0.18)'); ctx.fill();
  }
  clearQuiet();
}

// verification hook: a hidden DOM node carries live state so a headless run can assert on it
const hubs = Array.from({ length: N }, (_, i) => i).sort((a, b) => D.degree[b] - D.degree[a]).slice(0, 6);
function debugState() { const el = $('#home-debug'); if (!el) return; el.textContent = JSON.stringify(snapshot()); }
function snapshot() {
  let visible = 0; for (let i = 0; i < N; i++) { const [sx, sy] = nodeScreen(i); if (sx >= 0 && sy >= 0 && sx <= W && sy <= H) visible++; }
  return {
    variant: V.name, N, L, S, visible, frames, blooming, active: activeList.length, waves: waves.length, motionFrames, hover, selected, domHover, domSel,
    cam: { x: +cam.x.toFixed(1), y: +cam.y.toFixed(1), s: +cam.s.toFixed(3) }, lights: D.nLights, packets: packets.length, river: river.length,
    hubs: hubs.map(i => { const [sx, sy] = nodeScreen(i); return { i, id: nodes[i].id, deg: D.degree[i], sx: Math.round(sx), sy: Math.round(sy), fade: +fade[i].toFixed(2), glow: +glow[i].toFixed(2) }; }),
    quiet: { x0: Math.round(quietRect.x0), y0: Math.round(quietRect.y0), x1: Math.round(quietRect.x1), y1: Math.round(quietRect.y1) }, landed: { ...landed }, shown: { ...shown },
  };
}
// PORT: pause/resume. `pause()` stops the loop and leaves EVERY byte of state alive — the
// typed arrays, the GL buffers, the camera, the bloom — so `resume()` is a single rAF and
// not a rebuild. `lastNow` is re-based on resume: without it the first frame back would see
// a dt of however long the CEO spent in the app UI, and the springs would jump.
function pause() {
  if (!running) return;
  running = false;
  if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
}
function resume() {
  if (running) return;
  running = true;
  lastNow = performance.now();
  quietDirty = true;
  resize();
  rafId = requestAnimationFrame(frame);
}

$('#home-loading').classList.add('gone');
rafId = requestAnimationFrame(frame);
// READ-ONLY, FOR `tests/home.js` ONLY. Calling `ingest()` from outside (the line above) does
// NOT reset this field's own idle-learning clock (`lastIngest`/`nextIngestGap`, line 593/838) —
// only the scheduler's own auto-fire does that — so a driven line and an auto-fired one can
// land on top of each other, and there was no way for a caller to know how much headroom the
// auto clock had left before that could happen. Exposed as the raw countdown so a test can wait
// for a safe window rather than guess one: `esc-20260917T113423Z-bdc63632`.
function autoIngestDueInMs() { return Math.max(0, (lastIngest + nextIngestGap) - performance.now()); }
// `eraseRects` is what `clearQuiet()` actually paints out, read straight off `quietRects`
// rather than described a second time: the fit suite asserts that no label the picture drew
// overlaps a rectangle the picture then rubs out, and a test carrying its own copy of that
// geometry would be a test of the copy. `labelBlockers` is the whole no-caption list, the
// banner included.
window.__loro = { N, L, S, V, snapshot, pause, resume, get running() { return running; }, quietRect, quietAt,
  get eraseRects() { const out = []; for (let g = 0; g < QUIET_N; g++) { const o = g * 4; if (quietRects[o + 2] < 0) continue; out.push({ x0: quietRects[o], y0: quietRects[o + 1], x1: quietRects[o + 2], y1: quietRects[o + 3] }); } return out; },
  get labelBlockers() { return blockRects.map(([x0, y0, x1, y1]) => ({ x0, y0, x1, y1 })); },
  get domLabelRects() { return domLabelRects; }, get nodeLabelRects() { return nodeLabelRects; }, landed, get pos() { return pos; }, get hover() { return hover; }, get selected() { return selected; }, get cam() { return cam; }, fade, lit, glow, releaseAll, ingest: () => ingest(performance.now()), get autoIngestDueInMs() { return autoIngestDueInMs(); }, get blooming() { return blooming; }, get motionFrames() { return motionFrames; }, get frames() { return frames; }, pushWave, get activeList() { return activeList; } };
})().catch((e) => {
  // THE HONEST FAILURE NEEDS SOMEWHERE TO LAND. Every throw above is inside this async
  // IIFE — `WebGL unavailable` at the context, a shader that will not compile, a program
  // that will not link — and a throw inside an async IIFE is a rejected promise nobody
  // holds. `home.js` knew that and worked around it by polling for `window.__loro` with an
  // eight-second deadline, so the CEO watched "waking loro…" for eight seconds over a
  // failure that was known in the first frame, and the console carried an unhandled
  // rejection either way.
  //
  // MEASURED, NOT SUPPOSED. `ui-suite-ci` run 33879245990 on a GitHub `macos-latest` runner:
  //
  //     console: WebGL: context lost.
  //     Unhandled Promise Rejection: Error: null
  //     attribute vec2 a_pos; attribute float a_z; …
  //
  // That is `shader()` above throwing `getShaderInfoLog(s) + '\n' + src` with a null info
  // log, because the context had gone. The picture's own recovery was correct and it was
  // eight seconds late and shouting on the way.
  //
  // So the failure is recorded where `home.js`'s `settled()` looks, and it degrades on the
  // spot. The throw is not swallowed — `__loroFailed` IS the report, and `home.js` renders
  // it as the sentence the CEO reads.
  window.__loroFailed = (e && e.message) || String(e) || "the picture could not start";
});
