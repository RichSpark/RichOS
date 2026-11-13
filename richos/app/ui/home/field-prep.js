// RichOS round 6.4 — prep: the loro laid into the brain.
//
// Input:  window.REF   — the 1,983 strands (28 points each), their colours and the 70 lights traced
//                        from the reference picture (round-5.2/v4, unchanged bytes, ../shared/brain-ref.js)
//         window.MATURE_LORO — the committed dataset (7,500 objects, 12,817 links, 4,800 sources)
// Output: typed arrays for the engine. Everything here is deterministic (seeded mulberry32).
//
// The idea: the picture's lobe strands are cut into twelve angular sectors around the core, one per
// domain, sized by how much of the loro lives there; every cluster gets a contiguous bundle of strands
// inside its domain; every object is placed ON a strand of its cluster (hubs near the root, leaves
// along the length). The links between them, drawn as flow-following curves, are then the fibres the
// brain is made of. The picture's stem — the river — is built from the 4,800 sources, banded by domain.
'use strict';

const TYPES = ['memory', 'conversation', 'decision', 'commitment', 'lesson', 'person', 'organization', 'project', 'initiative', 'theme'];
const TYPE_WORD = { memory: 'Memory', conversation: 'Conversation', decision: 'Decision', commitment: 'Commitment', lesson: 'Lesson', person: 'Person', organization: 'Organization', project: 'Project', initiative: 'Initiative', theme: 'Theme' };
const CORE = { x: 530, y: 470 };            // the picture's core, in brain units (0..1000)
const BRAIN = 1000;

function mulberry32(a) {
  return function () {
    a |= 0; a = a + 0x6D2B79F5 | 0;
    let t = Math.imul(a ^ a >>> 15, 1 | a);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}
function clamp(v, a, b) { return v < a ? a : v > b ? b : v; }
function b64(s) { const b = atob(s), u = new Uint8Array(b.length); for (let i = 0; i < b.length; i++) u[i] = b.charCodeAt(i); return u; }
// the picture's colours, boosted the way round-5.2/v4 boosts them
function vivid(c, boost) {
  const l = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
  let r = l + (c[0] - l) * boost, g = l + (c[1] - l) * boost, b = l + (c[2] - l) * boost;
  r = Math.max(0.02, r); g = Math.max(0.02, g); b = Math.max(0.02, b);
  const m = Math.max(r, g, b); return [r / m, g / m, b / m];
}
function angleOf(x, y) { let a = Math.atan2(-(y - CORE.y), x - CORE.x) * 180 / Math.PI; if (a < -90) a += 360; return a; }
function regionOf(x, y) { if (y > 690 || (y > 600 && x > 620 && x < 720)) return 'stem'; const a = angleOf(x, y); return a < -44 ? 'stem' : a < 66 ? 'occipital' : a < 126 ? 'parietal' : a < 204 ? 'frontal' : 'temporal'; }

// ---------- strands ----------
// ROUND 72: NO SHARP ANGLES. A strand is 28 traced points (plus 6 where it is carried off the
// picture) and was drawn as the straight lines between them, so every place the tracing turned hard
// showed as a corner: a stair step, a zigzag, a spike, the bend where the river's extension starts.
// Two steps, both keeping the strand's two ends where they are:
//   1. relax - a point whose turn is sharper than SHARP_FROM degrees is pulled toward the middle of
//      its two neighbors, fully from SHARP_FULL degrees on, until no turn is sharper than SHARP_STOP
//      (or RELAX_MAX passes). A point on a gentle bend is not moved at all, so the picture's sweeps
//      and curls stay where the tracing put them.
//   2. curve - a centripetal Catmull-Rom spline through the relaxed points, SUB pieces per span,
//      then only the points the curve needs are kept: one wherever the line has turned KEEP_DEG
//      since the last kept point, so a straight run costs two points and a curl gets them all.
//      Each kept point carries `U`, its place along the TRACED strand (0 at the core end, 1 at the
//      far end, by traced point as before), so the colors along a strand do not shift.
const SHARP_FROM = 10, SHARP_FULL = 28, SHARP_STOP = 16, RELAX_MAX = 80, SUB = 6, KEEP_DEG = 2.5;
// The river's extension leaves the strand in the strand's own direction and eases into its flatter
// run (the app's constant 0.6, reached at the far end) instead of turning to it at the first point.
function extEase(u) { return 1 - 0.4 * (1 - (1 - u) * (1 - u)); }
function turnDeg(a, b, c) {
  const ux = b[0] - a[0], uy = b[1] - a[1], vx = c[0] - b[0], vy = c[1] - b[1];
  const lu = Math.hypot(ux, uy), lv = Math.hypot(vx, vy);
  if (lu < 1e-6 || lv < 1e-6) return 180;
  return Math.acos(clamp((ux * vx + uy * vy) / (lu * lv), -1, 1)) * 180 / Math.PI;
}
// Points that sit on top of each other become one point: the tracing has a few, relaxing a spike
// makes more, and a spline through them would knot. The two ends always stay.
function onePointEach(p) {
  const near = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]) < 0.5, out = [p[0]];
  for (let k = 1; k < p.length - 1; k++) if (!near(p[k], out[out.length - 1])) out.push(p[k]);
  const end = p[p.length - 1]; if (out.length > 1 && near(end, out[out.length - 1])) out.pop();
  out.push(end); return out;
}
function smoothStrand(pts) {
  const n0 = pts.length;
  // `u` is each point's place among the traced ones; it travels with the point from here on
  let p = onePointEach(pts.map((q, k) => { const c = [q[0], q[1]]; c.u = k / Math.max(1, n0 - 1); return c; }));
  for (let pass = 0; pass < RELAX_MAX && p.length > 2; pass++) {
    let worst = 0; const q = p.map(v => { const c = [v[0], v[1]]; c.u = v.u; return c; });
    for (let k = 1; k < p.length - 1; k++) {
      const t = turnDeg(p[k - 1], p[k], p[k + 1]); if (t > worst) worst = t;
      let w = clamp((t - SHARP_FROM) / (SHARP_FULL - SHARP_FROM), 0, 1); w = w * w * (3 - 2 * w);
      if (w > 0) { q[k][0] += ((p[k - 1][0] + p[k + 1][0]) / 2 - p[k][0]) * 0.5 * w; q[k][1] += ((p[k - 1][1] + p[k + 1][1]) / 2 - p[k][1]) * 0.5 * w; }
    }
    if (worst <= SHARP_STOP) break;
    p = onePointEach(q);
  }
  const n = p.length, us = p.map(q => q.u); if (n < 3) return { pts: p, U: us };
  // centripetal Catmull-Rom (Barry-Goldman): no overshoot and no loop where spans differ in length
  const out = [], knot = (a, b) => Math.sqrt(Math.hypot(b[0] - a[0], b[1] - a[1])) || 1e-3;
  const mix = (a, b, ta, tb, t) => { const f = (t - ta) / (tb - ta); return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]; };
  for (let k = 0; k < n - 1; k++) {
    const p1 = p[k], p2 = p[k + 1];
    const p0 = k > 0 ? p[k - 1] : [2 * p1[0] - p2[0], 2 * p1[1] - p2[1]];
    const p3 = k < n - 2 ? p[k + 2] : [2 * p2[0] - p1[0], 2 * p2[1] - p1[1]];
    const t0 = 0, t1 = t0 + knot(p0, p1), t2 = t1 + knot(p1, p2), t3 = t2 + knot(p2, p3);
    for (let j = 0; j < SUB; j++) {
      const t = t1 + (t2 - t1) * j / SUB;
      const a1 = mix(p0, p1, t0, t1, t), a2 = mix(p1, p2, t1, t2, t), a3 = mix(p2, p3, t2, t3, t);
      const b1 = mix(a1, a2, t0, t2, t), b2 = mix(a2, a3, t1, t3, t);
      const c = mix(b1, b2, t1, t2, t); c.u = us[k] + (us[k + 1] - us[k]) * j / SUB; out.push(c);
    }
  }
  const end = [p[n - 1][0], p[n - 1][1]]; end.u = 1; out.push(end);
  const kept = [out[0]]; let turned = 0;
  for (let k = 1; k < out.length - 1; k++) {
    turned += turnDeg(out[k - 1], out[k], out[k + 1]);
    if (turned >= KEEP_DEG) { kept.push(out[k]); turned = 0; }
  }
  kept.push(end);
  return { pts: kept, U: kept.map(c => c.u) };
}
function decodeStrands(REF) {
  const NP = REF.nPts, XY = new Uint16Array(b64(REF.strands).buffer), SC = b64(REF.strandColors);
  const strands = [];
  for (let i = 0; i < REF.nStrands; i++) {
    let pts = []; for (let k = 0; k < NP; k++) pts.push([XY[(i * NP + k) * 2] / 65535 * BRAIN, XY[(i * NP + k) * 2 + 1] / 65535 * BRAIN]);
    let cols = []; for (let k = 0; k < 4; k++) cols.push(vivid([SC[(i * 4 + k) * 3] / 255, SC[(i * 4 + k) * 3 + 1] / 255, SC[(i * 4 + k) * 3 + 2] / 255], 1.4));
    const d0 = Math.hypot(pts[0][0] - CORE.x, pts[0][1] - CORE.y), d1 = Math.hypot(pts[NP - 1][0] - CORE.x, pts[NP - 1][1] - CORE.y);
    if (d1 < d0) { pts.reverse(); cols.reverse(); }               // u = 0 is the core end
    // the river runs off the picture: extend the strands that leave at the bottom corners (as v4 does)
    const e = pts[NP - 1], e2 = pts[NP - 4];
    if (e[1] > 550 && (e[0] < 35 || e[0] > 965)) { let dx = e[0] - e2[0], dy = e[1] - e2[1]; const n = Math.hypot(dx, dy) || 1; dx /= n; dy /= n; const tx = e[0] < 500 ? -500 : 1500; const len = Math.abs(tx - e[0]) / Math.max(0.2, Math.abs(dx)); for (let k = 1; k <= 6; k++) { const u = k / 6; pts.push([e[0] + dx * len * u, e[1] + dy * len * u * extEase(u) + 20 * u * u]); } }
    const s0 = pts[0];
    if (s0[1] > 550 && (s0[0] < 35 || s0[0] > 965)) { let dx = s0[0] - pts[3][0], dy = s0[1] - pts[3][1]; const n = Math.hypot(dx, dy) || 1; dx /= n; dy /= n; const tx = s0[0] < 500 ? -500 : 1500; const len = Math.abs(tx - s0[0]) / Math.max(0.2, Math.abs(dx)); const ext = []; for (let k = 6; k >= 1; k--) { const u = k / 6; ext.push([s0[0] + dx * len * u, s0[1] + dy * len * u * extEase(u) + 20 * u * u]); } pts = ext.concat(pts); }
    const sm = smoothStrand(pts); pts = sm.pts;                    // ROUND 72: no sharp angles
    const n = pts.length, P = new Float32Array(n * 2), cum = new Float32Array(n);
    for (let k = 0; k < n; k++) { P[k * 2] = pts[k][0]; P[k * 2 + 1] = pts[k][1]; if (k) cum[k] = cum[k - 1] + Math.hypot(pts[k][0] - pts[k - 1][0], pts[k][1] - pts[k - 1][1]); }
    let mk = 0; for (let k = 1; k < n; k++) if (Math.abs(sm.U[k] - 0.5) < Math.abs(sm.U[mk] - 0.5)) mk = k; // ROUND 72: the traced middle, as before
    const mid = pts[mk];
    const lum = (SC[i * 12] + SC[i * 12 + 1] + SC[i * 12 + 2] + SC[i * 12 + 3] + SC[i * 12 + 4] + SC[i * 12 + 5]) / (6 * 255);
    strands.push({ i, P, n, U: Float32Array.from(sm.U), cum, len: cum[n - 1], cols, region: regionOf(mid[0], mid[1]), angle: angleOf(mid[0], mid[1]), d0: Math.hypot(pts[0][0] - CORE.x, pts[0][1] - CORE.y), lum, mid });
  }
  return strands;
}
// point + unit tangent at arc-length fraction u of a strand
function strandAt(s, u, out) {
  const target = clamp(u, 0, 1) * s.len, cum = s.cum, n = s.n;
  let lo = 0, hi = n - 1;
  while (hi - lo > 1) { const m = (lo + hi) >> 1; if (cum[m] <= target) lo = m; else hi = m; }
  const seg = cum[hi] - cum[lo] || 1, f = (target - cum[lo]) / seg, P = s.P;
  const x0 = P[lo * 2], y0 = P[lo * 2 + 1], x1 = P[hi * 2], y1 = P[hi * 2 + 1];
  out[0] = x0 + (x1 - x0) * f; out[1] = y0 + (y1 - y0) * f;
  const dx = x1 - x0, dy = y1 - y0, l = Math.hypot(dx, dy) || 1; out[2] = dx / l; out[3] = dy / l;
  return out;
}
function strandColor(s, u, out, o) {
  const t = clamp(u, 0, 1) * 3, k = Math.min(2, Math.floor(t)), f = t - k, a = s.cols[k], b = s.cols[k + 1];
  out[o] = a[0] + (b[0] - a[0]) * f; out[o + 1] = a[1] + (b[1] - a[1]) * f; out[o + 2] = a[2] + (b[2] - a[2]) * f;
}

// Domain order around the brain, counter-clockwise from the lower right: strongly coupled pairs sit
// next to each other (Customers–Product, Operations–Talent), the biggest domain takes the crown.
const ARC_ORDER = ['operations', 'talent', 'infrastructure', 'product', 'customers', 'revenue', 'market', 'brand', 'partnerships', 'strategy', 'capital', 'legal-risk'];

// APP: THE ARC IS NOW READ OFF THE DATASET, and for the synthetic dataset it is the same list.
//
// `ARC_ORDER` is the twelve domain ids the round's own generator emits. Until the app could draw
// a CUSTOMER'S loro (`home_field_data`, 2026-09-06) that was the only dataset there was, and
// `domainIndex.get('brand')` was never `undefined`. A real corpus's domains are his COMPANY LANES
// — `ceo`, `richos`, whatever he has — so every one of those lookups misses, and the very next
// line, `domains[undefined].nodeCount`, throws before a single strand is allotted.
//
// So the order is derived: the round's preferred order for the ids this dataset actually has,
// then anything it has that the round never heard of, in the dataset's own order.
//
// IT IS THE SAME LIST FOR THE ROUND'S DATA, and that is arithmetic rather than a hope. The
// synthetic dataset carries exactly the twelve ids in `ARC_ORDER` and no others, so the filter
// drops nothing and preserves order, and the concatenation appends nothing:
// `arcOrderOf(...)` === `ARC_ORDER`, element for element. The composition the CEO approved is
// therefore byte-identical over the dataset he approved it on.
function arcOrderOf(domains, domainIndex) {
  const known = ARC_ORDER.filter((id) => domainIndex.has(id));
  const rest = domains.map((d) => d.id).filter((id) => ARC_ORDER.indexOf(id) < 0);
  return known.concat(rest);
}

function prepare(loro, REF, opts) {
  opts = opts || {};
  const rnd = mulberry32(20260826);
  const strands = decodeStrands(REF);
  const nodes = loro.nodes, links = loro.links, domains = loro.domains;
  const N = nodes.length, L = links.length;
  const index = new Map(); nodes.forEach((n, i) => index.set(n.id, i));
  const domainIndex = new Map(); domains.forEach((d, di) => domainIndex.set(d.id, di));
  const clusterList = []; const clusterIndex = new Map();
  domains.forEach((d, di) => d.clusters.forEach(c => { clusterIndex.set(c.id, clusterList.length); clusterList.push({ id: c.id, domain: di, n: c.nodeCount, strands: [] }); }));
  const nodeCluster = new Int16Array(N), nodeDomain = new Uint8Array(N);
  for (let i = 0; i < N; i++) { nodeCluster[i] = clusterIndex.get(nodes[i].cluster); nodeDomain[i] = domainIndex.get(nodes[i].domain); }

  // ---- territories: lobe strands by angle → domains → clusters (shares by arc length) ----
  const lobe = strands.filter(s => s.region !== 'stem').sort((a, b) => a.angle - b.angle);
  const stem = strands.filter(s => s.region === 'stem');
  const lobeLen = lobe.reduce((a, s) => a + s.len, 0);
  const ARC = arcOrderOf(domains, domainIndex);
  const domStrands = domains.map(() => []);
  {
    const order = ARC.map(id => domainIndex.get(id));
    let k = 0, acc = 0;
    // APP: THE SHARE IS OF THE NODES THAT OWN A TERRITORY, not of every node on the page. For every
    // dataset but the ghost's the two are the same number (a domain's `nodeCount` is its own nodes,
    // `home_field.rs::a_domains_node_count_is_its_own_nodes`). The ghost's own lanes are `nodeCount: 0`
    // because his objects stand in the example's clusters (see `hosted` below), and dividing by every
    // node would shrink the example's twelve arcs and leave a bare wedge at the end of the circle.
    const shareN = domains.reduce((a, d) => a + d.nodeCount, 0) || N;
    order.forEach((di, oi) => {
      const share = domains[di].nodeCount / shareN;
      const target = oi === order.length - 1 ? Infinity : acc + share * lobeLen;
      while (k < lobe.length && (acc < target || domStrands[di].length === 0)) { domStrands[di].push(lobe[k]); acc += lobe[k].len; k++; }
    });
    while (k < lobe.length) { domStrands[order[order.length - 1]].push(lobe[k++]); }
  }
  domains.forEach((d, di) => {
    const ss = domStrands[di], total = ss.reduce((a, s) => a + s.len, 0);
    const cls = d.clusters.map(c => clusterList[clusterIndex.get(c.id)]);
    let k = 0, acc = 0;
    cls.forEach((c, ci) => {
      const target = ci === cls.length - 1 ? Infinity : acc + (c.n / d.nodeCount) * total;
      while (k < ss.length && (acc < target || c.strands.length === 0)) { c.strands.push(ss[k]); acc += ss[k].len; k++; }
    });
    while (k < ss.length) cls[cls.length - 1].strands.push(ss[k++]);
    // APP: A CLUSTER WITH NO STRAND LEFT TO STAND ON. The `|| c.strands.length === 0` above
    // guarantees each cluster one strand only WHILE STRANDS REMAIN — a domain with more clusters
    // than the arc gave it strands leaves the last ones empty, and `pickStrand([])` returns
    // `undefined`, which throws at `s.len` two lines later. It cannot happen on the round's own
    // data (34 clusters over 1,983 strands, and today's picture draws), and it is one bad share
    // away on a corpus whose lanes and groups nobody chose. Falling back to the domain's own
    // strands, then to the whole lobe, is a cluster drawn in the wrong place rather than a
    // picture that does not draw.
    cls.forEach((c) => { if (!c.strands.length) c.strands = ss.length ? ss.slice() : lobe.slice(); });
  });

  // ---- node placement: on a strand of its cluster ----
  const home = new Float32Array(N * 2), tan = new Float32Array(N * 2), nStrand = new Int32Array(N), nU = new Float32Array(N), nPerp = new Float32Array(N);
  const col = new Float32Array(N * 3), z = new Float32Array(N);
  const radius = new Float32Array(N), isLight = new Uint8Array(N), lightR = new Float32Array(N), lightPhase = new Float32Array(N);
  const typeCode = new Uint8Array(N), degree = new Int16Array(N), flags = new Uint8Array(N);
  const significance = new Float32Array(N), activity = new Float32Array(N), recency = new Float32Array(N), createdDay = new Int16Array(N);
  const delay = new Float32Array(N), dur = new Float32Array(N);
  const tmp = new Float32Array(4);
  const byCluster = clusterList.map(() => []);
  for (let i = 0; i < N; i++) byCluster[nodeCluster[i]].push(i);
  // APP: `pr` is the stream placement draws from. It is `rnd` for every dataset whose objects carry no
  // `layoutSeed` (the demonstration, byte for byte as approved). Objects read from his repositories
  // (`home_repos.rs`) carry their cluster's seed, and each cluster is placed from its own stream, so a
  // commit that adds objects to one folder does not move every object drawn after it.
  let pr = rnd;
  const pickStrand = (ss) => { // weighted by length
    let total = 0; for (const s of ss) total += s.len;
    let r = pr() * total; for (const s of ss) { r -= s.len; if (r <= 0) return s; } return ss[ss.length - 1];
  };
  // adjacency first: a leaf is placed on the strands of the hub it hangs off, so the membership fibres
  // run along the strand bundle instead of across it — that is where the picture's bundles and gaps come from
  const adjTmp = new Map();
  for (let k = 0; k < L; k++) { const l = links[k], s = index.get(l.s), t = index.get(l.t); (adjTmp.get(s) || adjTmp.set(s, []).get(s)).push(t); (adjTmp.get(t) || adjTmp.set(t, []).get(t)).push(s); }
  const hubGroup = new Int32Array(N).fill(-1);
  const place = (i, s, u) => {
    nStrand[i] = s.i; nU[i] = u; nPerp[i] = (pr() + pr() + pr() - 1.5) * 2.0;
    strandAt(s, u, tmp);
    home[i * 2] = tmp[0] - tmp[3] * nPerp[i]; home[i * 2 + 1] = tmp[1] + tmp[2] * nPerp[i];
    tan[i * 2] = tmp[2]; tan[i * 2 + 1] = tmp[3];
    strandColor(s, u, col, i * 3);
    z[i] = (pr() * 2 - 1) * 0.7 + (s.lum - 0.5);
  };
  // APP: A HOSTED NODE is one of HIS objects standing inside one of the example's clusters
  // (`field-ghost.js`: `hosted: true`, `host`). Its `cluster` is its own source group, which is what
  // hover and click light; where it STANDS is its host's strands. It is laid out AFTER everything
  // else, each group on a random stream seeded from the group's own id (`layoutSeed`), so the example's
  // objects land exactly where they do in the demonstration and a group does not move when another
  // is added.
  const hosted = (i) => nodes[i].hosted === true;
  for (let c = 0; c < clusterList.length; c++) {
    const ids = byCluster[c].filter((i) => !hosted(i)).sort((a, b) => nodes[b].degree - nodes[a].degree);
    // APP: a cluster the dataset declared and put nothing in. `nh` floors at 1, so `hubs` would
    // be an empty slice, `groups` would stay empty, and the `groups[groups.length - 1]` below
    // would be `undefined`. The round's data has no such cluster and `home_field.rs` emits none
    // (`a_domains_node_count_is_its_own_nodes` asserts the clusters add up), so this is inert on
    // both datasets that exist — and it is one line against a crash on a third.
    if (!ids.length) continue;
    const seed = nodes[ids[0]].layoutSeed;
    pr = seed === undefined || nodes[ids[0]].hosted ? rnd : mulberry32(seed >>> 0);
    const ss = clusterList[c].strands;
    const nh = Math.max(1, Math.min(ids.length, Math.round(ids.length * 0.07), Math.floor(ss.length / 2)));
    const hubs = ids.slice(0, nh);
    // the cluster's strands, contiguous in angle, split into one bundle per hub by length share
    const total = ss.reduce((a, s) => a + s.len, 0);
    const groups = []; let k = 0, acc = 0;
    hubs.forEach((h, hi) => {
      const g = []; const target = hi === nh - 1 ? Infinity : acc + total / nh;
      while (k < ss.length && (acc < target || g.length === 0)) { g.push(ss[k]); acc += ss[k].len; k++; }
      groups.push(g);
    });
    while (k < ss.length) groups[groups.length - 1].push(ss[k++]);
    for (let gi = 0; gi < groups.length; gi++) if (!groups[gi].length) groups[gi] = ss;
    hubs.forEach((h, hi) => {
      const g = groups[hi]; let s = g[0]; for (const x of g) if (x.len > s.len) s = x;
      hubGroup[h] = hi;
      place(h, s, 0.16 + pr() * 0.3);
    });
    for (let r = nh; r < ids.length; r++) {
      const i = ids[r];
      let best = -1, bd = -1;
      for (const j of adjTmp.get(i) || []) { if (hubGroup[j] >= 0 && nodeCluster[j] === c && nodes[j].degree > bd) { bd = nodes[j].degree; best = j; } }
      const g = best >= 0 ? groups[hubGroup[best]] : groups[Math.floor(pr() * groups.length)];
      const s = pickStrand(g);
      place(i, s, 0.06 + Math.pow(pr(), 0.8) * 0.92);
    }
  }
  pr = rnd;
  {
    // each of HIS source groups is one hub and its leaves, all on the hub's own strand: a small, tight
    // group standing in the example, which is what a young loro is
    const inGroup = new Map();
    for (let i = 0; i < N; i++) if (hosted(i)) (inGroup.get(nodes[i].cluster) || inGroup.set(nodes[i].cluster, []).get(nodes[i].cluster)).push(i);
    inGroup.forEach((members) => {
      const rndOwn = mulberry32(nodes[members[0]].layoutSeed >>> 0);
      const ss = clusterList[clusterIndex.get(nodes[members[0]].host)].strands;
      const placeOwn = (i, s, u) => {
        nStrand[i] = s.i; nU[i] = u; nPerp[i] = (rndOwn() + rndOwn() + rndOwn() - 1.5) * 2.0;
        strandAt(s, u, tmp);
        home[i * 2] = tmp[0] - tmp[3] * nPerp[i]; home[i * 2 + 1] = tmp[1] + tmp[2] * nPerp[i];
        tan[i * 2] = tmp[2]; tan[i * 2 + 1] = tmp[3];
        strandColor(s, u, col, i * 3);
        z[i] = (rndOwn() * 2 - 1) * 0.7 + (s.lum - 0.5);
      };
      members.sort((a, b) => nodes[b].degree - nodes[a].degree || a - b);
      let tot = 0; for (const x of ss) tot += x.len;
      let r = rndOwn() * tot, s = ss[ss.length - 1]; for (const x of ss) { r -= x.len; if (r <= 0) { s = x; break; } }
      members.forEach((i, m) => placeOwn(i, s, m === 0 ? 0.16 + rndOwn() * 0.3 : 0.06 + Math.pow(rndOwn(), 0.8) * 0.92));
    });
  }
  for (let i = 0; i < N; i++) {
    const n = nodes[i];
    typeCode[i] = TYPES.indexOf(n.type); degree[i] = n.degree; significance[i] = n.significance; activity[i] = n.activity; recency[i] = n.recency; createdDay[i] = n.createdDay;
    let r = clamp(0.45 * Math.sqrt(n.degree + 1), 0.7, 4.2);
    if (n.type === 'decision') r *= 1 + 0.6 * n.significance;
    else if (n.type === 'theme' || n.type === 'initiative') r = Math.max(r, 2.2);
    else if (n.type === 'person' || n.type === 'organization') r = Math.max(r, 1.4);
    // APP: HIS OBJECT IS NEVER A SPECK. A record out of his corpus has a degree of 0 or 1 (a source
    // file is its own group), so by the rule above it would be the smallest dot in the picture; the
    // round's own early objects are hubs with radii of 3 and 4. One of his is "the most legible single
    // object in the round" (round 11.4 NOTES, v2), so it is never smaller than a person.
    if (n.hosted === true) r = Math.max(r, 1.4);
    radius[i] = r;
    let f = 0;
    if (n.type === 'commitment' && n.status === 'at-risk') f |= 1;
    if (n.type === 'decision' && n.reversibility === 'low') f |= 2;
    if (n.createdDay >= loro.meta.horizonDays - 14) f |= 4;
    flags[i] = f;
  }
  // ---- the picture's lights become the loro's brightest hubs ----
  const lights = REF.nodes.slice().sort((a, b) => b[5] - a[5]);
  const used = new Uint8Array(N);
  const hubList = Array.from({ length: N }, (_, i) => i).filter(i => degree[i] >= 10).sort((a, b) => degree[b] - degree[a]);
  let nLights = 0;
  lights.forEach((lt, li) => {
    const lx = lt[0] * BRAIN, ly = lt[1] * BRAIN;
    if (regionOf(lx, ly) === 'stem' && ly > 700) return;          // the river's lights stay the river's
    let best = -1, bd = 1e9;
    for (const i of hubList) { if (used[i]) continue; const d = Math.hypot(home[i * 2] - lx, home[i * 2 + 1] - ly); if (d < bd) { bd = d; best = i; } }
    if (best < 0 || bd > 90) return;
    used[best] = 1; isLight[best] = 1; nLights++;
    home[best * 2] = lx; home[best * 2 + 1] = ly;
    const c = vivid([lt[2] / 255, lt[3] / 255, lt[4] / 255], 1.2);
    col[best * 3] = c[0]; col[best * 3 + 1] = c[1]; col[best * 3 + 2] = c[2];
    lightR[best] = li < 36 ? 14 + rnd() * 12 : 7 + rnd() * 5;
    lightPhase[best] = rnd() * Math.PI * 2;
    radius[best] = Math.max(radius[best], 3.2);
    // it still belongs to a strand for the bloom: the nearest point of its own strand
    const s = strands[nStrand[best]]; let bu = 0, bdd = 1e9;
    for (let k = 0; k <= 40; k++) { const u = k / 40; strandAt(s, u, tmp); const d = Math.hypot(tmp[0] - lx, tmp[1] - ly); if (d < bdd) { bdd = d; bu = u; } }
    nU[best] = bu; strandAt(s, bu, tmp); nPerp[best] = 0; tan[best * 2] = tmp[2]; tan[best * 2 + 1] = tmp[3];
  });
  // the remaining strong hubs are smaller lights of their own colour
  for (const i of hubList) { if (isLight[i]) continue; if (degree[i] >= 32) { isLight[i] = 1; lightR[i] = 4.5 + rnd() * 3.5; lightPhase[i] = rnd() * Math.PI * 2; nLights++; } }

  // ---- bloom timing: from the core outward, the way the picture draws itself in ----
  for (let i = 0; i < N; i++) {
    const s = strands[nStrand[i]];
    delay[i] = 0.25 + s.d0 / 700 * 1.6 + rnd() * 0.45 + nU[i] * 0.4;
    dur[i] = 1.6 + rnd() * 0.9 + nU[i] * 0.8;
  }

  // ---- links ----
  const ls = new Int32Array(L), lt = new Int32Array(L), lclass = new Uint8Array(L), lalpha = new Float32Array(L), lbend = new Float32Array(L), lhero = new Uint8Array(L), lphase = new Float32Array(L);
  const linkW = new Float32Array(L);
  for (let k = 0; k < L; k++) {
    const l = links[k], s = index.get(l.s), t = index.get(l.t);
    ls[k] = s; lt[k] = t;
    const rel = l.class === 'relationship';
    lclass[k] = l.cross ? 2 : rel ? 1 : 0;
    const sig = Math.max(significance[s], significance[t]);
    const hero = (isLight[s] || isLight[t]) && rnd() < 0.5 || (rel && sig > 0.5 && rnd() < 0.5);
    lhero[k] = hero ? 1 : 0;
    // ink budget: ~2.4x the picture's stroke length, so per-stroke alphas sit at ~40 % of v4's
    lalpha[k] = hero ? 0.2 + rnd() * 0.16 : (rel ? 0.085 : 0.05) + rnd() * 0.07;
    if (l.cross) lalpha[k] *= 0.7;
    // the picture's bright bundles stay bright, its dim ones recede: that is where the gaps come from
    lalpha[k] *= 0.5 + 0.95 * strands[nStrand[s]].lum;
    lbend[k] = l.cross ? 0.5 : nodeCluster[s] !== nodeCluster[t] ? 0.22 : 0;
    linkW[k] = hero ? 1.5 + rnd() * 0.9 : 0.7 + rnd() * 0.5;
    lphase[k] = rnd();
  }
  // adjacency (CSR)
  const adjStart = new Int32Array(N + 1);
  for (let k = 0; k < L; k++) { adjStart[ls[k] + 1]++; adjStart[lt[k] + 1]++; }
  for (let i = 0; i < N; i++) adjStart[i + 1] += adjStart[i];
  const adj = new Int32Array(adjStart[N]), fill = adjStart.slice(0, N);
  for (let k = 0; k < L; k++) { adj[fill[ls[k]]++] = lt[k]; adj[fill[lt[k]]++] = ls[k]; }
  // links per node (for lighting a node's fibres)
  const nlinkStart = new Int32Array(N + 1);
  for (let k = 0; k < L; k++) { nlinkStart[ls[k] + 1]++; nlinkStart[lt[k] + 1]++; }
  for (let i = 0; i < N; i++) nlinkStart[i + 1] += nlinkStart[i];
  const nlink = new Int32Array(nlinkStart[N]), fill2 = nlinkStart.slice(0, N);
  for (let k = 0; k < L; k++) { nlink[fill2[ls[k]]++] = k; nlink[fill2[lt[k]]++] = k; }

  // ---- domain colour (for labels and the river) ----
  const domCol = new Float32Array(domains.length * 3), domCount = new Int32Array(domains.length);
  for (let i = 0; i < N; i++) { const d = nodeDomain[i]; domCol[d * 3] += col[i * 3]; domCol[d * 3 + 1] += col[i * 3 + 1]; domCol[d * 3 + 2] += col[i * 3 + 2]; domCount[d]++; }
  for (let d = 0; d < domains.length; d++) { const c = domCount[d] || 1; const v = vivid([domCol[d * 3] / c, domCol[d * 3 + 1] / c, domCol[d * 3 + 2] / c], 1.5); domCol[d * 3] = v[0]; domCol[d * 3 + 1] = v[1]; domCol[d * 3 + 2] = v[2]; }

  // ---- the river: 4,800 sources on the stem strands, banded by domain ----
  // Stem strands are ordered across the river's width; each domain's sources take a contiguous band, so
  // the river's coloured bands are the domains' shares of everything loro has learned from.
  const axis = { x: -0.965, y: 0.26 };                                   // the river's direction (down-left)
  const across = (s) => s.mid[0] * -axis.y + s.mid[1] * axis.x;          // lateral coordinate
  const river = stem.slice().sort((a, b) => across(a) - across(b));
  const riverLen = river.reduce((a, s) => a + s.len, 0);
  const sources = loro.sources.filter(s => index.has(s.primaryNodeId));
  const S = sources.length;
  const srcStrand = new Int32Array(S), srcPerp = new Float32Array(S), srcNode = new Int32Array(S), srcDomain = new Uint8Array(S), srcPhase = new Float32Array(S);
  {
    const perDomain = domains.map(() => []);
    sources.forEach((s, si) => { perDomain[domainIndex.get(s.domain)].push(si); });
    let k = 0, acc = 0;
    ARC.forEach((id, oi) => {
      const di = domainIndex.get(id), list = perDomain[di];
      const target = oi === ARC.length - 1 ? Infinity : acc + (list.length / S) * riverLen;
      const band = [];
      while (k < river.length && (acc < target || band.length === 0)) { band.push(river[k]); acc += river[k].len; k++; }
      // APP: A DOMAIN AFTER THE RIVER RAN OUT. The guarantee above (one strand per domain) holds only
      // while strands remain; the ghost (`field-ghost.js`) puts his lanes AFTER the twelve territories,
      // which have taken every stem strand by then, and `pickStrand([])` is `undefined` at `s.i` below.
      // His sources then ride the river's last strand rather than the picture failing to draw.
      if (!band.length && river.length) band.push(river[river.length - 1]);
      list.forEach((si, j) => {
        const s = pickStrand(band);
        srcStrand[si] = s.i; srcPerp[si] = (rnd() + rnd() - 1) * 1.3; srcNode[si] = index.get(sources[si].primaryNodeId); srcDomain[si] = di; srcPhase[si] = rnd();
      });
    });
    while (k < river.length) k++;
  }

  return {
    N, L, S, nodes, domains, index, strands, stem, CORE, TYPE_WORD,
    home, tan, nStrand, nU, nPerp, col, z, radius, isLight, lightR, lightPhase, nLights,
    typeCode, degree, flags, significance, activity, recency, createdDay, delay, dur,
    nodeCluster, nodeDomain, clusterList, byCluster,
    ls, lt, lclass, lalpha, lbend, lhero, lphase, linkW, adjStart, adj, nlinkStart, nlink,
    domCol, sources, srcStrand, srcPerp, srcNode, srcDomain, srcPhase,
    strandAt, strandColor, vivid, clamp, mulberry32,
  };
}
