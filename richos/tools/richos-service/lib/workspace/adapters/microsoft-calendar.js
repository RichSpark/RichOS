/**
 * RichOS Workspace source — the MICROSOFT CALENDAR adapter (the system architecture §3.x / §4.3, P4).
 *
 * The second vendor's first source, and the proof of the claim §3.x makes: *"Adding Microsoft after
 * Google = writing three classes against this interface."* Nothing downstream of `toSourceItem`
 * changes — the same governance gate, the same immune system, the same evidence zone, the same
 * synthesis pipeline, the same `core.js` with no vendor branch added to it.
 *
 * Incremental sync = `/me/calendarView/delta` (the §2 diagram's "Calendar (event delta)"). The opaque
 * cursor is Graph's own `@odata.deltaLink` — a whole URL rather than a bare token, which is what
 * Microsoft issues and therefore what gets persisted. `core.js` neither knows nor cares: an opaque
 * cursor is opaque. It rides in the `syncToken` field because that is the field name the
 * vendor-agnostic core persists and hands back, exactly as Gmail's `historyId` does.
 *
 * Storing a URL as the cursor has one property worth having on purpose: the stored cursor is
 * re-validated by `assertDirectMicrosoftEndpoint` before every delta request, so a tampered or
 * corrupted sync-state file cannot redirect the CEO's calendar poll at another host. The suite
 * asserts it.
 *
 * ── TWO VENDOR DEFAULTS THAT WOULD HAVE CHANGED WHAT RICHOS RECORDS ──────────────────────────────
 * (Standing CEO rule: never a third party's default for anything unless it is proven best.)
 *
 *   1. THE TIMEZONE. Graph returns event times in the MAILBOX'S configured timezone unless a caller
 *      says otherwise, and reports which one it used in `start.timeZone`. Inheriting that would mean
 *      a setting changed inside Outlook silently changes the epoch RichOS records as the time of a
 *      meeting — and loro's entire value is anchoring decisions to when they happened. So
 *      `Prefer: outlook.timezone="UTC"` is pinned on every request.
 *
 *   2. THE UNMARKED LOCAL TIME. Even under UTC, Graph's `dateTime` is `2025-08-12T15:00:00.0000000`
 *      — no `Z`, no offset. `Date.parse` of that string is DEFINED to mean local time, so on a
 *      machine in Berlin every meeting would land two hours out, in a way no test written on a UTC
 *      box would catch and no user would report as anything more specific than "the times are
 *      wrong". `parseGraphDateTime` applies the `timeZone` Graph declared instead of trusting the
 *      string's shape. This is the single highest-consequence function in the file.
 *
 * ── CANCELLATION ─────────────────────────────────────────────────────────────────────────────────
 * A canceled or deleted event is a SUPERSEDE signal in temporal memory, never a hard delete — the
 * same treatment `google-calendar.js` gives `status: "cancelled"`. Graph spells removal two ways: an
 * `isCancelled` flag on a still-present event, and an `@removed` tombstone carrying an id and almost
 * nothing else. Both are handled, and the tombstone path is why every field read below is defensive:
 * a tombstone is a legitimate item, not a malformed one.
 *
 * The adapter is a thin normalizer over an injected MicrosoftGraphClient — no auth logic, no
 * governance, no storage. It talks to ONE vendor API and turns raw into a `SourceItem`.
 *
 * ── A PERSON HAS SEVERAL CALENDARS (2026-09-17) ──────────────────────────────────────────────────
 * The same defect `google-calendar.js` had, and the same two-mode fix — see that file's docblock for
 * the full reasoning; only what differs for Graph is recorded here.
 *
 *   - LEGACY / EXPLICIT SINGLE CALENDAR (`opts.calendarId` given): byte-identical to the old
 *     behavior, `primary` addressed via `/me/calendarView/delta` exactly as before.
 *   - MULTI-CALENDAR (the default): `GET /me/calendars` enumerates the account's calendars, and each
 *     is synced via `/me/calendars/{id}/calendarView/delta` — including the default calendar, which
 *     this adapter addresses by its discovered id rather than special-casing `isDefaultCalendar`,
 *     since Graph documents the two forms as equivalent for that calendar. `opts.calendarIds` skips
 *     discovery, as on the Google side.
 *
 * IDENTITY: same decision as Google — the source instance is the ACCOUNT in multi-calendar mode
 * (`hash(['microsoft','calendar',accountId])`, no calendar component), so the cursor persisted for
 * that instance becomes a map keyed by calendar id rather than a bare `@odata.deltaLink` string. A
 * flat cursor written before this change is read as the first calendar's own link.
 *
 * SCOPE (checked, not assumed — unlike Google, this one is GOOD news): Microsoft's own least-privilege
 * table for `GET /me/calendars` lists `Calendars.ReadBasic` as least-privileged and `Calendars.Read`
 * (the scope this adapter is pinned to, `config.js:MICROSOFT_SCOPES.calendar`, unchanged by this
 * file) as an accepted higher permission — so discovery runs for real under the current grant. No
 * scope-degraded fallback path exists here for that reason; a non-scope failure still surfaces rather
 * than being silently swallowed, exactly as Google's adapter treats anything that isn't a 403.
 *
 * DEDUP ACROSS CALENDARS: Graph's own `iCalUId` is "a unique identifier for an event across
 * calendars" (its own resource reference) while `id` is not — the SAME cross-calendar-stable
 * identity Google's `iCalUID` provides. The adapter merges same-poll duplicates by `iCalUId`+start
 * before anything reaches the ledger, exactly as the Google adapter does.
 */

import { createHash } from 'node:crypto';
import { buildSourceItem } from '../source-item.js';
import { GRAPH_BASE, GoneError } from '../microsoft-client.js';

export const ADAPTER_VERSION = '1.0.0';

/** The scope this adapter is built against (§6.2, read-only). */
export const CALENDAR_SCOPE = 'https://graph.microsoft.com/Calendars.Read';

/** A bounded first-sync window: the CEO's recent calendar, not all history (§4.3). */
export const DEFAULT_FULL_SYNC_WINDOW_MS = 90 * 24 * 60 * 60 * 1000;

/**
 * How far FORWARD the bounded window reaches. `calendarView` requires BOTH ends — unlike Google's
 * `timeMin`-only full sync, which is open-ended into the future — so the forward edge is a number
 * this adapter must choose rather than inherit. A quarter ahead covers the CEO's planned calendar;
 * anything later is picked up by a delta long before it happens.
 */
export const DEFAULT_FORWARD_WINDOW_MS = 90 * 24 * 60 * 60 * 1000;

/**
 * The event fields RichOS reads, pinned rather than accepting Graph's default projection. A vendor
 * adding a field should never silently change what lands in the CEO's evidence zone.
 */
export const EVENT_SELECT = [
  'id', 'iCalUId', 'subject', 'body', 'bodyPreview', 'start', 'end', 'location', 'organizer', 'attendees',
  'isCancelled', 'isAllDay', 'isOrganizer', 'seriesMasterId', 'type', 'webLink', 'changeKey',
  'lastModifiedDateTime', 'createdDateTime', 'sensitivity', 'showAs',
];

export class MicrosoftCalendarAdapter {
  /**
   * @param {{client:import('../microsoft-client.js').MicrosoftGraphClient, accountId:string,
   *   calendarId?:string, calendarIds?:string[], fullSyncWindowMs?:number, forwardWindowMs?:number,
   *   maxResults?:number, now?:() => number}} opts
   */
  constructor(opts) {
    if (typeof opts.accountId !== 'string' || !opts.accountId.trim()) {
      throw new Error('Calendar requires a stable accountId bound to the authenticated account');
    }
    this.accountId = opts.accountId.trim();
    this.client = opts.client;

    // LEGACY mode: an explicit single calendar id, byte-identical to the pre-2026-09-17 behavior.
    // `primary` means the signed-in user's default calendar, addressed as `/me/calendarView`. A named
    // calendar is addressed through `/me/calendars/{id}/calendarView`. No shared or delegated mailbox
    // path exists here: the roadmap is explicit that this is the CEO's own calendar.
    this.legacyCalendarId = typeof opts.calendarId === 'string' && opts.calendarId ? opts.calendarId : null;
    // MULTI mode, explicit list: skips discovery (tests, and a deliberate fallback if ever needed).
    this.explicitCalendarIds = Array.isArray(opts.calendarIds) && opts.calendarIds.length
      ? [...new Set(opts.calendarIds)]
      : null;
    this.multiCalendar = !this.legacyCalendarId;

    this.calendarId = this.legacyCalendarId || 'primary';

    this.sourceInstanceId = this.multiCalendar
      ? createHash('sha256').update(JSON.stringify(['microsoft', 'calendar', this.accountId])).digest('hex')
      : createHash('sha256')
        .update(JSON.stringify(['microsoft', 'calendar', this.accountId, this.calendarId])).digest('hex');

    this.fullSyncWindowMs = opts.fullSyncWindowMs ?? DEFAULT_FULL_SYNC_WINDOW_MS;
    this.forwardWindowMs = opts.forwardWindowMs ?? DEFAULT_FORWARD_WINDOW_MS;
    this.maxResults = opts.maxResults || 250;
    this.now = opts.now || (() => Date.now());
  }

  get vendor() {
    return 'microsoft';
  }
  get source() {
    return 'calendar';
  }

  /** The least-privilege scope this adapter needs. Read-only (§6.2). */
  get requiredScopes() {
    return [CALENDAR_SCOPE];
  }

  /**
   * `GET /me/calendars`, paginated via `@odata.nextLink`. Least-privileged for this call is
   * `Calendars.ReadBasic`; `Calendars.Read` (this adapter's pinned scope) is an accepted higher
   * permission, so this genuinely runs under the current grant (see module docblock).
   * @returns {Promise<{id:string, label:string}[]>}
   */
  async listCalendars() {
    const entries = [];
    let url = (() => {
      const u = new URL(`${GRAPH_BASE}/me/calendars`);
      u.searchParams.set('$select', 'id,name,isDefaultCalendar');
      u.searchParams.set('$top', String(this.maxResults));
      return u.toString();
    })();
    for (;;) {
      // eslint-disable-next-line no-await-in-loop
      const page = await this.client.getJson(url);
      for (const c of page.value || []) {
        if (!c.id) continue;
        entries.push({ id: c.id, label: c.name || c.id, isDefault: c.isDefaultCalendar === true });
      }
      if (page['@odata.nextLink']) {
        url = page['@odata.nextLink'];
        continue;
      }
      break;
    }
    entries.sort((a, b) => (a.isDefault ? -1 : b.isDefault ? 1 : a.id.localeCompare(b.id)));
    return entries;
  }

  /**
   * Poll for changes. `syncState` is the opaque cursor the core persisted last time (or null for a
   * first run / after a 410 reset). In LEGACY mode this is byte-identical to the original single
   * calendar loop. In MULTI mode it enumerates calendars, syncs each with its own delta link, merges
   * the results (deduping the same event seen on two calendars), and persists one cursor MAP.
   * @param {{syncToken?:string|Object<string,string|null>}|null} syncState
   * @returns {Promise<{items:any[], nextSyncState:{syncToken:*},
   *   calendars?:Array<{id:string,label:string,count:number,resynced:boolean}>}>}
   */
  async listChanges(syncState) {
    if (!this.multiCalendar) {
      return this.listChangesForCalendar(this.calendarId, syncState ? syncState.syncToken : null);
    }

    const calendars = this.explicitCalendarIds
      ? this.explicitCalendarIds.map((id) => ({ id, label: id }))
      : await this.listCalendars();
    const resolved = calendars.length ? calendars : [{ id: 'primary', label: 'Calendar' }];

    const priorMap = normalizeCursorMap(syncState ? syncState.syncToken : null, resolved[0].id);
    const nextMap = {};
    const report = [];
    const merged = [];
    const seen = new Set(); // cross-calendar de-dup key: iCalUId (or id) + start

    for (const cal of resolved) {
      const priorToken = priorMap[cal.id] ?? null;
      let resynced = false;
      let calResult;
      try {
        // eslint-disable-next-line no-await-in-loop
        calResult = await this.listChangesForCalendar(cal.id, priorToken);
      } catch (err) {
        if (err instanceof GoneError) {
          // A 410 on ONE calendar resets only that calendar's cursor — every other calendar's items
          // from this poll still land, and its own cursor is untouched.
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
      report.push({ id: cal.id, label: cal.label, count: landedCount, resynced });
    }

    return { items: merged, nextSyncState: { syncToken: nextMap }, calendars: report };
  }

  /**
   * The original single-calendar delta loop, now reusable per calendar id. Pages through
   * `@odata.nextLink` until Graph returns the deltaLink that anchors the next poll.
   * @param {string} calendarId
   * @param {string|null} token  a stored `@odata.deltaLink` URL, or null for a bounded full sync
   * @returns {Promise<{items:any[], nextSyncState:{syncToken:string|null}}>}
   */
  async listChangesForCalendar(calendarId, token) {
    const items = [];
    let url = token ? String(token) : this.buildDeltaUrl(calendarId);
    let deltaLink = null;
    for (;;) {
      // Throws GoneError(410) when Graph has aged the delta token out; the caller resets the cursor
      // and re-runs a bounded full sync, deduped by the ingest ledger so nothing double-lands.
      // eslint-disable-next-line no-await-in-loop
      const page = await this.client.getJson(url, { prefer: this.preferHeaders() });
      for (const ev of page.value || []) items.push(ev);
      if (page['@odata.nextLink']) {
        url = page['@odata.nextLink'];
        continue;
      }
      deltaLink = page['@odata.deltaLink'] || token || null;
      break;
    }
    return { items, nextSyncState: { syncToken: deltaLink } };
  }

  /**
   * The `Prefer` values pinned on every request. Both would otherwise be Graph's own defaults, and
   * both change what RichOS records — see the module docblock.
   * @returns {string[]}
   */
  preferHeaders() {
    return [`odata.maxpagesize=${this.maxResults}`, 'outlook.timezone="UTC"'];
  }

  /** Build the first-run `calendarView/delta` URL over a bounded window in BOTH directions (§4.3). */
  buildDeltaUrl(calendarId) {
    const id = calendarId || this.calendarId;
    const base = id === 'primary'
      ? `${GRAPH_BASE}/me/calendarView/delta`
      : `${GRAPH_BASE}/me/calendars/${encodeURIComponent(id)}/calendarView/delta`;
    const u = new URL(base);
    u.searchParams.set('startDateTime', new Date(this.now() - this.fullSyncWindowMs).toISOString());
    u.searchParams.set('endDateTime', new Date(this.now() + this.forwardWindowMs).toISOString());
    u.searchParams.set('$select', EVENT_SELECT.join(','));
    return u.toString();
  }

  /**
   * `calendarView/delta` returns whole event bodies, so `fetchItem` is identity — the interface stays
   * uniform (OneDrive and Outlook, whose feeds return refs, do real fetches here).
   */
  async fetchItem(ref) {
    return ref;
  }

  /**
   * Normalize a raw Graph event into the `SourceItem` contract (§4.1). Pure mapping, no I/O.
   * @param {any} ev
   * @returns {import('../source-item.js').SourceItem}
   */
  toSourceItem(ev) {
    const raw = ev && typeof ev === 'object' ? ev : {};
    const removed = Boolean(raw['@removed']);
    // `isOrganizer` is a VENDOR FACT about the signed-in user, not an inference: Graph is telling us
    // the authenticated account owns this event. It is the only self-identification Graph offers here
    // (unlike Google, which marks `self` on the attendee itself), and it is what lets the cheap scope
    // hint below tell a solo block from a meeting. Resolving everyone ELSE's affiliation is the
    // governance gate's job (§5.1), never the adapter's.
    const selfIsOrganizer = raw.isOrganizer === true;
    const organizer = raw.organizer && raw.organizer.emailAddress ? raw.organizer.emailAddress : null;
    const organizerEmail = organizer ? String(organizer.address || '').trim().toLowerCase() : '';
    const author = organizer
      ? {
        name: String(organizer.name || ''),
        email: organizerEmail,
        orgRelation: selfIsOrganizer ? 'self' : 'unknown',
      }
      : null;

    const attendees = Array.isArray(raw.attendees)
      ? raw.attendees.map((a) => {
        const addr = a && a.emailAddress ? a.emailAddress : {};
        const email = String(addr.address || '').trim().toLowerCase();
        return {
          name: String(addr.name || ''),
          email,
          // The organizer appears in the attendee list too; when Graph has told us the organizer IS
          // the signed-in user, that one entry is `self`. Everything else stays unknown.
          orgRelation: selfIsOrganizer && email && email === organizerEmail ? 'self' : 'unknown',
        };
      })
      : [];

    const withdrawn = removed || raw.isCancelled === true;
    const sourceItemId = `microsoft:calendar:${this.sourceInstanceId}:${String(raw.id || '')}`;
    // A withdrawal or a tombstone supersedes the evidence already written for this event; it is never
    // a hard delete (temporal memory — loro-architecture #3).
    const supersedes = withdrawn ? sourceItemId : null;

    const start = parseGraphDateTime(raw.start);
    const end = parseGraphDateTime(raw.end);

    return buildSourceItem({
      vendor: 'microsoft',
      source: 'calendar',
      kind: 'event',
      sourceItemId,
      // The SAME key the per-calendar merge above uses, carried on the item so the merge survives a
      // poll boundary — one meeting on two calendars is one meeting whether or not both copies
      // happened to arrive in the same delta. Parity with the Google adapter, by construction,
      // including the account scoping: one zone holds every account's evidence, and two mailboxes
      // can carry the same `iCalUId` without being one item.
      identityKey: dedupKeyFor(raw) ? `microsoft:calendar:${this.sourceInstanceId}:${dedupKeyFor(raw)}` : '',
      provenance: {
        fetchedAt: this.now(),
        // Graph's per-revision identity. `changeKey` changes when the event changes, which is all the
        // (sourceItemId, vendorEtag) dedup key needs. A tombstone carries neither, so its removal
        // reason stands in — otherwise every poll would re-ingest the same tombstone as a new
        // revision of itself.
        vendorEtag: String(raw.changeKey || (removed ? `removed:${removedReason(raw)}` : '')),
        vendorUrl: String(raw.webLink || ''),
        adapterVersion: ADAPTER_VERSION,
      },
      actors: { author, attendees, recipients: [] },
      temporal: {
        occurredAt: start,
        validFrom: start,
        validUntil: end,
        supersedes,
      },
      scopeHint: cheapScopeHint(attendees, removed),
      content: {
        title: String(raw.subject || (withdrawn ? '(withdrawn event)' : '(no title)')),
        // The invite body is the injection surface an event has (§5.3) — the same role Google
        // Calendar's `description` plays — so it goes in `content.text`, which is the field
        // `immune.js` scans. HTML is stripped to text; nothing is stored the scanner cannot see.
        text: eventBodyText(raw),
        structured: {
          location: raw.location && raw.location.displayName ? String(raw.location.displayName) : null,
          // The vendor's own spelling is kept as the key so a reader can grep Graph's field name.
          isCancelled: raw.isCancelled === true,
          withdrawn,
          removed,
          removedReason: removed ? removedReason(raw) : null,
          isAllDay: raw.isAllDay === true,
          isOrganizer: selfIsOrganizer,
          eventType: raw.type || null,
          seriesMasterId: raw.seriesMasterId || null,
          sensitivity: raw.sensitivity || null,
          showAs: raw.showAs || null,
          // The timezone Graph says it answered in, kept so a later reader can tell a correctly
          // converted time from one that was parsed as an unmarked local string.
          startTimeZone: raw.start && raw.start.timeZone ? String(raw.start.timeZone) : null,
          endTimeZone: raw.end && raw.end.timeZone ? String(raw.end.timeZone) : null,
          lastModifiedDateTime: raw.lastModifiedDateTime || null,
          createdDateTime: raw.createdDateTime || null,
          bodyContentType: raw.body && raw.body.contentType ? String(raw.body.contentType) : null,
          meetingLinks: bodyLinkTargets(raw),
        },
        // Graph does not inline attachments in a calendarView response and this adapter never asks
        // for them: §4.1 says attachments are refs, fetched lazily and never bulk-hoarded.
        attachmentsRefs: [],
      },
    });
  }
}

/**
 * Parse a Graph `dateTimeTimeZone` into epoch ms.
 *
 * THE BUG THIS EXISTS TO PREVENT: Graph sends `{"dateTime":"2025-08-12T15:00:00.0000000",
 * "timeZone":"UTC"}`. The string has no `Z` and no offset, so `Date.parse` reads it as local time by
 * definition — correct on a UTC machine, silently wrong by the operator's offset everywhere else.
 * The declared `timeZone` is the authority, not the string's shape.
 *
 * This adapter pins `Prefer: outlook.timezone="UTC"`, so UTC is the answer it expects. Any other
 * declared zone is still resolved, through the platform's own IANA database via `Intl`, and if that
 * cannot be done the value is refused as `null` rather than guessed — a missing time is governable
 * evidence; a confidently wrong one is not.
 *
 * @param {{dateTime?:string, timeZone?:string}|null|undefined} t
 * @returns {number|null}
 */
export function parseGraphDateTime(t) {
  if (!t || typeof t !== 'object') return null;
  const raw = typeof t.dateTime === 'string' ? t.dateTime.trim() : '';
  if (!raw) return null;

  // An explicit offset or Z already settles it — nothing to infer.
  if (/(?:Z|[+-]\d{2}:?\d{2})$/.test(raw)) {
    const ms = Date.parse(raw);
    return Number.isFinite(ms) ? ms : null;
  }

  const zone = String(t.timeZone || '').trim();
  if (!zone || /^(?:utc|gmt)$/i.test(zone)) {
    // Appending `Z` is what turns "unmarked local" into the UTC instant Graph actually meant.
    const ms = Date.parse(`${raw}Z`);
    return Number.isFinite(ms) ? ms : null;
  }
  return parseInNamedZone(raw, zone);
}

/**
 * Resolve an unmarked wall-clock string in a named IANA zone using the platform's own timezone
 * database through `Intl` — never a hand-maintained offset table, which would be wrong twice a year.
 * Returns null when the zone is unknown to the platform: refusing is honest, guessing is not.
 */
function parseInNamedZone(wallClock, zone) {
  const guessMs = Date.parse(`${wallClock}Z`);
  if (!Number.isFinite(guessMs)) return null;
  let fmt;
  try {
    fmt = new Intl.DateTimeFormat('en-US', {
      timeZone: zone,
      hour12: false,
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit',
    });
  } catch {
    return null; // the platform does not know this zone
  }
  // Two passes: the offset at the guessed instant, then re-checked at the corrected instant, so a
  // timestamp sitting near a daylight-saving boundary lands on the right side of it.
  let ms = guessMs;
  for (let i = 0; i < 2; i += 1) {
    const offset = zoneOffsetMs(fmt, ms);
    if (offset === null) return null;
    ms = guessMs - offset;
  }
  return ms;
}

/** How far ahead of UTC `zone` is at instant `ms`, in milliseconds. */
function zoneOffsetMs(fmt, ms) {
  const parts = {};
  for (const p of fmt.formatToParts(new Date(ms))) parts[p.type] = p.value;
  if (!parts.year || !parts.month || !parts.day) return null;
  const hour = parts.hour === '24' ? '00' : parts.hour; // some locales render midnight as 24
  const asUtc = Date.parse(
    `${parts.year}-${parts.month}-${parts.day}T${hour}:${parts.minute}:${parts.second}Z`,
  );
  return Number.isFinite(asUtc) ? asUtc - ms : null;
}

/** Graph's tombstone reason, defensively — a tombstone carries almost no other field. */
function removedReason(raw) {
  const r = raw && raw['@removed'];
  return r && typeof r === 'object' && r.reason ? String(r.reason) : 'deleted';
}

/**
 * The event's own text, as text. Graph returns `body: {contentType: "html"|"text", content}`; an
 * Outlook invite is nearly always HTML. The tags are stripped rather than stored, because
 * `content.text` is what the immune system scans and what synthesis reads for commitment cues.
 * `bodyPreview` is the fallback when no body came back — it is plain text by construction.
 */
export function eventBodyText(raw) {
  const body = raw && raw.body && typeof raw.body === 'object' ? raw.body : null;
  const content = body && typeof body.content === 'string' ? body.content : '';
  if (content) {
    return /html/i.test(String(body.contentType || '')) ? stripHtml(content) : content;
  }
  return typeof raw.bodyPreview === 'string' ? raw.bodyPreview : '';
}

/**
 * The web link targets of an HTML invite, in order, once each. Stripping the tags drops them, and an
 * Outlook or Teams invite carries its join link only as one ("Join the meeting now"), so they are
 * kept here for the meeting-notes prompt, which reads the call app from the link (richos-core
 * `meeting_calendar.rs`). A plain-text body keeps its URLs in `content.text` already.
 */
export function bodyLinkTargets(raw) {
  const body = raw && raw.body && typeof raw.body === 'object' ? raw.body : null;
  if (!body || typeof body.content !== 'string' || !/html/i.test(String(body.contentType || ''))) return [];
  const links = [...body.content.matchAll(/\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')/gi)]
    .map((m) => (m[1] ?? m[2]).trim().replace(/&amp;/gi, '&'))
    .filter((u) => /^https?:\/\//i.test(u));
  return [...new Set(links)];
}

/** Tags out, entities decoded, whitespace tidied. Script and style CONTENT is dropped, not unwrapped. */
export function stripHtml(html) {
  return String(html)
    .replace(/<(?:script|style)\b[^>]*>[\s\S]*?<\/(?:script|style)>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/[ \t]+/g, ' ')
    .replace(/\s*\n\s*/g, '\n')
    .trim();
}

/**
 * The adapter's cheap first guess (governance §5.1 makes the binding call). The adapter knows no
 * domains, so it can only tell "nobody else was on this" from "somebody else was".
 */
function cheapScopeHint(attendees, removed) {
  if (removed) return 'unknown'; // a tombstone carries no attendee list to reason from
  const others = attendees.filter((a) => a.orgRelation !== 'self');
  if (others.length === 0) return 'ceo-private'; // solo/self block
  return 'unknown'; // governance resolves domains and decides
}

/**
 * The cross-calendar identity key for de-dup: `iCalUId` (Graph's own "unique identifier for an event
 * across calendars" — an event's `id` is NOT) plus the start time, so distinct recurring instances
 * stay distinct. Falls back to the event's own `id`/tombstone id when `iCalUId` is absent, which only
 * ever narrows the merge back to "same calendar, same id" — never a false collapse across calendars.
 * @param {any} raw
 * @returns {string|null}
 */
function dedupKeyFor(raw) {
  const id = raw && (raw.id || (raw['@removed'] && raw.id));
  if (!id) return null;
  const uid = raw.iCalUId || id;
  const start = raw.start && raw.start.dateTime ? raw.start.dateTime : '';
  return `${uid}|${start}`;
}

/**
 * Migrate the persisted cursor into a per-calendar map. A cursor written before multi-calendar sync
 * existed is a bare `@odata.deltaLink` URL belonging to whichever calendar was synced back then —
 * `firstCalendarId` — so it is read as that calendar's own link rather than discarded.
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
