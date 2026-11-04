/**
 * RichOS Workspace source — the GOOGLE CALENDAR adapter (the system architecture §3.x / §4.3, P1).
 *
 * Calendar first (not Drive/Mail): the smallest privacy surface (no document bodies, no inbound-attacker
 * content beyond an invite description), the simplest incremental primitive, and it gives loro the
 * TEMPORAL SKELETON everything else hangs off — "that decision came out of your 12 Aug leadership
 * meeting." The ideal source to prove governance + the core + OAuth end-to-end before touching docs/mail.
 *
 * Incremental sync = `events.list` with a `syncToken` (§4.3): a first run is a bounded full sync over a
 * rolling window; thereafter delta-only. An expired token surfaces as `GoneError` (410) → the CORE
 * resets the cursor and does a fresh full sync, deduped by the ingest ledger so nothing double-lands.
 *
 * The adapter is a thin normalizer over an injected GoogleClient — no auth logic, no governance, no
 * storage. It talks to ONE vendor API and turns raw → `SourceItem`. Everything downstream is vendor-blind.
 *
 * ── A PERSON HAS SEVERAL CALENDARS (2026-09-17) ──────────────────────────────────────────────────
 * Until this change the adapter read exactly one calendar (`opts.calendarId || 'primary'`) and nothing
 * else — a shared calendar, a secondary calendar, a subscribed team calendar never reached RichOS. Two
 * modes now exist, chosen by what the caller passes:
 *
 *   - LEGACY / EXPLICIT SINGLE CALENDAR (`opts.calendarId` given): byte-identical to the old behavior.
 *     `sourceInstanceId` still hashes in the calendar id, so an existing caller pinning a specific
 *     calendar keeps its own cursor and its own identity exactly as before. Every test that already
 *     exercised this shape (independent-cursor tests, the legacy-cursor-retirement test) is unaffected.
 *
 *   - MULTI-CALENDAR (the default — no `calendarId`, which is what `registry.js` actually constructs
 *     for a real account): the adapter enumerates the account's calendars on every poll
 *     (`calendarList.list`) and runs the same per-calendar events sync against each one, honoring
 *     `accessRole`/`hidden`/`deleted`. `opts.calendarIds` (an explicit array) skips discovery and syncs
 *     exactly that list — the escape hatch used when discovery is unavailable and by tests.
 *
 * IDENTITY DECISION (documented per the brief): in multi-calendar mode the source INSTANCE is the
 * ACCOUNT, not a calendar — `sourceInstanceId = hash(['google','calendar',accountId])`, no calendar
 * component. One instance means one `ingestOnce` pass, one entry in `sync-state.json`/`_last_sync.json`,
 * and ONE cursor value — which is why the cursor's shape changes from a bare `syncToken` string to an
 * object keyed by calendar id (`{ [calendarId]: syncToken }`). `sync-state.js` already persists an
 * opaque cursor for any adapter, so nothing there needed to change. A cursor written before this change
 * (a bare string) is read as belonging to the FIRST calendar synced this poll — normally `primary` — so
 * nobody's stored progress is discarded (`normalizeCursorMap`).
 *
 * DEDUP ACROSS CALENDARS: the SAME event can appear on two calendars the account can see (an invite
 * copied onto a shared calendar is the same meeting, with the same `iCalUID`, per Google's own
 * documentation — "the iCalUID is identical across calendars" even though each copy's `id` and `etag`
 * differ). The ledger dedups on (sourceItemId, vendorEtag), which would NOT collapse two copies with
 * different etags, so the adapter itself merges same-poll duplicates by `iCalUID`+start time BEFORE
 * anything reaches the ledger, keeping the first calendar's copy. That is collector-path parity for
 * "the same event," not "the same calendar."
 *
 * SCOPE (checked, not assumed): `calendarList.list` needs `calendar.readonly`, `calendar`,
 * `calendar.calendarlist` or `calendar.calendarlist.readonly` (Google's own method reference). Asked
 * to widen from "read events" to "read calendars, so every calendar syncs", the CEO answered **yes**
 * on 2026-09-17 (`richos-hq/wiki/ceo-decisions.md` §44), so `config.js:GOOGLE_SCOPES.calendar` now
 * pins `calendar.readonly` (`CALENDAR_LIST_SCOPE` below) — one of the four scopes above, so discovery
 * succeeds for an account that has been through the re-consent §44 requires (a code change is not a
 * consent: widening what this file REQUESTS was always the CEO's decision, never this adapter's, and
 * a stored grant only ever reflects a screen he actually approved).
 *
 * An installation still holding the OLD grant, `calendar.events.readonly` (`CALENDAR_EVENTS_ONLY_SCOPE`
 * below; `events.*` only, none of the four), still fails discovery with 403 — exactly as it did before
 * this file's scope pin changed — and that is reported as `degraded`, not silently masked: the adapter
 * falls back to `primary` only and says so. The code path below is IDENTICAL regardless of which grant
 * is live; only Google's own 403 decides which branch runs. That is the positive control that the
 * widening is CONSENT-gated, not code-gated: nothing here special-cases the old scope to force it into
 * degraded mode, so a re-consent is the only way discovery starts working, not a flag flip.
 */

import { createHash } from 'node:crypto';
import { buildSourceItem } from '../source-item.js';
import { GoneError } from '../google-client.js';

export const ADAPTER_VERSION = '1.0.0';
const API_BASE = 'https://www.googleapis.com/calendar/v3';
/** A bounded first-sync window: the CEO's recent + near-future calendar, not all history (§4.3). */
export const DEFAULT_FULL_SYNC_WINDOW_MS = 90 * 24 * 60 * 60 * 1000;

/** The scope this adapter is pinned to since §44 — permits `calendarList.list` (see module docblock). */
export const CALENDAR_LIST_SCOPE = 'https://www.googleapis.com/auth/calendar.readonly';

/** The pre-§44 scope: events only, no calendar discovery — the registry's fallback grant. */
export const CALENDAR_EVENTS_ONLY_SCOPE = 'https://www.googleapis.com/auth/calendar.events.readonly';

/** Why `calendarList.list` cannot run under `CALENDAR_EVENTS_ONLY_SCOPE` (see module docblock). */
export const CALENDAR_LIST_DEGRADED_REASON =
  'calendar list requires a broader Google scope than calendar.events.readonly grants '
  + '(calendarList.list needs calendar.readonly, calendar, calendar.calendarlist or '
  + 'calendar.calendarlist.readonly) — syncing the primary calendar only until you re-consent to a '
  + 'wider scope';

export class GoogleCalendarAdapter {
  /**
   * @param {{client:import('../google-client.js').GoogleClient, accountId:string, calendarId?:string,
   *   calendarIds?:string[], fullSyncWindowMs?:number, maxResults?:number, now?:() => number}} opts
   */
  constructor(opts) {
    if (typeof opts.accountId !== 'string' || !opts.accountId.trim()) {
      throw new Error('Calendar requires a stable accountId bound to the authenticated account');
    }
    this.accountId = opts.accountId.trim();
    this.client = opts.client;

    // LEGACY mode: an explicit single calendar id, byte-identical to the pre-2026-09-17 behavior.
    this.legacyCalendarId = typeof opts.calendarId === 'string' && opts.calendarId ? opts.calendarId : null;
    // MULTI mode, explicit list: skips discovery (the scope-degraded fallback path, and tests).
    this.explicitCalendarIds = Array.isArray(opts.calendarIds) && opts.calendarIds.length
      ? [...new Set(opts.calendarIds)]
      : null;
    this.multiCalendar = !this.legacyCalendarId;

    // Kept for reporting/labels and as the legacy path's own calendar id (default 'primary').
    this.calendarId = this.legacyCalendarId || 'primary';

    this.sourceInstanceId = this.multiCalendar
      ? createHash('sha256').update(JSON.stringify(['google', 'calendar', this.accountId])).digest('hex')
      : createHash('sha256')
        .update(JSON.stringify(['google', 'calendar', this.accountId, this.calendarId])).digest('hex');

    this.fullSyncWindowMs = opts.fullSyncWindowMs ?? DEFAULT_FULL_SYNC_WINDOW_MS;
    this.maxResults = opts.maxResults || 250;
    this.now = opts.now || (() => Date.now());
  }

  get vendor() {
    return 'google';
  }
  get source() {
    return 'calendar';
  }

  /**
   * `calendarList.list`, paginated, filtered to calendars actually worth syncing: neither hidden
   * (the CEO chose not to show it, which is a real signal — a calendar unhidden later is picked up
   * on the next poll) nor deleted. Ordered with `primary` first, then by id, so calendar order — and
   * therefore which copy of a cross-calendar duplicate wins the merge below — is deterministic.
   * @returns {Promise<{id:string, label:string, accessRole:string}[]>}
   */
  async listCalendars() {
    const entries = [];
    let pageToken = null;
    for (;;) {
      const u = new URL(`${API_BASE}/users/me/calendarList`);
      u.searchParams.set('maxResults', String(this.maxResults));
      u.searchParams.set('showHidden', 'false');
      u.searchParams.set('showDeleted', 'false');
      if (pageToken) u.searchParams.set('pageToken', pageToken);
      // eslint-disable-next-line no-await-in-loop
      const page = await this.client.getJson(u.toString());
      for (const c of page.items || []) {
        if (c.hidden || c.deleted || !c.id) continue;
        entries.push({ id: c.id, label: c.summaryOverride || c.summary || c.id, accessRole: c.accessRole || 'unknown' });
      }
      if (page.nextPageToken) {
        pageToken = page.nextPageToken;
        continue;
      }
      break;
    }
    entries.sort((a, b) => (a.id === 'primary' ? -1 : b.id === 'primary' ? 1 : a.id.localeCompare(b.id)));
    return entries;
  }

  /**
   * Poll for changes. `syncState` is the opaque cursor the core persisted last time (or null for a
   * first run / after a 410 reset). In LEGACY mode this is byte-identical to the original single
   * calendar loop. In MULTI mode it enumerates calendars, syncs each with its own cursor, merges the
   * results (deduping the same event seen on two calendars), and persists one cursor MAP.
   * @param {{syncToken?:string|Object<string,string|null>}|null} syncState
   * @returns {Promise<{items:any[], nextSyncState:{syncToken:*},
   *   calendars?:Array<{id:string,label:string,count:number,resynced:boolean,owned:boolean}>,
   *   degraded?:string}>}
   */
  async listChanges(syncState) {
    if (!this.multiCalendar) {
      return this.listChangesForCalendar(this.calendarId, syncState ? syncState.syncToken : null);
    }

    let calendars;
    let degraded;
    if (this.explicitCalendarIds) {
      calendars = this.explicitCalendarIds.map((id) => ({ id, label: id }));
    } else {
      try {
        calendars = await this.listCalendars();
        if (!calendars.length) calendars = [{ id: 'primary', label: 'Primary' }];
      } catch (err) {
        if (err && err.status === 403) {
          degraded = CALENDAR_LIST_DEGRADED_REASON;
          calendars = [{ id: 'primary', label: 'Primary' }];
        } else {
          throw err;
        }
      }
    }

    const priorMap = normalizeCursorMap(syncState ? syncState.syncToken : null, calendars[0].id);
    const nextMap = {};
    const report = [];
    const merged = [];
    const seen = new Set(); // cross-calendar de-dup key: iCalUID (or id) + start

    for (const cal of calendars) {
      const priorToken = priorMap[cal.id] ?? null;
      let resynced = false;
      let calResult;
      try {
        // eslint-disable-next-line no-await-in-loop
        calResult = await this.listChangesForCalendar(cal.id, priorToken);
      } catch (err) {
        if (err instanceof GoneError) {
          // A 410 on ONE calendar resets only that calendar's cursor — the others are untouched, and
          // this poll's items from every other calendar still land.
          // eslint-disable-next-line no-await-in-loop
          calResult = await this.listChangesForCalendar(cal.id, null);
          resynced = true;
        } else {
          throw err;
        }
      }
      nextMap[cal.id] = calResult.nextSyncState.syncToken;
      let landedCount = 0;
      for (const ev of calResult.items) {
        const key = dedupKeyFor(ev);
        if (key) {
          if (seen.has(key)) continue; // same event, already landed from an earlier calendar this poll
          seen.add(key);
        }
        merged.push(ev);
        landedCount += 1;
      }
      // `owned` is the ONE fact about a calendar that governance needs and cannot work out for
      // itself: a secondary calendar authors its own events under its own address, so whether the
      // account OWNS that address decides whether those events are the CEO's or a stranger's. It is
      // reported in the generic vocabulary the core already passes through — no accessRole, no
      // vendor word, nothing here inspected by anyone but the container-ownership rule.
      report.push({ id: cal.id, label: cal.label, count: landedCount, resynced, owned: cal.accessRole === 'owner' });
    }

    return {
      items: merged,
      nextSyncState: { syncToken: nextMap },
      calendars: report,
      ...(degraded ? { degraded } : {}),
    };
  }

  /**
   * The original single-calendar poll loop, now reusable per calendar id. Pages through the feed
   * until a `nextSyncToken` is returned.
   * @param {string} calendarId
   * @param {string|null} token
   * @returns {Promise<{items:any[], nextSyncState:{syncToken:string|null}}>}
   */
  async listChangesForCalendar(calendarId, token) {
    const items = [];
    let pageToken = null;
    let nextSyncToken = null;
    for (;;) {
      const url = this.buildListUrl({ calendarId, syncToken: token || null, pageToken });
      // eslint-disable-next-line no-await-in-loop
      const page = await this.client.getJson(url); // throws GoneError(410) on an expired syncToken
      for (const ev of page.items || []) items.push(ev);
      if (page.nextPageToken) {
        pageToken = page.nextPageToken;
        continue;
      }
      nextSyncToken = page.nextSyncToken || token || null;
      break;
    }
    return { items, nextSyncState: { syncToken: nextSyncToken } };
  }

  /** Build the events.list URL. Full sync (no token) uses a bounded timeMin window; delta uses syncToken. */
  buildListUrl({ calendarId, syncToken, pageToken }) {
    const u = new URL(`${API_BASE}/calendars/${encodeURIComponent(calendarId || this.calendarId)}/events`);
    u.searchParams.set('maxResults', String(this.maxResults));
    u.searchParams.set('singleEvents', 'true'); // expand recurrence so each instance is its own item
    u.searchParams.set('showDeleted', 'true'); // cancellations arrive as deletions → supersede/removal
    if (syncToken) {
      u.searchParams.set('syncToken', syncToken);
    } else {
      // Full sync: a bounded rolling window. orderBy is incompatible with syncToken, so only on full sync.
      u.searchParams.set('timeMin', new Date(this.now() - this.fullSyncWindowMs).toISOString());
      u.searchParams.set('orderBy', 'updated');
    }
    if (pageToken) u.searchParams.set('pageToken', pageToken);
    return u.toString();
  }

  /**
   * Calendar returns full event bodies in the list, so `fetchItem` is identity — the interface is kept
   * uniform (Drive/Mail adapters, whose lists return refs, do real fetches here).
   */
  async fetchItem(ref) {
    return ref;
  }

  /**
   * Normalize a raw Calendar event → the `SourceItem` contract (§4.1). Pure mapping, no I/O.
   * @param {any} ev
   * @returns {import('../source-item.js').SourceItem}
   */
  toSourceItem(ev) {
    const attendees = Array.isArray(ev.attendees)
      ? ev.attendees.map((a) => ({ name: a.displayName || '', email: a.email || '', orgRelation: a.self ? 'self' : 'unknown' }))
      : [];
    const organizer = ev.organizer || ev.creator || null;
    const author = organizer
      ? { name: organizer.displayName || '', email: organizer.email || '', orgRelation: organizer.self ? 'self' : 'unknown' }
      : null;

    const cancelled = ev.status === 'cancelled';
    // Cancellation is a supersede signal in temporal memory — never a hard delete.
    const supersedes = cancelled ? `google:calendar:${this.sourceInstanceId}:${ev.id}` : null;

    return buildSourceItem({
      vendor: 'google',
      source: 'calendar',
      kind: 'event',
      sourceItemId: `google:calendar:${this.sourceInstanceId}:${ev.id}`,
      // The SAME key the per-poll merge above uses, carried on the item so the merge survives a poll
      // boundary. A copy shared onto a second calendar next month arrives in a different `listChanges`
      // from the original, and the `seen` set is gone by then; the core recognizes it from this.
      //
      // SCOPED TO THE ACCOUNT, exactly as `sourceItemId` is, and the suite is what proved it must be:
      // one evidence zone holds every account's items, and two accounts of the CEO's can hold the
      // same meeting under the same `iCalUID`. Those are two items by the product's own rule ("the
      // SAME Google item id under two accounts is two items"), so an unscoped key would have let one
      // account's calendar silently suppress the other's.
      identityKey: dedupKeyFor(ev) ? `google:calendar:${this.sourceInstanceId}:${dedupKeyFor(ev)}` : '',
      provenance: {
        fetchedAt: this.now(),
        vendorEtag: String(ev.etag || ''),
        vendorUrl: String(ev.htmlLink || ''),
        adapterVersion: ADAPTER_VERSION,
      },
      actors: { author, attendees, recipients: [] },
      temporal: {
        occurredAt: parseEventTime(ev.start),
        validFrom: parseEventTime(ev.start),
        validUntil: parseEventTime(ev.end),
        supersedes,
      },
      scopeHint: cheapScopeHint(attendees),
      content: {
        title: String(ev.summary || (cancelled ? '(cancelled event)' : '(no title)')),
        text: String(ev.description || ''),
        structured: {
          location: ev.location || null,
          status: ev.status || 'confirmed',
          start: ev.start || null,
          end: ev.end || null,
          recurringEventId: ev.recurringEventId || null,
          meetingLinks: conferenceLinks(ev),
          cancelled,
        },
        attachmentsRefs: Array.isArray(ev.attachments)
          ? ev.attachments.map((at) => ({ title: at.title, fileUrl: at.fileUrl, mimeType: at.mimeType }))
          : [],
      },
    });
  }
}

/**
 * The event's video-call links: `hangoutLink` and the conference's web entry points. Google keeps a
 * Meet link (and a conferencing add-on's) only there, never in the description or location, and the
 * meeting-notes prompt reads the call app from it (richos-core `meeting_calendar.rs`). Phone and
 * SIP entry points are not links to a call app and are left out.
 */
function conferenceLinks(ev) {
  const entryPoints = ev.conferenceData && Array.isArray(ev.conferenceData.entryPoints) ? ev.conferenceData.entryPoints : [];
  const links = [ev.hangoutLink, ...entryPoints.map((p) => p && p.uri)]
    .filter((u) => typeof u === 'string' && /^https?:\/\//i.test(u));
  return [...new Set(links)];
}

/** Parse a Calendar start/end (dateTime or all-day date) into epoch ms, or null. */
function parseEventTime(t) {
  if (!t) return null;
  const raw = t.dateTime || t.date || null;
  if (!raw) return null;
  const ms = Date.parse(raw);
  return Number.isFinite(ms) ? ms : null;
}

/** The adapter's cheap first guess (governance §5.1 makes the binding call). No domains known here. */
function cheapScopeHint(attendees) {
  const others = attendees.filter((a) => a.orgRelation !== 'self');
  if (others.length === 0) return 'ceo-private'; // solo/self block
  return 'unknown'; // governance resolves domains and decides
}

/**
 * The cross-calendar identity key for de-dup: `iCalUID` (identical across every calendar's copy of
 * the same event, per Google's own documentation — an event's `id` is NOT) plus the start time (so
 * distinct instances of a recurring series, which can share an `iCalUID` base, stay distinct). Falls
 * back to the event's own `id` when `iCalUID` is absent, which only ever narrows the merge back to
 * "same calendar, same id" — never a false collapse across calendars.
 * @param {any} ev
 * @returns {string|null}
 */
function dedupKeyFor(ev) {
  if (!ev || !ev.id) return null;
  const uid = ev.iCalUID || ev.id;
  const start = ev.start ? (ev.start.dateTime || ev.start.date || '') : '';
  return `${uid}|${start}`;
}

/**
 * Migrate the persisted cursor into a per-calendar map. A cursor written before multi-calendar sync
 * existed is a bare string belonging to whichever calendar was synced back then — `firstCalendarId`
 * (normally `primary`) — so it is read as that calendar's token rather than discarded. Anything else
 * (already a map, or absent) passes through/starts empty.
 * @param {string|Object<string,string|null>|null|undefined} cursor
 * @param {string} firstCalendarId
 * @returns {Object<string,string|null>}
 */
export function normalizeCursorMap(cursor, firstCalendarId) {
  if (!cursor) return {};
  if (typeof cursor === 'string') return { [firstCalendarId]: cursor };
  if (typeof cursor === 'object') return { ...cursor };
  return {};
}
