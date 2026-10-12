#!/usr/bin/env node
import { validateStorage } from '../../cli/machine-settings.mjs';
// Ad hoc signing for a phone build with PRODUCTION push, through the App Store Connect API only.
//
//   node Tools/adhoc-signing.ts --bundle dev.richos.connect.perf --device <hardware UDID> --team <TEAM> \
//     --out <output directory> [--env-file <testflight env>]
//
// A development-signed build registers a SANDBOX push token, so it never tests the push path the store
// build uses (Quint's App Review rehearsal, 2026-10-04: reply notifications had never been seen working
// on the production path). An ad hoc (distribution) profile carries aps-environment `production`, as an
// App Store build does, and still installs on a registered test phone. This makes sure of everything that
// needs, and nothing else:
//
//   1. the App IDs exist: the app and the two extensions it embeds (`.notification-service`, `.share`),
//      registered if missing;
//   2. the app's App ID has Push Notifications (the extensions need no capability: their only entitlement
//      is the team's own Keychain group);
//   3. an Apple Distribution certificate whose private key is in this Mac's keychain (matched by the
//      certificate's SHA-1, the hash `security find-identity` prints), never created here;
//   4. the phone is a registered, enabled device;
//   5. one ad hoc profile per App ID, named `RichOS ad hoc <identifier>`: reused when Apple says it is
//      ACTIVE and its own content names that App ID, that certificate and that phone (and, for the app,
//      aps-environment `production`) for at least another day; otherwise deleted and made again.
//
// Each profile is written to --out and installed where Xcode looks for profiles, so `xcodebuild
// -exportArchive` with manual signing finds it by name. Prints one JSON document (exit 0), or the step
// that failed and why (exit 1). The API key is read from the private TestFlight file (testflight.ts's
// rules: mode 600, outside every git working tree) and is never printed.
//
// Battery-check: NO. This runs on the Mac against Apple's API; nothing in the app changes.
import * as NodeChildProcess from "node:child_process";
import * as NodeCrypto from "node:crypto";
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeUtil from "node:util";

import { makeAppStoreToken, readTestFlightEnv } from "../Release/testflight.ts";

const API_ORIGIN = "https://api.appstoreconnect.apple.com";
export const EXTENSION_SUFFIXES = [".notification-service", ".share"];
export const PROFILE_PREFIX = "RichOS ad hoc ";
const DAY_MS = 86_400_000;

type Json = Record<string, unknown>;
type Resource = { id: string; type: string; attributes: Json };
export type Request = (path: string, method?: string, body?: unknown) => Promise<Json | undefined>;

/** What a profile's own signed content says (decoded by `security cms -D`). */
export type ProfileFacts = {
  uuid: string;
  name: string;
  appId: string;
  team: string;
  aps: string | null;
  devices: string[];
  certificates: string[];
  expires: string;
};

export type Deps = {
  request: Request;
  identities: () => string[];
  decode: (content: Buffer) => ProfileFacts;
  install: (uuid: string, content: Buffer, out: string) => string;
  now: () => number;
};

function data(value: Json | undefined, what: string): Resource[] {
  const items = value?.data;
  if (!Array.isArray(items)) throw new Error(`App Store Connect returned no list of ${what}.`);
  return items as Resource[];
}

function one(value: Json | undefined, what: string): Resource {
  const item = value?.data as Resource | undefined;
  if (!item || typeof item.id !== "string") throw new Error(`App Store Connect returned no ${what}.`);
  return item;
}

/** The App Store Connect requests, with the token kept out of every error. */
export function apiRequest(config: { keyId: string; issuerId: string }, privateKey: NodeCrypto.KeyObject, fetchImpl: typeof fetch = fetch): Request {
  return async (path, method = "GET", body) => {
    const url = new URL(path, API_ORIGIN);
    if (url.origin !== API_ORIGIN || !url.pathname.startsWith("/v1/")) throw new Error("Refusing an App Store Connect request to an unexpected URL.");
    const token = makeAppStoreToken(config, privateKey);
    let response: Response;
    try {
      response = await fetchImpl(url, {
        method,
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        redirect: "error",
        signal: AbortSignal.timeout(30_000),
      });
    } catch {
      throw new Error(`App Store Connect ${method} ${url.pathname} failed or timed out.`);
    }
    if (response.status === 204) return undefined;
    const result = (await response.json().catch(() => undefined)) as Json | undefined;
    if (!response.ok) {
      const errors = Array.isArray(result?.errors) ? (result.errors as Json[]) : [];
      const details = errors.map((e) => [e.code, e.title, e.detail].filter((p) => typeof p === "string").join(": ")).join("; ").replaceAll(token, "[redacted token]");
      throw new Error(`App Store Connect ${method} ${url.pathname}: HTTP ${response.status}${details ? ` (${details.slice(0, 600)})` : ""}.`);
    }
    return result;
  };
}

/** The App ID for exactly this identifier (the API's filter also matches longer identifiers), registered if absent. */
async function bundleId(request: Request, identifier: string, done: string[]) {
  const found = data(await request(`/v1/bundleIds?filter[identifier]=${encodeURIComponent(identifier)}&limit=200`), "bundle IDs")
    .find((item) => item.attributes.identifier === identifier);
  if (found) return found;
  done.push(`registered the App ID ${identifier}`);
  return one(await request("/v1/bundleIds", "POST", {
    data: { type: "bundleIds", attributes: { identifier, name: `RichOS ${identifier}`.replace(/[^A-Za-z0-9 ]/gu, " "), platform: "IOS" } },
  }), `App ID ${identifier}`);
}

/** Push Notifications on the app's App ID; true when it had to be added. */
async function ensurePush(request: Request, app: Resource) {
  const capabilities = data(await request(`/v1/bundleIds/${encodeURIComponent(app.id)}/bundleIdCapabilities`), "capabilities");
  if (capabilities.some((item) => item.attributes.capabilityType === "PUSH_NOTIFICATIONS")) return false;
  await request("/v1/bundleIdCapabilities", "POST", {
    data: {
      type: "bundleIdCapabilities",
      attributes: { capabilityType: "PUSH_NOTIFICATIONS" },
      relationships: { bundleId: { data: { type: "bundleIds", id: app.id } } },
    },
  });
  return true;
}

const sha1 = (bytes: Buffer) => NodeCrypto.createHash("sha1").update(bytes).digest("hex").toUpperCase();

/** The Apple Distribution certificate this Mac can sign with: the one whose SHA-1 is a keychain identity's. */
async function certificate(request: Request, identities: string[]) {
  const listed = data(await request("/v1/certificates?filter[certificateType]=DISTRIBUTION,IOS_DISTRIBUTION&limit=200"), "certificates");
  const have = new Set(identities.map((hash) => hash.toUpperCase()));
  const usable = listed.filter((item) => typeof item.attributes.certificateContent === "string"
    && have.has(sha1(Buffer.from(String(item.attributes.certificateContent), "base64"))));
  if (!usable.length) {
    throw new Error(`No Apple Distribution certificate of the team has its private key in this Mac's keychain (${listed.length} on the account, `
      + `${identities.length} signing identities here). Creating one needs a new private key on this Mac; not done automatically.`);
  }
  usable.sort((a, b) => String(b.attributes.expirationDate).localeCompare(String(a.attributes.expirationDate)));
  return { resource: usable[0], sha1: sha1(Buffer.from(String(usable[0].attributes.certificateContent), "base64")) };
}

async function device(request: Request, udid: string) {
  const listed = data(await request(`/v1/devices?filter[udid]=${encodeURIComponent(udid)}&limit=200`), "devices")
    .filter((item) => String(item.attributes.udid).toLowerCase() === udid.toLowerCase());
  const found = listed[0];
  if (!found) throw new Error(`The phone ${udid} is not a registered device of the team; register it (Xcode does on its first development build to it).`);
  if (found.attributes.status !== "ENABLED") throw new Error(`The phone ${udid} is registered but ${String(found.attributes.status)}.`);
  return found;
}

/** Why this profile content cannot sign this App ID for this phone, or null when it can. */
export function profileProblem(facts: ProfileFacts, want: { team: string; identifier: string; udid: string; cert: string; push: boolean; now: number }) {
  if (facts.appId !== `${want.team}.${want.identifier}`) return `it is for ${facts.appId}`;
  if (!facts.devices.some((udid) => udid.toLowerCase() === want.udid.toLowerCase())) return "it does not include the phone";
  if (!facts.certificates.includes(want.cert)) return "it does not include the distribution certificate";
  if (Date.parse(facts.expires) - want.now < DAY_MS) return `it expires ${facts.expires}`;
  if (want.push && facts.aps !== "production") return `its aps-environment is ${facts.aps ?? "absent"}, not production`;
  return null;
}

export async function ensureAdHoc(
  args: { bundle: string; udid: string; team: string; out: string },
  deps: Deps,
) {
  const done: string[] = [];
  const identifiers = [args.bundle, ...EXTENSION_SUFFIXES.map((suffix) => args.bundle + suffix)];
  const ids = [];
  for (const identifier of identifiers) ids.push(await bundleId(deps.request, identifier, done));
  if (await ensurePush(deps.request, ids[0])) done.push(`turned on Push Notifications for ${args.bundle}`);
  const cert = await certificate(deps.request, deps.identities());
  const phone = await device(deps.request, args.udid);
  const profiles: Record<string, Json> = {};
  for (const [index, identifier] of identifiers.entries()) {
    const name = PROFILE_PREFIX + identifier;
    const want = { team: args.team, identifier, udid: args.udid, cert: cert.sha1, push: index === 0, now: deps.now() };
    const existing = data(await deps.request(`/v1/profiles?filter[name]=${encodeURIComponent(name)}&limit=200`), "profiles")
      .filter((item) => item.attributes.name === name);
    let chosen: { content: Buffer; facts: ProfileFacts; how: string } | null = null;
    for (const profile of existing) {
      const content = Buffer.from(String(profile.attributes.profileContent ?? ""), "base64");
      const why = profile.attributes.profileState !== "ACTIVE" ? `Apple says ${String(profile.attributes.profileState)}`
        : profile.attributes.profileType !== "IOS_APP_ADHOC" ? `it is ${String(profile.attributes.profileType)}`
        : profileProblem(deps.decode(content), want);
      if (!why && !chosen) {
        chosen = { content, facts: deps.decode(content), how: "reused" };
        continue;
      }
      await deps.request(`/v1/profiles/${encodeURIComponent(profile.id)}`, "DELETE");
      done.push(`deleted the profile "${name}" (${why ?? "a duplicate"})`);
    }
    if (!chosen) {
      const created = one(await deps.request("/v1/profiles", "POST", {
        data: {
          type: "profiles",
          attributes: { name, profileType: "IOS_APP_ADHOC" },
          relationships: {
            bundleId: { data: { type: "bundleIds", id: ids[index].id } },
            certificates: { data: [{ type: "certificates", id: cert.resource.id }] },
            devices: { data: [{ type: "devices", id: phone.id }] },
          },
        },
      }), `profile ${name}`);
      const content = Buffer.from(String(created.attributes.profileContent ?? ""), "base64");
      const facts = deps.decode(content);
      const why = profileProblem(facts, want);
      if (why) throw new Error(`Apple made the profile "${name}", but ${why}.`);
      chosen = { content, facts, how: "created" };
      done.push(`created the profile "${name}"`);
    }
    profiles[identifier] = { name, uuid: chosen.facts.uuid, how: chosen.how, expires: chosen.facts.expires,
      aps: chosen.facts.aps, path: deps.install(chosen.facts.uuid, chosen.content, args.out) };
  }
  return {
    team: args.team,
    certificate: { id: cert.resource.id, name: cert.resource.attributes.name ?? cert.resource.attributes.displayName,
      expires: cert.resource.attributes.expirationDate, sha1: cert.sha1 },
    device: { id: phone.id, udid: phone.attributes.udid, name: phone.attributes.name },
    pushCapability: true,
    profiles,
    done,
  };
}

// The facts read from a profile's own signed plist. Python's plistlib reads its <data> and <date> values.
const FACTS = `import plistlib,sys,json,hashlib
p=plistlib.loads(sys.stdin.buffer.read());e=p.get('Entitlements',{})
print(json.dumps({'uuid':p['UUID'],'name':p['Name'],'appId':e.get('application-identifier',''),
 'team':(p.get('TeamIdentifier') or [''])[0],'aps':e.get('aps-environment'),'devices':p.get('ProvisionedDevices',[]),
 'certificates':[hashlib.sha1(c).hexdigest().upper() for c in p.get('DeveloperCertificates',[])],
 'expires':p['ExpirationDate'].strftime('%Y-%m-%dT%H:%M:%SZ')}))`;

export function decodeProfile(content: Buffer): ProfileFacts {
  const plist = NodeChildProcess.execFileSync("security", ["cms", "-D"], { input: content, maxBuffer: 16 * 1024 * 1024 });
  return JSON.parse(NodeChildProcess.execFileSync("python3", ["-c", FACTS], { input: plist, encoding: "utf8" })) as ProfileFacts;
}

/** The SHA-1 of every valid code-signing identity in this Mac's keychains (no private key leaves them). */
export function keychainIdentities(): string[] {
  const out = NodeChildProcess.execFileSync("security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" });
  return [...out.matchAll(/^\s*\d+\)\s+([0-9A-F]{40})\s/gmu)].map((m) => m[1]);
}

/** Where Xcode 16 and later look for profiles; the copy in --out is the record. */
export const XCODE_PROFILES = NodePath.join(NodeOS.homedir(), "Library/Developer/Xcode/UserData/Provisioning Profiles");
export function installProfile(uuid: string, content: Buffer, out: string, xcode = XCODE_PROFILES) {
  if (!/^[0-9A-Fa-f-]{36}$/u.test(uuid)) throw new Error(`A profile UUID ${uuid} is not a UUID.`);
  NodeFS.mkdirSync(out, { recursive: true });
  NodeFS.mkdirSync(xcode, { recursive: true });
  const kept = NodePath.join(out, `${uuid}.mobileprovision`);
  NodeFS.writeFileSync(kept, content, { mode: 0o600 });
  NodeFS.writeFileSync(NodePath.join(xcode, `${uuid}.mobileprovision`), content, { mode: 0o600 });
  return kept;
}

async function main() {
  const { values } = NodeUtil.parseArgs({
    options: { bundle: { type: "string" }, device: { type: "string" }, team: { type: "string" }, out: { type: "string" }, "env-file": { type: "string" } },
  });
  const { bundle, device: udid, team, out } = values;
  if (!bundle || !udid || !team || !out) throw new Error("Usage: adhoc-signing.ts --bundle ID --device UDID --team TEAM --out DIR [--env-file FILE]");
  if (!/^[A-Za-z0-9.-]+$/u.test(bundle)) throw new Error("--bundle must be a bundle identifier.");
  if (!/^[A-Z0-9]{10}$/u.test(team)) throw new Error("--team must be the 10-character Apple team ID.");
  validateStorage(NodePath.resolve(out));
  const envFile = values["env-file"] ?? process.env.RICHOS_IOS_TESTFLIGHT_ENV_FILE ?? NodePath.join(NodeOS.homedir(), ".config/richos/testflight.env");
  const { config, privateKey } = await readTestFlightEnv(envFile);
  const result = await ensureAdHoc({ bundle, udid, team, out: NodePath.resolve(out) }, {
    request: apiRequest(config, privateKey),
    identities: keychainIdentities,
    decode: decodeProfile,
    install: installProfile,
    now: Date.now,
  });
  NodeFS.writeFileSync(NodePath.join(out, "adhoc-signing.json"), `${JSON.stringify(result, null, 1)}\n`);
  process.stdout.write(`${JSON.stringify({ ok: true, result })}\n`);
}

if (import.meta.main) {
  main().catch((error: unknown) => {
    process.stderr.write(`${JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) })}\n`);
    process.exitCode = 1;
  });
}
