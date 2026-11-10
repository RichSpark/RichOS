/**
 * RichOS Workspace source — THE COMMANDS (`richos-service workspace …`).
 *
 * This is the runtime the library did not have. Before it, `lib/workspace/` held the governance layer,
 * the core spine, two adapters, the OAuth helpers and the token manager — and nothing anywhere
 * constructed any of them: no file outside `adapters/`, the README and the test suite so much as named
 * an adapter, and `core.js` had no caller at all. The CEO's own setup guide (Step 6) ends by telling
 * him to run `richos-service workspace connect google`, which did not exist. These four commands are
 * that sentence made true:
 *
 *   connect     the §6 auth ceremony — PKCE, the loopback consent leg, tokens into the OS keychain
 *   status      what is connected, how healthy the grant is, which sources run, what the last poll did
 *   sync        ONE pass of `ingestOnce` per granted source (`--once` is the only mode there is)
 *   disconnect  revoke vendor-side, then forget locally
 *
 * THREE DISCIPLINES HOLD THIS FILE TOGETHER.
 *
 * 1. NEVER-SILENT. Every refusal names the file, the scope or the step that would fix it. A source
 *    that will not run is printed BY NAME with the scope it lacks, because a sync that quietly covers
 *    fewer sources than the CEO believes is the exact failure this layer was built to prevent.
 * 2. NO TOKEN AND NO CLIENT SECRET IS EVER PRINTED — not truncated, not hashed, not in an error. Both
 *    live in the keychain and are read only by `TokenManager`; nothing here formats either. What the
 *    CEO sees of his client secret is the word "missing" or the words "in keychain".
 * 3. ONE PATH, NOT TWO. `status` reports the sources it does by calling the SAME `buildRegistry` that
 *    `sync` polls through, so what it displays cannot drift from what a sync would actually do. A
 *    display fed by its own second opinion is a wrong number waiting for a release.
 *
 * EVERY COMMAND HERE IS ACCOUNT-SHAPED (2026-09-17). The CEO has more than one Google account — the
 * first has his Drive and no mailbox, the second has his mail — so `connect --account X` ADDS X and
 * leaves every other grant alone, `status` and `sync` run the whole list and report per account and
 * per source, and `disconnect` will not guess which one he means while two are configured. The loop
 * is the only new thing: what happens INSIDE it for one account is the code that was already here,
 * reading one account's view of the config rather than the config.
 *
 * Everything external is injected (http, keychain backend, browser opener, clock, output sink) so the
 * suite drives the real dispatcher end to end against a mock Google — no live account, ever.
 */

import { execFile } from 'node:child_process';

import {
  workspaceZone, workspaceClientConfigPath, workspaceRunStatePath, workspaceSyncStatePath,
} from '../config.js';
import {
  loadClientConfig, saveClientConfig, validateClientConfig, clientConfigTemplate, identityFrom,
  migrateClientConfig, accountsOf, accountView, upsertAccount,
} from './client-config.js';
import { readInstalledClientFile } from './client-secret.js';
import {
  buildRegistry, parseGrantedScopes, scopesForSources, sourceEntry, sourceForGrantScope, scopeMatcherFor,
} from './registry.js';
import { resolveGrantedIdentity, describeIdentityAttempts, sameAddress } from './identity.js';
import { awaitAuthorizationCode, consentState } from './consent.js';
import { pkcePair } from './oauth.js';
import { defaultSecretBackend } from './keychain.js';
import {
  SUPPORTED_VENDORS, profileFor, registryForGrant, hasStoredClientSecret, saveStoredClientSecret,
  wantsClientAuth, vendorWords,
} from './vendors.js';
import { getSyncState, forgetInstances as forgetCursorsFor } from './sync-state.js';
import { getRunState, recordRun, describeRun, forgetInstances as forgetRunsFor } from './run-state.js';
import { ingestOnce } from './core.js';
import { runPromotion, describePromotion } from './promote-run.js';
import { planRepair, applyRepair, describeRepair, parseInstant } from './repair.js';
import { runCalendarSeed } from './calendar-seed.js';

export { SUPPORTED_VENDORS };

/**
 * WHY an identity could not be read, in one clause — a vendor that cannot be asked at all says so in
 * its own words (`vendors.js`), and a vendor that can says which of its probes the grant left unusable.
 */
function unverifiedReason(d, identity) {
  if (!(d.profile.identityProbes || []).length) {
    return d.profile.identityUnverifiableReason
      || `${d.words.label} cannot be asked which account a grant belongs to under the scopes RichOS requests`;
  }
  return identity.attempts.length
    ? 'every way of asking that your granted scopes allow was tried, and none of them answered'
    : 'none of the scopes that can name an account are in this grant';
}

/** Label column width, matching `doctor`'s existing alignment. */
const L = (label) => `${label}:`.padEnd(12);

/**
 * Fill in the real-world dependencies. Every one of them is overridable, and the suite overrides all
 * of them — which is what makes the command the test exercises the same command the CLI runs.
 */
function resolve(deps = {}) {
  const zone = deps.zone || workspaceZone();
  // The vendor is resolved FIRST because the client config file, the keychain service, the loopback
  // port and every refusal's vocabulary all hang off it. A default computed before the vendor is how
  // a Microsoft connect ends up reading Google's config file.
  const vendor = deps.vendor || 'google';
  const known = SUPPORTED_VENDORS.includes(vendor);
  return {
    vendor,
    // Null for a vendor nobody supports, so `checkVendor` below can answer with a sentence instead of
    // a stack trace. Every command calls it before it touches `profile`.
    profile: known ? profileFor(vendor) : null,
    words: known ? vendorWords(vendor) : null,
    zone,
    clientConfigFile: deps.clientConfigFile || workspaceClientConfigPath(zone, vendor),
    syncStateFile: deps.syncStateFile || workspaceSyncStatePath(zone),
    runStateFile: deps.runStateFile || workspaceRunStatePath(zone),
    backend: deps.backend || null, // resolved lazily: the platform store throws off macOS
    http: deps.http || ((url, init) => globalThis.fetch(url, init)),
    awaitCode: deps.awaitCode || awaitAuthorizationCode,
    openBrowser: deps.openBrowser || openInBrowser,
    onBrowserUnavailable: deps.onBrowserUnavailable,
    pkce: deps.pkce || pkcePair,
    makeState: deps.makeState || consentState,
    now: deps.now || (() => Date.now()),
    out: deps.out || ((line) => console.log(line)),
    linkBase: deps.linkBase || null,
    sources: deps.sources || null,
    only: deps.only || null,
    clientId: deps.clientId || null,
    clientFile: deps.clientFile || null,
    tenant: deps.tenant || null,
    clientSecret: deps.clientSecret || null,
    accountId: deps.accountId || null,
    // Promotion is ON by default: a sync exists so Rich can answer from the CEO's calendar, and
    // evidence nobody promoted answers nothing. `--no-promote` is the diagnostic pull.
    promote: deps.promote !== false,
    // Where the loro writer component lives. Injected only by the suite, which runs against the
    // in-repo one; in production `promotion-writer.js` resolves it (RICHOS_LORO_DIR, then in-repo).
    loroDir: deps.loroDir || null,
    timeoutMs: deps.timeoutMs,
    keychainService: deps.keychainService,
    forgetCursors: Boolean(deps.forgetCursors),
    // `repair`'s window and mode. `since`/`until` are whatever the operator typed — parsed by
    // `repair.js`, which refuses a time it cannot read rather than letting it become a NaN that
    // matches everything or nothing.
    since: deps.since ?? null,
    until: deps.until ?? null,
    apply: Boolean(deps.apply),
    dryRun: Boolean(deps.dryRun),
    // `seed-calendar`'s own two flags, and the keychain service its grant lives under. The service
    // is overridable ONLY so the suite can drive the real command against an in-memory store; the
    // default is a name nothing in the sync path reads (see calendar-seed.js).
    teardown: Boolean(deps.teardown),
    setId: deps.setId || null,
    seedKeychainService: deps.seedKeychainService,
  };
}

/** The OS secure store, or a refusal that says why (Windows/Linux have a documented seam, not a store). */
function resolveBackend(d) {
  if (d.backend) return { backend: d.backend };
  try {
    return { backend: defaultSecretBackend() };
  } catch (err) {
    return { error: String(err.message || err) };
  }
}

/** Open a URL in the CEO's default browser. Best-effort: failing to open is never failing the flow. */
function openInBrowser(url) {
  return new Promise((resolve_) => {
    const cmd = process.platform === 'darwin' ? 'open' : process.platform === 'win32' ? 'cmd' : 'xdg-open';
    const args = process.platform === 'win32' ? ['/c', 'start', '', url] : [url];
    execFile(cmd, args, (err) => resolve_(!err));
  });
}

/**
 * Load + validate the CEO's client config, printing the actionable refusal if it is not there yet.
 *
 * `mayAdd` is CONNECT'S ALONE. Only a consent ceremony may put a new account in this file: `status`,
 * `sync` and `disconnect` take `--account` to CHOOSE one, and a mistyped address there has to come
 * back as "that is not one of your accounts", not as a new entry written to disk. A test caught the
 * version of this where every command shared the branch: `sync --account typo@acme.com` created
 * `typo@acme.com`, then reported it as not connected.
 * @returns {{config?:object, exitCode?:number}}
 */
function requireClientConfig(d, { mayAdd = false } = {}) {
  let raw;
  try {
    raw = loadClientConfig(d.clientConfigFile);
  } catch (err) {
    d.out(String(err.message));
    return { exitCode: 1 };
  }

  // `connect --client-id … --account …` is the one-command form of the guide's Step 5 — and with a
  // config already on disk it ADDS that account rather than replacing what is there. That is the
  // whole fix: the same command the CEO ran for his first account is the command for his second, and
  // the first account's entry comes through it untouched.
  if (mayAdd && (d.clientId || d.accountId || d.tenant)) {
    const base = migrateClientConfig(raw, d.vendor).config;
    let merged = {
      ...base,
      ...(d.clientId ? { clientId: String(d.clientId).trim() } : {}),
      // Entra's second id. It belongs to the CLIENT, not to an account, so `--tenant` on any connect
      // sets it for the whole app — which is what it is: one app registration, one directory.
      ...(d.tenant ? { tenant: String(d.tenant).trim() } : {}),
    };
    if (d.accountId) {
      // An account already in the file keeps its own sources (resolved to whatever the registry
      // declares TODAY, never a URL frozen on the day it was first granted); a new one starts with
      // the shared default and is narrowed or widened by `--source` in `connect` below.
      const existing = accountView(merged, d.accountId);
      merged = upsertAccount(merged, {
        accountId: d.accountId,
        sources: existing ? existing.sources : [],
        legacyScopes: existing ? existing.legacyScopes : [],
        orgDomains: existing ? existing.orgDomains : [],
      });
    }
    const check = validateClientConfig(merged, { vendor: d.vendor });
    if (!check.ok) {
      d.out('That is not yet a complete OAuth client config:');
      for (const p of check.problems) d.out(`  - ${p}`);
      return { exitCode: 1 };
    }
    saveClientConfig(check.config, d.clientConfigFile, { vendor: d.vendor });
    d.out(`${L('config')}wrote ${d.clientConfigFile}`);
    return { config: check.config };
  }

  if (!raw) {
    d.out(`No ${d.words.label} OAuth client config yet. RichOS never ships one — the app registration is yours (§6.1).`);
    d.out('');
    d.out(`Put your own client details at:  ${d.clientConfigFile}`);
    d.out('');
    for (const line of clientConfigTemplate(d.vendor).split('\n')) d.out(`  ${line}`);
    d.out('');
    if (d.vendor === 'google') {
      d.out('Or let RichOS read it out of the JSON your Google Cloud console downloads:');
      d.out('  richos-service workspace connect google --client-file <client_secret_….json> --account you@yourcompany.com');
    } else {
      d.out('Or hand both ids straight to connect, which writes this file for you:');
      d.out('  richos-service workspace connect microsoft --client-id <application (client) id> \\');
      d.out('      --tenant <directory (tenant) id> --account you@yourcompany.com');
    }
    d.out('');
    d.out(`The 10-minute setup is ${d.profile.guide}.`);
    return { exitCode: 1 };
  }

  const check = validateClientConfig(raw, { vendor: d.vendor });
  if (!check.ok) {
    d.out(`The OAuth client config at ${d.clientConfigFile} is not usable yet:`);
    for (const p of check.problems) d.out(`  - ${p}`);
    return { exitCode: 1 };
  }
  // TWO independent things can move on first read, and each is announced rather than done quietly —
  // the CEO is invited to open this file, so he is told when it changes under him:
  //   1. A single-account file written before accounts became a list moves to the list shape.
  //   2. An account still holding a scope URL frozen at some past consent moves to source names,
  //      resolved through the registry from here on — the fix a plain re-consent depended on.
  // Both can fire on the SAME read (an old v1 file has both problems at once), so one save covers it.
  if (check.migrated) {
    saveClientConfig(check.config, d.clientConfigFile, { vendor: d.vendor });
    if (check.migratedShape) {
      d.out(`${L('config')}${d.clientConfigFile} now lists accounts (your existing account is unchanged)`);
    }
    if (check.migratedScopes) {
      d.out(`${L('config')}${d.clientConfigFile} now names sources instead of scope URLs `
        + '(a re-consent asks for what is granted TODAY, not what was frozen when you first connected)');
    }
  }
  return { config: check.config };
}

/**
 * A TokenManager bound to ONE account's view of the config and the injected store/transport.
 *
 * `adoptLegacyTokens` goes to the FIRST account in the file and to no other: the one grant written
 * before keys carried an address belongs to the account that was connected then, which is the account
 * the migration put at the head of the list. It is a one-shot — the adoption deletes the old item —
 * so the rule can never reach a second account's grant.
 */
function tokenManagerFor(account, backend, d, opts = {}) {
  return d.profile.tokenManager(account, backend, d, opts);
}

/**
 * The registry for a stored grant — the ONE construction path `status` and `sync` both use.
 * @returns {{enabled:Array, skipped:Array, granted:string[]}}
 */
function registryFor(record, account, tm, d) {
  return registryForGrant(d.profile, record, account, tm, d);
}

/**
 * EVERY source instance one account owns, granted or not — the ids its cursors and its last-run
 * records are filed under.
 *
 * It builds them through `buildRegistry` with every declared scope, which means the ids come off the
 * same adapter constructors a sync polls with rather than off a second hash written here. A separate
 * id derivation would be a second opinion about what a cursor is called, and `--forget-cursors` would
 * eventually silently forget nothing.
 * @returns {string[]}
 */
function instanceIdsFor(account, tm, d) {
  const { enabled } = buildRegistry({
    vendor: d.vendor,
    grantedScopes: Object.values(d.profile.scopes),
    accountId: account.accountId,
    now: d.now,
    makeClient: () => d.profile.makeClient(tm, d),
  });
  return enabled.map((e) => e.adapter.sourceInstanceId);
}

/**
 * The accounts a command runs over.
 *
 * `--account X` names one. Without it, the commands that READ (`status`, `sync`) run every configured
 * account — that is the point of a list — and the commands that CHANGE one account (`connect`,
 * `disconnect`) refuse to guess while more than one is configured, listing the flag for each. With
 * exactly one account there is nothing to guess and nothing to ask.
 *
 * @returns {{views:Array<object>}|{exitCode:number}}
 */
function selectAccounts(config, d, opts = {}) {
  const all = accountsOf(config);
  const view = (id) => accountView(config, id, { vendor: d.vendor });
  if (d.accountId) {
    const want = String(d.accountId).trim().toLowerCase();
    const one = view(want);
    if (!one) {
      d.out(`"${want}" is not a ${d.words.account} in ${d.clientConfigFile}. Configured:`);
      for (const a of all) d.out(`  ${a.accountId}`);
      d.out('');
      d.out(`Add it with:  richos-service workspace connect ${d.vendor} --account ${want}`);
      return { exitCode: 1 };
    }
    return { views: [one] };
  }
  if (opts.one && all.length > 1) {
    d.out(`${all.length} ${d.words.label} accounts are configured, so RichOS will not pick one to ${opts.verb}:`);
    for (const a of all) d.out(`  --account ${a.accountId}`);
    return { exitCode: 1 };
  }
  return { views: all.map((a) => view(a.accountId)) };
}

/** True for the account that owns the pre-list keychain item — the first in the file, and only it. */
function isFirstAccount(config, accountId) {
  const all = accountsOf(config);
  return Boolean(all.length) && all[0].accountId === accountId;
}

/** Health states, worst first — so a run over several accounts reports the one that needs him. */
const HEALTH_ORDER = [
  'refresh-expired',   // Google: the Testing-mode 7-day clock ran out
  'refresh-refused',   // Microsoft: Entra actually refused the stored grant (an observed fact)
  'no-consent',
  'refresh-expiring-soon',
  'idle-advisory',     // Microsoft: unused for months — a heads-up, never a deadline
  'healthy',
];

function worstHealth(states) {
  for (const s of HEALTH_ORDER) if (states.includes(s)) return s;
  return states[0] || 'no-consent';
}

/** Refuse anything but a vendor that actually has an auth ceremony and adapters. */
function checkVendor(d) {
  if (d.profile) return null;
  d.out(`"${d.vendor}" is not a Workspace vendor RichOS can connect. Available: ${SUPPORTED_VENDORS.join(', ')}.`);
  d.out('Each one is a separate consent, a separate keychain entry and a separate sync position; you may connect either or both.');
  return { exitCode: 1 };
}

// =================================================================================================
// connect
// =================================================================================================

/**
 * The §6 auth ceremony. Loopback PKCE against the CEO's OWN OAuth client: no RichOS server, tokens
 * straight into the OS keychain, and the client secret Google demands of a Desktop-app client kept in
 * that same keychain rather than in any file.
 *
 * Re-running it for the SAME account re-consents (the guide's "2-click re-consent"): the keychain
 * write is an update, so a fresh grant replaces that account's old record cleanly. Running it with a
 * DIFFERENT `--account` ADDS that account — its own entry in the config, its own keychain item, its
 * own consent screen — and every account already connected is left exactly as it was. Until
 * 2026-09-17 the second form was the first one: one config field and one keychain key meant the
 * CEO's second Google account silently replaced his first.
 */
export async function connect(deps = {}) {
  const d = resolve(deps);
  const bad = checkVendor(d);
  if (bad) return bad;

  // `--client-file <the client_secret_….json the console downloads>` is Steps 4 and 5 in one flag.
  // TWO fields are read out of it and nothing else: the client id, which goes in the config file the
  // CEO can read, and the client secret, which goes into the keychain below and into no file at all.
  let downloaded = null;
  if (d.clientFile) {
    try {
      downloaded = readInstalledClientFile(d.clientFile);
    } catch (err) {
      d.out(String(err.message));
      return { exitCode: 1 };
    }
    if (d.clientId && d.clientId !== downloaded.clientId) {
      // Two different clients named in one command is a coin toss, and the losing side is a consent
      // screen for an app the CEO did not mean. Refuse, and show both.
      d.out('--client-id and --client-file name two different OAuth clients, so RichOS will not pick one:');
      d.out(`  --client-id    ${d.clientId}`);
      d.out(`  --client-file  ${downloaded.file}`);
      return { exitCode: 1 };
    }
    d.clientId = downloaded.clientId;
  }

  const loaded = requireClientConfig(d, { mayAdd: true });
  if (loaded.exitCode) return { exitCode: loaded.exitCode };
  let config = loaded.config;

  // Exactly one account is consented to per run: a consent screen is one account's, so a connect that
  // silently covered several would be claiming an approval the CEO never gave.
  const chosen = selectAccounts(config, d, { one: true, verb: 'connect' });
  if (chosen.exitCode) return { exitCode: chosen.exitCode };
  const account = chosen.views[0];

  // Which scopes to request: the CURRENT registry scope for this account's sources, or exactly the
  // sources named with --source. Either way the VALUES come from the scope registry (config.js),
  // never from a literal typed here, in the guide, or frozen in the config file at some past consent
  // — so a scope the CEO re-rules (Drive's width §40, Calendar's §44) is what a plain re-consent
  // requests, not whatever was true the day this account was first connected. `account.scopes` is
  // `accountView`'s OWN computed field (registry.js:scopesForSources against `account.sources`), so
  // this line needs no resolution step of its own — resolving it twice would be the second opinion
  // the account-config docstring already warns against.
  let scopes = account.scopes;
  if (d.sources && d.sources.length) {
    const unknown = d.sources.filter((s) => !sourceEntry(s, d.vendor));
    if (unknown.length) {
      d.out(`unknown source(s): ${unknown.join(', ')}. Known: ${d.profile.sources.map((s) => s.source).join(', ')}.`);
      return { exitCode: 1 };
    }
    scopes = scopesForSources(d.sources, d.vendor);
    // Persist what he actually chose — as SOURCE NAMES, never as the scope URLs those names resolve
    // to today — so a re-consent a week later requests the CURRENT scope for the same sources without
    // him having to remember the flags, and without freezing today's URL the way the old shape did.
    // `upsertAccount` writes THIS account's entry and no other: `--source drive` on the mail account
    // may not narrow the calendar account. An explicit `--source` list is a full replacement, exactly
    // like the URL list it replaces was — a legacy scope this run did not name is dropped, same as a
    // source this run did not name.
    const wanted = [...new Set(d.sources)];
    if (wanted.slice().sort().join(',') !== account.sources.slice().sort().join(',')) {
      config = upsertAccount(config, { accountId: account.accountId, sources: wanted, orgDomains: account.orgDomains });
      saveClientConfig(config, d.clientConfigFile, { vendor: d.vendor });
      account.sources = wanted;
      account.legacyScopes = [];
      account.scopes = scopes;
    }
  }

  const backendResult = resolveBackend(d);
  if (backendResult.error) {
    d.out(`cannot reach a secure token store: ${backendResult.error}`);
    return { exitCode: 1 };
  }
  const backend = backendResult.backend;

  // THE SECRET IS TAKEN IN BEFORE THE TOKEN MANAGER IS BUILT, because the manager reads it at
  // construction: storing it afterwards would put it in the keychain and NOT in the config the
  // exchange sends, and the connect would fail for a reason the keychain contradicts.
  //
  // `--client-secret` is also the DECLARATION that a registration is confidential. On the Microsoft
  // side `assertPublicClient` refuses a secret that arrives with no such declaration, and it is right
  // to: a secret appearing by habit and a registration that genuinely needs one need opposite
  // responses. Handing it over on this command is the CEO saying which of the two this is, so it is
  // recorded in his config rather than inferred from a stray field.
  if (d.clientSecret) {
    saveStoredClientSecret(d.profile, account, backend, d, String(d.clientSecret));
    if (!account.confidentialClient) {
      config = { ...config, confidentialClient: true };
      saveClientConfig(config, d.clientConfigFile, { vendor: d.vendor });
      account.confidentialClient = true;
    }
  }
  if (downloaded) {
    saveStoredClientSecret(d.profile, account, backend, d, downloaded.clientSecret);
  }

  const tm = tokenManagerFor({ ...account, scopes }, backend,
    d, { adoptLegacyTokens: isFirstAccount(config, account.accountId) });

  // What Google ACTUALLY granted last time — a local keychain read, never a live call — so a plain
  // re-consent can say WHY it is asking again. This is the seam that was missing: `status`'s own
  // re-consent line pointed here and, until now, a re-consent for an account already at this width
  // would have silently re-requested the OLD narrow scope, discharging nothing.
  const priorRecord = tm.load();
  const priorGranted = priorRecord ? parseGrantedScopes(priorRecord.scope) : [];
  const priorSources = new Set(priorGranted.map((g) => sourceForGrantScope(g, d.vendor)).filter(Boolean));

  d.out(`${L('account')}${account.accountId}`);
  // What this run is NOT touching, said before the browser opens. The failure this replaced was
  // silent, so the reassurance is explicit rather than left to the CEO to verify afterwards.
  const others = accountsOf(config).filter((a) => a.accountId !== account.accountId);
  if (others.length) d.out(`${L('keeping')}${others.map((a) => a.accountId).join(', ')}  (untouched by this consent)`);
  d.out(`${L('client')}${account.clientId}  (yours — RichOS ships no OAuth client of its own)`);
  // The DIRECTORY the sign-in is pinned to, printed before the browser opens because it is the one
  // field of an Entra config whose mistake is invisible afterwards: a wrong tenant signs him in
  // somewhere that is not his, and the failure arrives as an AADSTS code after a consent screen.
  if (account.tenant) d.out(`${L('tenant')}${account.tenant}  (your own directory — never "/common")`);
  d.out(`${L('requesting')}${scopes.length} read-only scope${scopes.length === 1 ? '' : 's'}:`);
  for (const s of scopes) {
    const entry = d.profile.sources.find((e) => e.scope === s);
    d.out(`            ${s}${entry ? `   (${entry.label})` : ''}`);
    // Named beside the scope it is about, not as a summary line at the end: with more than one
    // source in one request, "widened" has to say WHICH one or it is a fact he cannot act on.
    if (entry && priorGranted.length && !priorGranted.includes(s) && priorSources.has(entry.source)) {
      d.out(`              wider than your current ${entry.label} authorization — that is why you are being asked again`);
    }
  }

  // THE SECRET, BEFORE THE BROWSER — AND WHAT IS CHECKABLE HERE IS DIFFERENT PER VENDOR.
  //
  // GOOGLE: its Desktop-app client type refuses the token exchange without `client_secret` even under
  // PKCE (client-secret.js has the probe), and the first live attempt found that out AFTER the CEO had
  // approved the consent screen — a wasted approval and a `400 invalid_request` for an answer. That is
  // a fact about the client TYPE, so it is checkable before the browser opens, and it is checked here
  // where the fix costs one flag instead of one consent.
  //
  // MICROSOFT: the intended registration is a PUBLIC client that needs no secret, and Entra's own
  // discovery document does not advertise `none` as a token-endpoint auth method — so "no secret is
  // required" is the DOCUMENTED path and not a proven one, and whether THIS registration allows public
  // client flows is a per-app switch nothing outside the CEO's tenant can read. There is therefore
  // nothing honest to check before the attempt. What there IS is a precise refusal afterwards, which
  // `connect` prints at the exchange below: Entra names the condition with an AADSTS code, and the
  // fix is one switch in the portal. A pre-check here would be this file inventing a fact about his
  // app registration, which is exactly what the Google side got wrong in the other direction.
  if (downloaded) {
    d.out(`${L('secret')}read from ${downloaded.file} and stored in the OS keychain — never written to any RichOS file`);
  } else if (d.clientSecret) {
    d.out(`${L('secret')}stored in the OS keychain (service ${tm.service}) — never written to any RichOS file`);
    d.out(`            your ${d.words.label} app is recorded as a CONFIDENTIAL client, so RichOS will send it at the token exchange`);
  } else if (d.vendor === 'google' && !hasStoredClientSecret(d.profile, account, backend, d)) {
    d.out('');
    d.out('NOT CONNECTED — no client secret on file, and Google will refuse the token exchange');
    d.out('without one. Your OAuth client is a "Desktop app": Google requires its secret at the');
    d.out('exchange even with PKCE, and does not treat it as confidential.');
    d.out('');
    d.out('On your OAuth client page in the Google Cloud console, use "Download JSON", then:');
    d.out('');
    d.out(`  richos-service workspace connect ${d.vendor} --client-file <the downloaded client_secret_….json>`);
    d.out('');
    d.out(`RichOS reads the client id and the secret out of that file, keeps the secret in the OS`);
    d.out(`keychain (service ${tm.service}), and writes it to no file. You can delete the download afterwards.`);
    return { exitCode: 1 };
  } else if (account.confidentialClient && !hasStoredClientSecret(d.profile, account, backend, d)) {
    // The config says confidential and the keychain holds nothing — so the exchange would be sent
    // without the credential the registration demands, and Entra would answer with a code about a
    // missing secret while this machine believed it had one.
    d.out('');
    d.out(`NOT CONNECTED — your config records this ${d.words.label} app as a confidential client, but no client`);
    d.out(`secret is in the keychain (service ${tm.service}).`);
    d.out('');
    d.out(`  richos-service workspace connect ${d.vendor} --client-id ${account.clientId} --account ${account.accountId} --client-secret <the value>`);
    d.out('');
    d.out('RichOS keeps it in the OS keychain and writes it to no file. If the app is actually a public');
    d.out(`client, delete "confidentialClient" from ${d.clientConfigFile} and run connect again.`);
    return { exitCode: 1 };
  }
  d.out('');

  // The listener is bound BEFORE the browser is opened — the same ordering discipline Drive's first
  // sync uses. A browser that arrives before the port is listening is a failed consent the CEO has
  // to repeat; the reverse costs nothing.
  let settleRedirect;
  const redirectReady = new Promise((r) => { settleRedirect = r; });
  const state = d.makeState();
  const pkce = d.pkce();

  const codePromise = d.awaitCode({
    redirectUri: account.redirectUri,
    state,
    ...(d.timeoutMs === undefined ? {} : { timeoutMs: d.timeoutMs }),
    onListening: (info) => settleRedirect(info.url),
  });

  let effectiveRedirect = account.redirectUri;
  let code;
  try {
    effectiveRedirect = await Promise.race([
      redirectReady,
      codePromise.then(() => account.redirectUri, () => account.redirectUri),
    ]);
    const authUrl = d.profile.authUrl({
      account, scopes, redirectUri: effectiveRedirect, challenge: pkce.challenge, state,
    });
    const opened = await d.openBrowser(authUrl);
    d.out(opened
      ? `Opened ${d.words.label}'s consent screen in your browser. Approve it there and come back.`
      : 'Could not open a browser. Open this URL yourself:');
    if (!opened) {
      d.onBrowserUnavailable?.(authUrl);
      d.out('');
      d.out(`  ${authUrl}`);
      d.out('');
    }
    d.out(d.profile.consentNotice);
    ({ code } = await codePromise);
    // Between the CEO's click and `CONNECTED` sits a token exchange and an identity probe — both
    // network round trips with nothing printed in between until now. Say so the moment the code is
    // in hand, so a wait here reads as progress rather than as the same hang this ceremony just had
    // its loopback leg fixed for.
    d.out('consent received — exchanging…');
  } catch (err) {
    d.out('');
    d.out(`NOT CONNECTED — ${String(err.message || err)}`);
    return { exitCode: 1 };
  }

  let tokens;
  try {
    tokens = await d.profile.exchange({
      account, tm, redirectUri: effectiveRedirect, scopes, code, verifier: pkce.verifier, d,
    });
  } catch (err) {
    d.out('');
    // The AADSTS code is quoted explicitly when the vendor gave one: Microsoft localizes the
    // description and does not localize the number, so the number is the string a support article is
    // keyed on and the one worth putting in front of the CEO.
    const detail = String(err.message || err);
    const code = err && err.aadsts;
    d.out(`NOT CONNECTED — ${d.words.label} refused the token exchange: ${detail}${code && !detail.includes(code) ? ` (${code})` : ''}`);
    // THE ONE REFUSAL THAT NAMES A ONE-LINE FIX. Entra answers a public-client exchange with an
    // AADSTS code when the registration is not actually registered as public — and that is a switch,
    // not a code change. The CEO has the portal open two steps behind him; telling him which switch
    // costs him thirty seconds, and not telling him costs a night (the Google side spent one).
    if (d.vendor === 'microsoft' && wantsClientAuth(err)) {
      d.out('');
      d.out('That code means Entra is treating your app registration as a CONFIDENTIAL client, so it wants');
      d.out('client authentication RichOS deliberately does not hold. The one-line fix is Step 4 of');
      d.out(`${d.profile.guide}:`);
      d.out('');
      d.out('  Entra → your app → Authentication → Advanced settings → "Allow public client flows" = Yes → Save');
      d.out('');
      d.out('Then run this command again. You do not need to create a secret, and nothing else changes.');
      d.out('');
      d.out('If your organization forbids public client flows, that is the other legitimate answer, and');
      d.out('RichOS takes it deliberately rather than by inference:');
      d.out('');
      d.out(`  richos-service workspace connect ${d.vendor} --account ${account.accountId} --client-secret <the value>`);
      d.out('');
      d.out('which records the app as confidential and keeps the secret in the OS keychain, in no file.');
    }
    return { exitCode: 1 };
  }

  if (!tokens.refresh_token) {
    // Without the durable secret there is nothing to poll with tomorrow. Say so now, loudly.
    d.out('');
    d.out(`NOT CONNECTED — ${d.words.label} returned an access token but no refresh token, so RichOS could only`);
    d.out('read your calendar for the next hour and then go silent. Remove RichOS at');
    d.out(`${d.profile.consentUrl} and run connect again to force a fresh consent.`);
    return { exitCode: 1 };
  }

  // ── WHOSE CONSENT IS THIS? ─────────────────────────────────────────────────────────────────────
  // BEFORE the grant is stored, because after it is stored the damage is already done: on 2026-09-17
  // two consents given minutes apart were filed under each other's addresses, and every sync
  // afterwards read the wrong account's cloud (`identity.js` has the measured output). The command
  // line cannot know which Google account was signed in in the browser — but the grant itself can be
  // ASKED, using only the read scopes it already carries.
  const granted = parseGrantedScopes(tokens.scope || scopes.join(' '));
  // The SAME client a sync polls with, given the token this exchange just returned rather than the
  // keychain's — collector-path parity, and the only token that can answer the question being asked.
  // A `getAccessToken` that closes over the fresh token is all the manager interface this needs, and
  // using the real manager instead would read the record that has not been written yet.
  const probeClient = d.profile.makeClient({ getAccessToken: async () => tokens.access_token }, d);
  const identity = await resolveGrantedIdentity({
    probes: d.profile.identityProbes || [],
    grantedScopes: granted,
    matches: scopeMatcherFor(d.vendor),
    get: (url) => probeClient.getJson(url),
  });

  if (identity.email && !sameAddress(identity.email, account.accountId)) {
    const discarded = await d.profile.discardTokens(tokens, d);
    d.out('');
    d.out(`NOT CONNECTED — that consent belongs to ${identity.email}, not ${account.accountId}.`);
    d.out('');
    d.out(`${d.words.label} was asked whose grant this is (${identity.via}, using the scopes you just approved — no extra`);
    d.out('permission), and it answered with a different address from the one this command was for. Storing');
    d.out('it under the name you typed is exactly how two accounts end up reading each other\'s mail and');
    d.out('calendars, so nothing was stored.');
    d.out('');
    d.out(`${L('discarded')}${discarded.note}`);
    d.out('');
    d.out(`To connect ${account.accountId}: sign out of ${identity.email} in your browser, or open the consent`);
    d.out('screen in a window signed in as that account, and run the same command again:');
    d.out('');
    d.out(`  richos-service workspace connect ${d.vendor} --account ${account.accountId}`);
    d.out('');
    d.out(`To connect ${identity.email} instead, run it with --account ${identity.email}.`);
    return { exitCode: 1, connected: false, identity: { email: identity.email, matched: false, via: identity.via } };
  }

  // AN ABSENCE OF EVIDENCE IS NOT A CONTRADICTION, AND IS NOT TREATED AS ONE.
  //
  // Refusing here too was the first shape of this check, and the suite caught what it costs: a grant
  // that carries only `calendar.events.readonly` — the pre-§44 width, which the registry supports
  // deliberately and reports as LIMITED — can answer no probe at all, and refusing it would turn a
  // degraded-but-working connect into a hard failure over a question nobody could answer. That is
  // RichOS deciding a narrower grant is unusable, which is the CEO's call and he made it the other
  // way. So the absence is stored WITH the grant and said out loud at connect and at every status,
  // never swallowed: `identity: NOT VERIFIED` is a line he reads, not a silence.
  const identityMeta = identity.email
    ? { email: identity.email, via: identity.via, verifiedAt: d.now() }
    : { email: null, via: null, verifiedAt: d.now(), unverified: unverifiedReason(d, identity) };

  tm.onAuthorized(tokens, { identity: identityMeta });
  const { enabled, skipped } = registryFor({ scope: granted.join(' ') }, account, tm, d);
  const health = tm.health();

  d.out('');
  d.out(`CONNECTED — ${account.accountId}`);
  // The identity line sits ABOVE the tokens line on purpose: it is the answer to "is this the right
  // account?", and that question is settled before the CEO reads anything about what was stored.
  if (identity.email) {
    d.out(`${L('identity')}${identity.email} (verified — ${d.words.label} itself says this grant is that account's, asked via ${identity.via})`);
  } else {
    d.out(`${L('identity')}NOT VERIFIED — ${identityMeta.unverified}`);
    for (const line of describeIdentityAttempts(identity)) d.out(`            ${line}`);
    d.out(`            RichOS cannot tell whose consent this is, so check that ${account.accountId} is the account you approved it with.`);
  }
  d.out(`${L('tokens')}stored in the OS keychain (service ${tm.service}, this account's own item). Nothing written to any RichOS file or server.`);
  for (const e of enabled) {
    d.out(`${L('enabled')}${e.label} — ${e.scope}`);
    if (e.degraded) d.out(`            LIMITED: ${e.degraded}`);
    // A wider grant held and deliberately not used is a fact he should see, and it is NOT a
    // degradation — saying "degraded" about it would misdescribe it in the one direction that matters.
    if (e.grantNote) d.out(`            NOTE: ${e.grantNote}`);
  }
  for (const s of skipped) d.out(`${L('skipped')}${s.label} — ${s.reason}`);
  if (health.msUntilRefreshExpiry != null) {
    d.out(`${L('expires')}${new Date(d.now() + health.msUntilRefreshExpiry).toISOString()}  (${health.state}; External+Testing apps expire after ~7 days — re-run connect to re-consent)`);
  } else if (!d.profile.publishesGrantLifetime) {
    // NOT a missing feature and never a guessed clock: Entra publishes no fixed lifetime for a public
    // client's refresh token, so a countdown here would produce a confident re-consent prompt on a day
    // nothing is wrong and silence on the day something is.
    d.out(`${L('expires')}${d.words.label} publishes no fixed lifetime for this authorization, so RichOS shows no countdown — it will ask you to sign in again when, and only when, ${d.words.label} actually refuses`);
  }
  d.out('');
  d.out(`Pull it now with:  richos-service workspace sync ${d.vendor} --once${others.length ? '   (every connected account)' : ''}`);
  if (!others.length) {
    d.out(`Add another ${d.words.account}:  richos-service workspace connect ${d.vendor} --account <the other address>`);
  }
  return {
    exitCode: 0,
    connected: true,
    account: account.accountId,
    identity: { email: identity.email, matched: Boolean(identity.email), via: identity.via },
    accounts: accountsOf(config).map((a) => a.accountId),
    enabled: enabled.map((e) => e.source),
    skipped: skipped.map((s) => s.source),
    granted,
  };
}

// =================================================================================================
// status
// =================================================================================================

/**
 * What is connected, how healthy it is, which sources run, and what the last poll actually did — for
 * EVERY configured account, each with its own expiry, its own health and its own per-source lines.
 *
 * One account that needs re-consent makes the whole command exit non-zero while the others still
 * report in full: a status that stopped at the first unhealthy account would hide the rest, and a
 * status that returned 0 because most accounts were fine would hide the one that needs him.
 */
export async function status(deps = {}) {
  const d = resolve(deps);
  const bad = checkVendor(d);
  if (bad) return bad;

  const loaded = requireClientConfig(d);
  if (loaded.exitCode) return { exitCode: loaded.exitCode };
  const config = loaded.config;

  const backendResult = resolveBackend(d);
  if (backendResult.error) {
    d.out(`cannot reach a secure token store: ${backendResult.error}`);
    return { exitCode: 1 };
  }

  const chosen = selectAccounts(config, d);
  if (chosen.exitCode) return { exitCode: chosen.exitCode };
  const views = chosen.views;

  d.out(`${L('vendor')}${d.words.label}`);
  d.out(`${L('client')}${config.clientId}`);
  if (config.tenant) d.out(`${L('tenant')}${config.tenant}`);
  d.out(`${L('config')}${d.clientConfigFile}`);
  d.out(`${L('zone')}${d.zone}`);
  if (views.length > 1) d.out(`${L('accounts')}${views.length} — ${views.map((v) => v.accountId).join(', ')}`);

  const accounts = [];
  let exitCode = 0;

  for (const account of views) {
    const tm = tokenManagerFor(account, backendResult.backend, d,
      { adoptLegacyTokens: isFirstAccount(config, account.accountId) });
    const record = tm.load();

    d.out('');
    d.out(`${L('account')}${account.accountId}`);
    // For Google, "missing" is a real fault — the exchange cannot happen without it. For Microsoft a
    // public client is the INTENDED shape, so an absent secret is reported as that rather than as a
    // lack: a status line that reads "missing" about something nothing needs sends the CEO looking.
    if (d.vendor === 'google' || account.confidentialClient) {
      d.out(`${L('secret')}${hasStoredClientSecret(d.profile, account, backendResult.backend, d) ? 'in keychain' : 'missing'}`);
    } else {
      d.out(`${L('secret')}none — a public client, which is the intended registration (PKCE is the proof)`);
    }

    if (!record) {
      d.out(`${L('auth')}NOT CONNECTED — no grant in the keychain (service ${tm.service})`);
      d.out(`Connect with:  richos-service workspace connect ${d.vendor} --account ${account.accountId}`);
      accounts.push({ accountId: account.accountId, connected: false, health: 'no-consent', enabled: [], skipped: [] });
      exitCode = 1;
      continue;
    }

    const health = tm.health();
    const hours = health.msUntilRefreshExpiry == null ? null : Math.max(0, Math.round(health.msUntilRefreshExpiry / 3600000));
    d.out(`${L('auth')}${health.state.toUpperCase()}${hours == null ? '' : ` — ~${hours}h left on the grant`}`);
    d.out(`            ${health.message}`);

    // WHOSE grant this is, as the vendor itself answered at connect. Three states, and the third one
    // is the reason this line exists: a grant stored before the check existed carries no proof, and
    // the two grants on the CEO's own machine on 2026-09-17 were stored under each other's names.
    const proven = record.identity && record.identity.email ? record.identity : null;
    if (proven && sameAddress(proven.email, account.accountId)) {
      d.out(`${L('identity')}${proven.email} (verified at connect, via ${proven.via})`);
    } else if (proven) {
      d.out(`${L('identity')}WRONG ACCOUNT — this keychain item is filed under ${account.accountId} but the grant in it`);
      d.out(`            belongs to ${proven.email}. Everything it syncs is the other account's. Run:`);
      d.out(`              richos-service workspace connect ${d.vendor} --account ${account.accountId}`);
      exitCode = 1;
    } else if (record.identity) {
      d.out(`${L('identity')}NOT VERIFIED — ${record.identity.unverified || 'the grant could not be asked whose it is'}`);
    } else {
      d.out(`${L('identity')}not verified — this grant predates the identity check, so RichOS cannot say whose it is.`);
      d.out(`            Re-run \`workspace connect ${d.vendor} --account ${account.accountId}\` and it will be proven at consent.`);
    }

    const { enabled, skipped, granted } = registryFor(record, account, tm, d);
    d.out(`${L('granted')}${granted.length} scope${granted.length === 1 ? '' : 's'}`);
    for (const e of enabled) {
      const cursor = getSyncState(d.vendor, e.source, d.syncStateFile, e.adapter.sourceInstanceId);
      const run = getRunState(d.vendor, e.source, e.adapter.sourceInstanceId, d.runStateFile);
      d.out(`${L(e.label.toLowerCase())}ON — ${cursor ? 'delta cursor stored (next poll is incremental)' : 'no cursor yet (next poll is a bounded full sync)'}`);
      if (e.degraded) d.out(`            LIMITED: ${e.degraded}`);
      if (e.grantNote) d.out(`            NOTE: ${e.grantNote}`);
      d.out(`            last sync: ${describeRun(run)}`);
    }
    for (const s of skipped) d.out(`${L(s.label.toLowerCase())}off — ${s.reason}`);

    if (health.needsReauth) exitCode = 1;
    accounts.push({
      accountId: account.accountId,
      connected: true,
      health: health.state,
      identity: proven ? { email: proven.email, via: proven.via, matched: sameAddress(proven.email, account.accountId) } : null,
      enabled: enabled.map((e) => e.source),
      skipped: skipped.map((s) => s.source),
    });
  }

  if (!accounts.length) {
    d.out(`${L('accounts')}none configured — richos-service workspace connect ${d.vendor} --account you@yourcompany.com`);
    return { exitCode: 1, connected: false, accounts: [] };
  }

  // The top-level fields describe the WHOLE run rather than one account: connected only when every
  // account is, health the worst any account reports, enabled the union of what would poll.
  return {
    exitCode,
    connected: accounts.every((a) => a.connected),
    health: worstHealth(accounts.map((a) => a.health)),
    enabled: [...new Set(accounts.flatMap((a) => a.enabled))],
    skipped: [...new Set(accounts.flatMap((a) => a.skipped))],
    accounts,
  };
}

// =================================================================================================
// sync
// =================================================================================================

/**
 * ONE ingest pass per granted source. `--once` is the only mode, and that is a decision rather than an
 * omission: a scheduled poller is a daemon with a lifecycle, a launch agent and a failure mode of its
 * own, and switching one on for the CEO's calendar is his call to make, not a flag's. So `--daemon`
 * and friends are REFUSED by name instead of being quietly ignored.
 */
export async function sync(deps = {}) {
  const d = resolve(deps);
  const bad = checkVendor(d);
  if (bad) return bad;

  const loaded = requireClientConfig(d);
  if (loaded.exitCode) return { exitCode: loaded.exitCode };
  const config = loaded.config;

  const backendResult = resolveBackend(d);
  if (backendResult.error) {
    d.out(`cannot reach a secure token store: ${backendResult.error}`);
    return { exitCode: 1 };
  }

  const chosen = selectAccounts(config, d);
  if (chosen.exitCode) return { exitCode: chosen.exitCode };
  const views = chosen.views;
  const many = views.length > 1;

  const results = [];
  const reconnectAccounts = new Set();
  const calendarInstances = [];
  let failures = 0;
  let polled = false;
  let refused = 0; // accounts that could not be polled at all — unconnected, expired, nothing granted

  for (const account of views) {
    // An account is polled, or it is refused BY NAME and the run carries on to the next. Stopping
    // the whole sync at the first unconnected account would take the CEO's working account down with
    // the one that needs a re-consent; returning 0 because the rest were fine would hide it.
    if (many) {
      d.out('');
      d.out(`${L('account')}${account.accountId}`);
    }

    const tm = tokenManagerFor(account, backendResult.backend, d,
      { adoptLegacyTokens: isFirstAccount(config, account.accountId) });
    const record = tm.load();
    if (!record) {
      if (account.sources.includes('calendar')) reconnectAccounts.add(account.accountId);
      d.out(`NOT CONNECTED — no grant in the keychain. Run: richos-service workspace connect ${d.vendor}${many ? ` --account ${account.accountId}` : ''}`);
      refused += 1;
      continue;
    }

    const health = tm.health();
    if (health.needsReauth) {
      // NEVER-SILENT: do not poll, do not half-poll, do not log a warning and carry on.
      if (account.sources.includes('calendar')) reconnectAccounts.add(account.accountId);
      d.out(`AUTH — ${health.message}`);
      d.out(`Re-consent with:  richos-service workspace connect ${d.vendor}${many ? ` --account ${account.accountId}` : ''}`);
      refused += 1;
      continue;
    }
    if (health.state === 'refresh-expiring-soon') d.out(`WARNING — ${health.message}`);

    const { enabled, skipped } = registryFor(record, account, tm, d);
    for (const s of skipped) d.out(`${L('skipped')}${s.label} — ${s.reason}`);
    for (const e of enabled) {
      // A source running narrower than it could is said out loud on every pull, not only in `status`:
      // the counts below look identical either way, and that is exactly how a silent downgrade hides.
      if (e.degraded) d.out(`${L('limited')}${e.label} — ${e.degraded}`);
      if (e.grantNote) d.out(`${L('note')}${e.label} — ${e.grantNote}`);
    }
    if (!enabled.length) {
      d.out('nothing to sync: the grant enables no source RichOS has an adapter for.');
      refused += 1;
      continue;
    }

    const identity = identityFrom(account);
    polled = true;

    for (const e of enabled) {
      let summary = null;
      let error = null;
      let unavailable = null;
      try {
        // eslint-disable-next-line no-await-in-loop
        summary = await ingestOnce({
          adapter: e.adapter,
          identity,
          tokenManager: tm,
          zone: d.zone,
          now: d.now,
          ...(d.linkBase ? { repoRoot: d.linkBase } : {}),
        });
      } catch (err) {
        if (e.source === 'calendar' && err?.needsReauth) reconnectAccounts.add(account.accountId);
        // A stated account condition (`err.unavailable`, e.g. Gmail's "no mailbox on this Google
        // account") is not a failure this loop reports as one: the vocabulary is generic here — any
        // adapter can raise it — so this file never needs to know which vendor's error shape it is.
        // It is per ACCOUNT as much as per source: the CEO's first Google account has no mailbox and
        // his second does, and the same Gmail adapter is right about both on the same run.
        if (err && err.unavailable) {
          unavailable = String(err.reason || err.message || err);
        } else {
          error = String(err.message || err);
          failures += 1;
        }
      }
      recordRun({
        vendor: d.vendor, source: e.source, instance: e.adapter.sourceInstanceId,
        at: d.now(), summary, error, unavailable,
      }, d.runStateFile);

      if (unavailable) {
        d.out(`${L(e.label.toLowerCase())}unavailable — ${unavailable}`);
      } else if (error) {
        d.out(`${L(e.label.toLowerCase())}FAILED — ${error}`);
      } else {
        d.out(`${L(e.label.toLowerCase())}observed ${summary.observed}, ingested ${summary.ingested}, deduped ${summary.deduped}`
          + (summary.quarantined ? `, quarantined ${summary.quarantined}` : '')
          + (summary.resynced ? '  (cursor was lost — bounded full resync, deduped by the ledger)' : ''));
        // A source that can span more than one sub-resource (Calendar: which calendars) says which
        // ones it actually read, by name and count, so a zero above can be explained at a glance
        // rather than read as "nothing anywhere" (§"a person has several calendars").
        if (Array.isArray(summary.calendars) && summary.calendars.length) {
          d.out(`            calendars: ${summary.calendars.map((c) => `${c.label} ${c.count}`).join(', ')}`);
        }
        // A copy of something already in the zone, recognized across polls. Said out loud rather than
        // folded into `deduped`: "the same item again" and "the same meeting under a second id on
        // another calendar" are two different things to have happened, and a reader who sees a lower
        // `ingested` than the calendars line led him to expect deserves the reason.
        if (summary.mergedDuplicates) {
          d.out(`            merged: ${summary.mergedDuplicates} `
            + `${summary.mergedDuplicates === 1 ? 'copy' : 'copies'} of an event already in the zone `
            + 'under another calendar\'s id — one meeting, one memory');
        }
        if (summary.degraded) d.out(`            LIMITED: ${summary.degraded}`);
        if (summary.events.length || summary.commitments.length || summary.entityCandidates.length) {
          d.out(`            candidates: ${summary.events.length} event, ${summary.commitments.length} commitment, ${summary.entityCandidates.length} entity`);
        }
      }
      if (e.source === 'calendar' && !reconnectAccounts.has(account.accountId)) {
        calendarInstances.push(e.adapter.sourceInstanceId);
      }
      results.push({ account: account.accountId, source: e.source, summary, error, unavailable });
    }
  }

  if (many) d.out('');
  d.out(`${L('evidence')}${d.zone}`);

  // ── PROMOTION (§4.4 step 4) — ONCE PER SYNC RUN, NOT ONCE PER ACCOUNT ──────────────────────────
  // The pull is not the point; being able to ANSWER from what was pulled is. Until this call existed
  // a sync ended with evidence on disk and loro still unable to say what happened on Tuesday.
  //
  // It runs after EVERY account has been polled, and exactly once, because `promoteFromEvidence`
  // reads the ZONE — which holds every account's and every vendor's evidence. Inside the loop it
  // would redo all of it per account, which is why the engineer who wired multi-account left it out
  // rather than putting it in the wrong place.
  //
  // Gated on `polled`: a run that reached nothing has nothing new to promote, and a promotion line on
  // a run that never reached the vendor would read as progress that did not happen.
  let promotion = null;
  if (!d.promote) {
    d.out(`${L('promoted')}skipped — --no-promote, so this was a diagnostic pull: the evidence is stored and`);
    d.out("            loro's memory was NOT updated. Run sync again without the flag to promote it.");
  } else if (polled) {
    promotion = await runPromotion({ zone: d.zone, now: d.now, ...(d.loroDir ? { loroDir: d.loroDir } : {}) });
    for (const line of describePromotion(promotion, L)) d.out(line);
  }

  const calendarRecovery = [...reconnectAccounts];
  if (!polled) return { exitCode: 1, polled: false, results, promotion, calendarRecovery, calendarInstances };
  // An account that could not be polled is a non-zero exit even when every account that DID poll
  // succeeded — the CEO asked for all of them. A promotion that FAILED is the same class of problem
  // as a source that failed: the pull worked and the memory it exists to build did not get written.
  const promotionFailed = Boolean(promotion && promotion.failed && promotion.failed.length);
  return {
    exitCode: (failures || promotionFailed) ? 2 : (refused ? 1 : 0),
    polled: true,
    results,
    promotion,
    calendarRecovery,
    calendarInstances,
  };
}

// =================================================================================================
// disconnect
// =================================================================================================

/**
 * Revoke vendor-side (best-effort), then delete the local grant — for ONE account.
 * `--forget-cursors` also forgets where that account got to.
 *
 * With more than one account configured this REFUSES without `--account` and lists them, rather than
 * picking. Every other command here either names its account or runs all of them; this one destroys
 * something, so a default would be a guess with no undo. With exactly one account there is nothing to
 * guess, and the command works exactly as it always has.
 */
export async function disconnect(deps = {}) {
  const d = resolve(deps);
  const bad = checkVendor(d);
  if (bad) return bad;

  const loaded = requireClientConfig(d);
  if (loaded.exitCode) return { exitCode: loaded.exitCode };
  const config = loaded.config;

  const backendResult = resolveBackend(d);
  if (backendResult.error) {
    d.out(`cannot reach a secure token store: ${backendResult.error}`);
    return { exitCode: 1 };
  }

  const chosen = selectAccounts(config, d, { one: true, verb: 'disconnect' });
  if (chosen.exitCode) return { exitCode: chosen.exitCode };
  const account = chosen.views[0];

  const tm = tokenManagerFor(account, backendResult.backend, d,
    { adoptLegacyTokens: isFirstAccount(config, account.accountId) });
  if (!tm.load()) {
    d.out(`nothing to disconnect — no grant in the keychain for ${account.accountId} (service ${tm.service}).`);
    return { exitCode: 0, disconnected: false, account: account.accountId };
  }

  // The instance ids BEFORE the grant goes, because they come from the adapters and the adapters are
  // built from this account's address — which the revoke does not change, but the order makes the
  // dependency obvious to whoever reads this next.
  const instances = instanceIdsFor(account, tm, d);

  const outcome = await tm.disconnect();
  d.out(`DISCONNECTED — ${account.accountId}`);
  // WHAT ACTUALLY HAPPENED, PER VENDOR, AND NEVER A SUCCESS-SHAPED SENTENCE FOR A STEP THAT DID NOT
  // RUN. Google publishes a revoke endpoint and this really did call it. Entra publishes NONE — its
  // discovery document has no `revocation_endpoint` at all — so there is no request to make and the
  // consent record in the CEO's account is still standing. Printing Google's line here would be
  // telling him RichOS revoked something it has no way to revoke, which is the precise failure the
  // never-silent posture exists to prevent. The local deletion is a real guarantee on both: no
  // token, no access from this machine.
  if (d.profile.revokesVendorSide) {
    d.out(`${L('revoked')}asked ${d.words.label} to invalidate the refresh token (best-effort; the local deletion is the guarantee)`);
  } else {
    d.out(`${L('revoked')}NOT REVOKED — ${d.words.label} gives an app no way to revoke its own grant, so RichOS did not try and is not claiming it did`);
    d.out(`            your consent record is still listed in your account: remove "RichOS" at ${outcome && outcome.revokeUrl ? outcome.revokeUrl : d.profile.consentUrl} to finish it`);
  }
  d.out(`${L('keychain')}entry removed (service ${tm.service}) — from this moment this machine cannot read your ${d.words.label} data`);
  // The GRANT is what disconnect forgets. The client secret is not part of the grant — it identifies
  // your own OAuth app the way the client id does, it opens nothing on its own now the refresh token
  // is revoked, and `_oauth_client.json` beside it is kept for exactly the same reason. Deleting it
  // would make the next connect a console trip instead of two clicks, so it stays, and says so.
  if (hasStoredClientSecret(d.profile, account, backendResult.backend, d)) {
    d.out(`${L('secret')}your client secret stays in the keychain, so reconnecting needs no flags (delete the app registration in the ${d.words.console} to retire it for good)`);
  }

  const others = accountsOf(config).filter((a) => a.accountId !== account.accountId);
  if (others.length) {
    d.out(`${L('keeping')}${others.map((a) => a.accountId).join(', ')}  (their grants and cursors are untouched)`);
  }

  if (d.forgetCursors) {
    // THIS account's cursors, not the file. Deleting the whole file was right while one account
    // existed and is another account's silent full re-pull the moment two do.
    const dropped = forgetCursorsFor(instances, d.syncStateFile) + forgetRunsFor(instances, d.runStateFile);
    d.out(`${L('cursors')}forgotten for ${account.accountId} (${dropped} record${dropped === 1 ? '' : 's'}) — reconnecting it starts with a bounded full sync`);
  } else {
    d.out(`${L('cursors')}kept, so reconnecting resumes where it stopped (--forget-cursors to drop them)`);
  }
  d.out(`${L('evidence')}kept at ${d.zone} — it is yours; delete it yourself if you want it gone`);
  d.out('');
  d.out(`Reconnect this account with:  richos-service workspace connect ${d.vendor} --account ${account.accountId}`);
  d.out(`You can ${d.profile.revokesVendorSide ? 'also revoke' : 'remove the consent record'} at ${d.profile.consentUrl} at any time.`);
  return {
    exitCode: 0,
    disconnected: true,
    account: account.accountId,
    // The claim itself, returned rather than only printed, so a caller (and the suite) can assert
    // that `disconnect microsoft` never reports a revocation.
    vendorSideRevoked: Boolean(d.profile.revokesVendorSide),
  };
}

// =================================================================================================
// repair
// =================================================================================================

/**
 * UNDO ONE SYNC RUN — the command that exists because one crossed-consent run already happened
 * before `connect` could refuse it (`identity.js`, `repair.js`).
 *
 * VENDOR-FREE ON PURPOSE. The evidence zone, the ingest ledger and the cursor file hold every
 * vendor's and every account's work in one place, and a run is identified by WHEN it wrote — so
 * taking a vendor here would invite `repair google` to look like it repaired only Google's rows,
 * which is not something the ledger can express.
 *
 * DRY RUN IS THE DEFAULT, and `--apply` is the only way past it: a window is a claim about a run,
 * and the only way to check a claim about a run is to read what acting on it would do.
 */
export async function repair(deps = {}) {
  const d = resolve(deps);

  if (d.apply && d.dryRun) {
    d.out('--apply and --dry-run are opposite instructions, so RichOS will not pick one. Run it with neither');
    d.out('(which reports and changes nothing), or with --apply.');
    return { exitCode: 1 };
  }
  if (!d.since) {
    d.out('workspace repair undoes ONE sync run, and a run is identified by the window it wrote in.');
    d.out('');
    d.out('  richos-service workspace repair --since <ISO instant> [--until <ISO instant>] [--apply]');
    d.out('');
    d.out('Take the window from the run\'s own output. Without --apply it reports exactly what it would do');
    d.out('and changes nothing.');
    return { exitCode: 1 };
  }

  let plan;
  try {
    plan = planRepair({
      zone: d.zone,
      since: parseInstant(d.since, 'since'),
      ...(d.until ? { until: parseInstant(d.until, 'until') } : {}),
      now: d.now,
    });
  } catch (err) {
    d.out(String(err.message || err));
    return { exitCode: 1 };
  }

  const dryRun = !d.apply;
  const nothing = !plan.ledger.removing.length && !plan.cursors.resetting.length;
  if (nothing && !plan.refusals.length) {
    d.out(`${L('zone')}${d.zone}`);
    d.out(`${L('window')}${new Date(plan.since).toISOString()}  ..  ${new Date(plan.until).toISOString()}`);
    d.out('');
    d.out('Nothing was ingested and no cursor moved inside that window, so there is nothing to undo.');
    d.out('If you expected rows here, check the window against the run\'s own output — the times in it are UTC.');
    return { exitCode: 0, dryRun, plan, nothing: true };
  }

  const done = dryRun ? null : applyRepair(plan, { now: d.now });
  for (const line of describeRepair(plan, L, { dryRun, ...(done ? { done } : {}) })) d.out(line);

  if (!dryRun) {
    d.out('');
    d.out(`Next: richos-service workspace sync ${d.vendor} --once   (a bounded full sync, deduped by the repaired ledger)`);
  }
  // A refusal inside an otherwise-successful repair is not a success: the run did what it could and
  // says what it would not touch, and the exit code carries that so a script cannot miss it.
  const exitCode = plan.refusals.length || (done && done.errors.length) ? 1 : 0;
  return { exitCode, dryRun, plan, ...(done ? { done } : {}) };
}

// =================================================================================================
// seed-calendar — A TEST TOOL, with its own consent
// =================================================================================================

/**
 * Plant a designed, reproducible test calendar on ONE account — and hold the write consent that does
 * in a keychain item the sync path cannot read (`calendar-seed.js` is the whole argument).
 *
 * It lives beside the four product commands rather than inside them because everything ABOVE this
 * line is the CEO's read-only source and this is a fixture generator for it. What it shares with them
 * is the plumbing that must not be duplicated: the client config, the account list, the secure store.
 * What it does not share is the grant.
 */
export async function seedCalendar(deps = {}) {
  const d = resolve(deps);
  const bad = checkVendor(d);
  if (bad) return bad;
  if (d.vendor !== 'google') {
    // Not an oversight to be filled in later: the fixtures are Google Calendar event resources, and
    // Graph's write surface is a different API with a different consent. Saying so is the honest
    // answer; quietly seeding Google because the vendor argument was ignored is not.
    d.out(`seed-calendar is a Google Calendar tool — there is no ${d.words.label} fixture set.`);
    return { exitCode: 1 };
  }

  const loaded = requireClientConfig(d);
  if (loaded.exitCode) return { exitCode: loaded.exitCode };
  const config = loaded.config;

  const backendResult = resolveBackend(d);
  if (backendResult.error) {
    d.out(`cannot reach a secure token store: ${backendResult.error}`);
    return { exitCode: 1 };
  }

  // ONE account per run, exactly like `connect`: a consent screen belongs to one account, and so
  // does a calendar full of fixtures.
  const chosen = selectAccounts(config, d, { one: true, verb: 'seed' });
  if (chosen.exitCode) return { exitCode: chosen.exitCode };

  return runCalendarSeed({ d, account: chosen.views[0], backend: backendResult.backend });
}

// =================================================================================================
// dispatch + the line `doctor` prints
// =================================================================================================

export const USAGE = [
  '  richos-service workspace connect google [--client-file <client_secret_….json>] [--client-id <id>] [--account you@co.com] [--source calendar --source drive --source mail]',
  '                                                                          # run it again with a different --account to ADD a second Google account',
  '  richos-service workspace connect microsoft --client-id <application (client) id> --tenant <directory (tenant) id|consumers> --account you@co.com [--source calendar --source drive --source mail]',
  '                                                                          # the two ids are Step 3 of the Microsoft 365 setup guide; no secret — it is a public client',
  '  richos-service workspace status [google|microsoft] [--account you@co.com]         # every account of that vendor unless one is named',
  '  richos-service workspace sync [google|microsoft] [--once] [--account you@co.com] [--source calendar] [--no-promote]',
  '                                                                          # --once is the only mode: no daemon. Promotes what it pulled into loro memory unless --no-promote',
  '  richos-service workspace disconnect google|microsoft --account you@co.com [--forget-cursors]',
  '  richos-service workspace repair --since <ISO instant> [--until <ISO instant>] [--apply]',
  '                                                                          # undo ONE sync run: its ledger rows, its evidence and the cursors it moved.',
  '                                                                          # Reports and changes nothing unless --apply. Vendor-free: one zone holds them all.',
  '  richos-service workspace seed-calendar --account you@co.com [--set <id>] [--dry-run] [--teardown]',
  '                                                                          # A TEST TOOL: plants a designed, reproducible fixture calendar. It asks for its OWN',
  '                                                                          # write consent, kept in its own keychain item — your read-only grant is untouched.',
  '                                                                          # --dry-run prints the expected promotion outcome per fixture and writes nothing.',
  '',
  '  The two vendors are separate everywhere: separate consent, separate client config file, separate',
  '  keychain entries, separate cursors. Connect either or both; disconnecting one touches nothing of the other.',
].join('\n');

/** Flags that would mean "keep running" — refused by name, because that is a decision, not a flag. */
const SCHEDULER_FLAGS = ['daemon', 'watch', 'every', 'interval', 'forever', 'poll'];

/**
 * The dispatcher the CLI calls, and the one the suite drives end to end.
 * @param {{sub:string, deps?:object}} args
 * @returns {Promise<{exitCode:number}>}
 */
export async function runWorkspace(args) {
  const deps = args.deps || {};
  const out = deps.out || ((line) => console.log(line));
  const sub = args.sub;

  if (!sub || sub === 'help' || sub === '--help') {
    out('usage:');
    out(USAGE);
    return { exitCode: sub ? 0 : 1 };
  }
  for (const f of args.schedulerFlags || []) {
    if (SCHEDULER_FLAGS.includes(f)) {
      out(`"--${f}" is not a thing RichOS does. \`workspace sync\` runs exactly one pass.`);
      out('A background poller is a daemon with its own lifecycle and failure modes — switching one on');
      out('for your calendar is your decision to make deliberately, not a flag on a sync command.');
      return { exitCode: 1 };
    }
  }

  switch (sub) {
    case 'connect': return connect(deps);
    case 'status': return status(deps);
    case 'sync': return sync(deps);
    case 'disconnect': return disconnect(deps);
    case 'repair': return repair(deps);
    case 'seed-calendar': return seedCalendar(deps);
    default:
      out(`unknown workspace command "${sub}".`);
      out('usage:');
      out(USAGE);
      return { exitCode: 1 };
  }
}

/**
 * One line for `doctor`. Read-only and failure-tolerant: the Workspace source is optional, so nothing
 * here may turn a healthy transcription toolchain into a red doctor.
 * @returns {string}
 */
export function doctorLine(deps = {}) {
  let d;
  try {
    d = resolve(deps);
  } catch (err) {
    return `unavailable — ${String(err.message || err)}`;
  }
  if (!d.profile) return `unavailable — "${d.vendor}" is not a Workspace vendor RichOS can connect`;
  let config;
  try {
    const raw = loadClientConfig(d.clientConfigFile);
    if (!raw) return `not set up — run \`richos-service workspace connect ${d.vendor}\` (needs your own OAuth client first)`;
    const check = validateClientConfig(raw, { vendor: d.vendor });
    if (!check.ok) return `config needs a fix — ${check.problems[0]}`;
    config = check.config;
  } catch (err) {
    return `config unreadable — ${String(err.message || err)}`;
  }
  const backendResult = resolveBackend(d);
  if (backendResult.error) return `no secure token store — ${backendResult.error}`;
  try {
    // One clause per account, joined — so a machine with two accounts says so on doctor's one line
    // rather than reporting whichever account happened to be first and being silent about the other.
    const views = accountsOf(config).map((a) => accountView(config, a.accountId, { vendor: d.vendor }));
    if (!views.length) return `not set up — run \`richos-service workspace connect ${d.vendor} --account you@yourcompany.com\``;
    const clauses = views.map((account) => {
      const tm = tokenManagerFor(account, backendResult.backend, d,
        { adoptLegacyTokens: isFirstAccount(config, account.accountId) });
      const record = tm.load();
      if (!record) return `${account.accountId} — not connected`;
      const health = tm.health();
      const record2 = record;
      // Through the SAME registry `status` and `sync` build from, so doctor's one line cannot disagree
      // with what a sync would actually poll — a display fed by its own second opinion is a wrong
      // number waiting for a release. It also means Microsoft's short-form grant matches here too: a
      // literal `granted.includes(scope)` would report "no source enabled" for every Entra grant.
      const { enabled } = registryFor(record2, account, tm, d);
      const on = enabled.map((e) => e.label);
      return `${account.accountId} — ${health.state}${on.length ? `, ${on.join(' + ')}` : ', no source enabled'}`;
    });
    if (clauses.length === 1 && clauses[0].endsWith('not connected')) {
      return `not connected — run \`richos-service workspace connect ${d.vendor}\``;
    }
    return clauses.join('; ');
  } catch (err) {
    return `unavailable — ${String(err.message || err)}`;
  }
}
