// THE GHOST — one dataset out of two: the user's own records in front, the demonstration behind.
//
// CEO, 2026-11-02, `richos-hq/design/mockups/rounds/round-11.4`: "V2". Round 11.4 v2 ("The
// Ghost") fills the field from the second launch on with the whole shape of a finished loro, held
// at a dim floor, deaf to the pointer, counted in nothing and named on screen as an example. The
// user's own records burn inside it in warm gold, and the example's floor falls to nothing by 2,500
// of them: it is never removed, it is outnumbered.
//
// In the round every record was a prefix of the synthetic loro. In the app the records are his, so
// this file builds the ONE dataset the ported engine draws:
//
//   * the demonstration's objects, links, sources and territories, flagged `ghost: true`. They
//     keep their own names (the engine never prints a ghost object's name: it cannot be hovered,
//     it is in no count);
//   * his own, from `home_field_data`, every id prefixed `own-` so the two id spaces can never
//     meet (a company lane called "brand" must not become the demonstration's "brand" territory).
//     They are unflagged, and the engine's arrival order puts them first, so `V.count` = how many
//     of them there are = the number of records that have arrived.
//
// WHERE HIS OBJECTS STAND. A lane of his is not one of the example's twelve territories, and giving
// it an arc of its own would pile every record into one wedge at the end of the circle, not "a
// handful of warm lights scattered across a cold continent" (round 11.4, `v2/v2.js`). So each of his
// source groups is hosted by one of the example's clusters, chosen by a fixed stride so that his
// groups spread over all of them, and `field-prep.js` lays his objects out there, after the example's
// own and on a random stream of its own (`hosted: true`). Hosting is layout only: an object keeps its
// own lane for everything a person reads, so a territory name on the screen is always one of HIS lanes
// (the example's territory names are never drawn: the example is in no count), his lanes are
// `nodeCount: 0` territories with no arc of their own, and what hover and click treat as one group is
// his own source group (`cluster`), never the example's cluster that happens to host it (`host`).
//
// WHERE A GROUP STANDS DOES NOT DEPEND ON THE OTHERS. Its host and the random stream that places it
// come from a hash of its own id (`layoutSeed`), so a record added later does not move the groups
// already there, and the picture does not rearrange itself when the example's floor reaches zero:
// the example is still loaded then, at no light at all, because it is what he is laid out in.
//
// WHAT IS NOT CARRIED OVER from the demonstration: its specialists, its tasks and its
// `bragSignals`. Those are the demonstration's claims about a workforce he does not have, and the
// left column is his own (`home.js`, `renderTeamColumn`). `meta` is his, with `synthetic: false`,
// so everything that asks "whose loro is on the page" still answers "his".
//
// Pure: the same inputs give the same output, no DOM, no clock. `tests/home-ghost.js` drives it.

"use strict";

window.RichFieldGhost = (function () {
  /// At this many of HIS objects the example's floor has fallen to exactly zero (round 11.4
  /// `v2/index.html`, `ghostSpan: 2500`), so the picture is his alone: the example is still in the
  /// dataset, at no light, deaf and in no count.
  var SPAN = 2500;

  var HORIZON_DAYS = 40000; // `home_field.rs::HORIZON_DAYS`: nothing is drawn as "new"

  var ZERO_BRAG = {
    specialistsManaged: 0,
    specialistsActiveNow: 0,
    tasksCompleted: 0,
    tasksHandledWithoutCeo: 0,
    ceoMinutesSaved: 0,
    monthsWorkingTogether: 0,
    sourcesUnderstood: 0,
    memoryObjects: 0,
    decisionsRemembered: 0,
    lessonsAccumulated: 0,
  };

  var own = function (id) {
    return "own-" + id;
  };

  /// A number out of a string (djb2): the same group id is the same number on every launch.
  function hash(text) {
    var h = 5381;
    for (var i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
    return h >>> 0;
  }

  function copy(o, patch) {
    var c = {};
    for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) c[k] = o[k];
    for (var p in patch) c[p] = patch[p];
    return c;
  }

  /// How many drawable objects a `home_field_data` dataset holds. The one number the arrival gate
  /// and the example's floor are driven by.
  function objectsOf(field) {
    return field && Array.isArray(field.nodes) ? field.nodes.length : 0;
  }

  /// `demo` is the baked `window.MATURE_LORO`; `field` is his dataset or `null` (no corpus, or
  /// nothing in it). Returns the dataset to draw. The demonstration's own arrays are reused, not
  /// copied: they are 7,500 objects and nothing else on the page reads them.
  function merge(demo, field) {
    var mine = field && Array.isArray(field.nodes) ? field : null;
    var nodes = demo.nodes.map(function (n) {
      return copy(n, { ghost: true });
    });
    var links = demo.links.slice();
    var sources = demo.sources.slice();
    var domains = demo.domains.slice();

    if (mine) {
      var hostIds = [];
      var hostDomain = {};
      demo.domains.forEach(function (d) {
        d.clusters.forEach(function (c) {
          hostIds.push(c.id);
          hostDomain[c.id] = d.id;
        });
      });
      var hostOf = {};
      mine.nodes.forEach(function (n) {
        if (!Object.prototype.hasOwnProperty.call(hostOf, n.cluster)) hostOf[n.cluster] = hostIds[hash(own(n.cluster)) % hostIds.length];
        // `cluster` is his own group (what hover lights); `host` is the example's cluster it stands in
        nodes.push(copy(n, { id: own(n.id), domain: own(n.domain), cluster: own(n.cluster), host: hostOf[n.cluster], layoutSeed: hash(own(n.cluster)), hosted: true }));
      });
      var nodeHost = {};
      mine.nodes.forEach(function (n) {
        nodeHost[n.id] = hostOf[n.cluster];
      });
      mine.links.forEach(function (l) {
        links.push(copy(l, { s: own(l.s), t: own(l.t) }));
      });
      mine.sources.forEach(function (s) {
        // His source's thread in the river takes the band of the example territory that hosts its
        // object: with every source on his own lane's band (the one `nodeCount: 0` territory) 170 of
        // them were one bright ribbon on a single strand (measured on the real app, 170 records).
        // The river is the one place a territory's colors show, and these are the example's.
        sources.push(copy(s, { id: own(s.id), domain: hostDomain[nodeHost[s.primaryNodeId]] || own(s.domain), primaryNodeId: own(s.primaryNodeId) }));
      });
      mine.domains.forEach(function (d) {
        // `nodeCount: 0`: a territory with no arc of its own (its objects are hosted), carrying his
        // real groups as its clusters so that hover and click find them
        domains.push({
          id: own(d.id),
          label: d.label,
          nodeCount: 0,
          clusters: d.clusters.map(function (c) {
            return copy(c, { id: own(c.id) });
          }),
        });
      });
    }

    var meta = mine
      ? copy(mine.meta, { synthetic: false })
      : {
          name: "RichOS company memory",
          version: 1,
          synthetic: false,
          counts: { records: 0, objects: 0 },
        };
    meta.horizonDays = HORIZON_DAYS;
    meta.ghost = { objects: demo.nodes.length };

    return {
      meta: meta,
      bragSignals: mine && mine.bragSignals ? mine.bragSignals : ZERO_BRAG,
      specialists: [],
      domains: domains,
      nodes: nodes,
      links: links,
      tasks: [],
      sources: sources,
    };
  }

  return { SPAN: SPAN, merge: merge, objectsOf: objectsOf };
})();
