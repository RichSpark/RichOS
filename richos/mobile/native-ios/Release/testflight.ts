#!/usr/bin/env node
import { stateDir } from '../../cli/machine-settings.mjs';
// TestFlight upload, status and publish for the RichOS native iPhone app, run locally on this Mac.
//
// ADOPTED AS-IS from T3 Code (https://github.com/pingdotgg/t3code) at
// 2eb6a53343ffb4ce747617746ee85433115ad18f, `scripts/swift-testflight.ts`, MIT License,
// Copyright (c) 2026 T3 Tools Inc. (full notice: Release/third-party/T3-Code-LICENSE.txt).
// Adoption ledger §2.8 B3: "RichOS has no TestFlight tooling, and this runs as a local command on
// one Mac with no CI."
//
// What changed from T3's file, and why each change fits RichOS (§70):
//   1. Names: RICHOS_IOS_ASC_* variables and ~/.config/richos/testflight.env (mode 600) instead of
//      T3's; the archived app is RichOSNative.app.
//   2. The bundle identifier is read from the private file (RICHOS_IOS_ASC_BUNDLE_ID) instead of a
//      constant, because RichOS's production identifier is the CEO's decision and not yet made
//      (build plan §6 item 3). Every guard that compared against T3's constant compares against it.
//   3. The public (external) group is OPTIONAL. RichOS's first target is TestFlight INTERNAL testing
//      (build plan §6 item 1); with no public group configured, `publish` sets the notes, turns on
//      automatic notification and assigns the internal group, and never submits for beta review.
//      With one configured, T3's behavior is unchanged. For the same reason an export marked
//      `testFlightInternalTestingOnly` is accepted rather than refused.
//   4. The private file is refused if it sits inside a git working tree (credentials never in git),
//      in addition to T3's mode-600 rule. Credentials are never read from argv or the environment's
//      variables themselves, only from the file.
//   5. The speed gate (CEO 2026-10-03, §106: nothing reaches users without passing the speed tests):
//      upload and publish refuse without the phone speed watch's §104 PASS for the exact app code
//      (speedGate, runRelease below). Upload gates the commit the archive itself carries (stamped
//      into the archived app's Info.plist by Release/platform.yml) and records it; publish gates
//      that commit.
//   6. The review-walk gate (CEO 2026-10-04, §107: the CEO is never the first tester): upload also
//      refuses unless the automated walk of exactly what Apple's reviewer does passed on the archive's
//      stamped commit (walkGate below; richos/mobile/perf/reviewwalk.py, written by `rios review-walk`).
//      A build that never reaches App Store Connect can never be selected for review there.
//   7. The listing and App Review (CEO 2026-10-04: the listing must never differ from the record, and
//      no unproven version may be submitted). check-listing compares the live App Store listing with
//      the record; apply-listing sets it from the record; submit is the only way this Mac sends a
//      version to App Review and refuses unless the listing matches and the selected build's commit
//      has both the speed pass and the review-walk pass. None of it has a skip option.
// Nothing else is changed: no retries of writes, refusal of a second upload of an existing build,
// app and group ownership checks before any write, the key written to a private temporary file
// only for Xcode and removed after.
//
// @effect-diagnostics nodeBuiltinImport:off globalDate:off globalFetch:off preferSchemaOverJson:off - This standalone release tool uses Node APIs and validates JSON at the API boundary.
import * as NodeCrypto from "node:crypto";
import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeFSP from "node:fs/promises";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeUtil from "node:util";

const API_ORIGIN = "https://api.appstoreconnect.apple.com";
const ARCHIVED_APP = "Products/Applications/RichOSNative.app/Info.plist";
const TOKEN_SECONDS = 600;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function record(value: unknown, label: string) {
  if (!isRecord(value)) throw new Error(`Expected an object for ${label}.`);
  return value;
}

function text(value: unknown, label: string) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`Expected a nonempty string for ${label}.`);
  }
  return value;
}

function boolean(value: unknown, label: string) {
  if (typeof value !== "boolean") throw new Error(`Expected a boolean for ${label}.`);
  return value;
}

function items(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Expected a list for ${label}.`);
  return value;
}

function resource(value: unknown, type: string) {
  const data = record(value, type);
  if (data.type !== type) throw new Error(`Expected an App Store Connect ${type} resource.`);
  return {
    id: text(data.id, `${type}.id`),
    attributes: record(data.attributes ?? {}, `${type}.attributes`),
    relationships: record(data.relationships ?? {}, `${type}.relationships`),
  };
}

type Resource = ReturnType<typeof resource>;

function relationship(value: Resource, name: string, type: string) {
  const relation = record(value.relationships[name], `${name} relationship`);
  return relation.data === null ? undefined : resource(relation.data, type).id;
}

export function parseTestFlightEnv(source: string) {
  const env = NodeUtil.parseEnv(source);
  const publicGroup = env.RICHOS_IOS_ASC_PUBLIC_GROUP_ID;
  const config = {
    keyId: text(env.RICHOS_IOS_ASC_KEY_ID, "RICHOS_IOS_ASC_KEY_ID"),
    issuerId: text(env.RICHOS_IOS_ASC_ISSUER_ID, "RICHOS_IOS_ASC_ISSUER_ID"),
    appId: text(env.RICHOS_IOS_ASC_APP_ID, "RICHOS_IOS_ASC_APP_ID"),
    bundleId: text(env.RICHOS_IOS_ASC_BUNDLE_ID, "RICHOS_IOS_ASC_BUNDLE_ID"),
    publicGroupId:
      publicGroup === undefined || publicGroup.trim() === ""
        ? undefined
        : text(publicGroup, "RICHOS_IOS_ASC_PUBLIC_GROUP_ID"),
    internalGroupId: text(env.RICHOS_IOS_ASC_INTERNAL_GROUP_ID, "RICHOS_IOS_ASC_INTERNAL_GROUP_ID"),
  };
  if (!/^[A-Za-z0-9.-]+$/u.test(config.bundleId)) {
    throw new Error("RICHOS_IOS_ASC_BUNDLE_ID must be a bundle identifier.");
  }
  if (config.publicGroupId === config.internalGroupId) {
    throw new Error("Public and internal groups must be different.");
  }
  const encodedKey = text(env.RICHOS_IOS_ASC_PRIVATE_KEY_BASE64, "RICHOS_IOS_ASC_PRIVATE_KEY_BASE64");
  let privateKey: NodeCrypto.KeyObject;
  try {
    const decoded = Buffer.from(encodedKey, "base64");
    if (decoded.toString("base64") !== encodedKey) throw new Error("invalid base64");
    privateKey = NodeCrypto.createPrivateKey(decoded);
  } catch {
    throw new Error("RICHOS_IOS_ASC_PRIVATE_KEY_BASE64 must contain the base64-encoded .p8 file.");
  }
  requireAppStorePrivateKey(privateKey);
  return { config, privateKey };
}

type TestFlightConfig = ReturnType<typeof parseTestFlightEnv>["config"];
type BuildSelection = { build: string; version: string };

/// RichOS addition: a credentials file inside a git working tree is one `git add` from being
/// published. Walks up from the file's directory looking for `.git`.
export function insideGitWorkTree(path: string) {
  let directory = NodePath.dirname(NodePath.resolve(path));
  while (true) {
    if (NodeFS.existsSync(NodePath.join(directory, ".git"))) return true;
    const parent = NodePath.dirname(directory);
    if (parent === directory) return false;
    directory = parent;
  }
}

export async function readTestFlightEnv(path: string) {
  let file: NodeFSP.FileHandle | undefined;
  let source: string;
  const resolved = expandHome(path);
  if (insideGitWorkTree(resolved)) {
    throw new Error(
      `Refusing the TestFlight env file at ${path}: it is inside a git working tree. Keep it in ~/.config/richos/ (mode 600).`,
    );
  }
  try {
    file = await NodeFSP.open(resolved, "r");
    const metadata = await file.stat();
    if (!metadata.isFile() || (metadata.mode & 0o077) !== 0) {
      throw new Error("unsafe permissions");
    }
    source = await file.readFile("utf8");
  } catch {
    throw new Error(
      `Cannot read TestFlight env file at ${path}. Use a private file with mode 600.`,
    );
  } finally {
    await file?.close();
  }
  return parseTestFlightEnv(source);
}

function validateBuildSelection(selection: BuildSelection) {
  if (
    !/^\d+(?:\.\d+){0,2}$/u.test(selection.build) ||
    !/^\d+\.\d+(?:\.\d+)?$/u.test(selection.version)
  ) {
    throw new Error("Use an exact numeric build number and version.");
  }
  return selection;
}

function requireAppStorePrivateKey(privateKey: NodeCrypto.KeyObject) {
  if (
    privateKey.type !== "private" ||
    privateKey.asymmetricKeyType !== "ec" ||
    privateKey.asymmetricKeyDetails?.namedCurve !== "prime256v1"
  ) {
    throw new Error("App Store Connect requires a P-256 private key.");
  }
}

export function makeAppStoreToken(
  config: Pick<TestFlightConfig, "keyId" | "issuerId">,
  privateKey: NodeCrypto.KeyObject,
  nowSeconds = Math.floor(Date.now() / 1_000),
) {
  requireAppStorePrivateKey(privateKey);
  const header = Buffer.from(
    JSON.stringify({ alg: "ES256", kid: config.keyId, typ: "JWT" }),
  ).toString("base64url");
  const payload = Buffer.from(
    JSON.stringify({
      iss: config.issuerId,
      iat: nowSeconds,
      exp: nowSeconds + TOKEN_SECONDS,
      aud: "appstoreconnect-v1",
    }),
  ).toString("base64url");
  const input = `${header}.${payload}`;
  const signature = NodeCrypto.sign("sha256", Buffer.from(input), {
    key: privateKey,
    dsaEncoding: "ieee-p1363",
  });
  return `${input}.${signature.toString("base64url")}`;
}

/// Screenshots (CEO 2026-10-04: the corrected store screenshots must reach the listing without anyone
/// clicking them in). The local files, in file-name order, with the MD5 Apple reports as sourceFileChecksum.
export type LocalShot = { fileName: string; md5: string };
export type RemoteShot = { fileName: string; checksum: string | undefined; state: string | undefined };

export function localShots(dir: string): { fileName: string; path: string; md5: string; size: number }[] {
  const names = NodeFS.readdirSync(dir)
    .filter((name) => name.toLowerCase().endsWith(".png"))
    .sort();
  if (names.length === 0) throw new Error(`No PNG files in ${dir}.`);
  if (names.length > 10) throw new Error("An App Store screenshot set holds at most 10 screenshots.");
  return names.map((fileName) => {
    const path = NodePath.join(dir, fileName);
    const bytes = NodeFS.readFileSync(path);
    return { fileName, path, md5: NodeCrypto.createHash("md5").update(bytes).digest("hex"), size: bytes.length };
  });
}

/// Compares the listing's screenshots (in Apple's stored order) with the local files (in file-name order).
/// Returns one line per problem; an empty list means same count, same order, every checksum matching and
/// every screenshot COMPLETE.
export function compareScreenshots(local: LocalShot[], remote: RemoteShot[]) {
  const problems: string[] = [];
  if (local.length !== remote.length) {
    problems.push(`Expected ${local.length} screenshots on the listing; Apple has ${remote.length}.`);
  }
  for (let index = 0; index < Math.min(local.length, remote.length); index++) {
    const want = local[index]!;
    const have = remote[index]!;
    const position = `Position ${index + 1}`;
    if (have.state !== "COMPLETE") {
      problems.push(`${position} (${want.fileName}): Apple reports ${have.state ?? "no state"}, not COMPLETE.`);
    }
    if ((have.checksum ?? "").toLowerCase() !== want.md5.toLowerCase()) {
      problems.push(
        `${position} (${want.fileName}): Apple's checksum ${have.checksum ?? "(none)"} is not the local file's ${want.md5}.`,
      );
    }
  }
  return problems;
}

/// The listing record (CEO 2026-10-04: the App Store listing must never differ from the record). The record
/// is the JSON file kept in the private record repository (richos-hq docs/operations/*-listing-state.json);
/// these are the fields check-listing compares, by the name it prints. Name, subtitle and privacy URL live
/// on Apple's app info; the rest on the version.
export const LISTING_FIELDS = [
  "name",
  "subtitle",
  "description",
  "keywords",
  "promotionalText",
  "privacyPolicyUrl",
  "supportUrl",
] as const;
export type ListingField = (typeof LISTING_FIELDS)[number];
export type ListingText = Record<ListingField, string | null>;
const APP_INFO_FIELDS: readonly ListingField[] = ["name", "subtitle", "privacyPolicyUrl"];

export type ListingRecord = { path: string; version: string; locale: string; text: ListingText; screenshotsDir: string };

function optionalText(value: unknown, label: string) {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") throw new Error(`Expected a string or null for ${label}.`);
  return value;
}

/// Reads the record. Its `screenshotsDir` is relative to the record's own repository, so the record names
/// exactly one folder of screenshots, in its own history.
export function readListingRecord(path: string): ListingRecord {
  const resolved = NodePath.resolve(expandHome(path));
  let data: unknown;
  try {
    data = JSON.parse(NodeFS.readFileSync(resolved, "utf8"));
  } catch {
    throw new Error(`Cannot read the listing record at ${path}.`);
  }
  const root = record(data, "listing record");
  const localization = record(root.localization, "record localization");
  const copy = record(root.copy, "record copy");
  const version = record(root.version, "record version");
  const locale = text(copy.locale, "record copy.locale");
  if (localization.locale !== locale) throw new Error("The record's localization and copy name different locales.");
  const dir = text(root.screenshotsDir, "record screenshotsDir");
  if (NodePath.isAbsolute(dir) || dir.split(/[\\/]/u).includes("..")) {
    throw new Error("The record's screenshotsDir must be a path inside the record's repository.");
  }
  let repository = NodePath.dirname(resolved);
  while (!NodeFS.existsSync(NodePath.join(repository, ".git"))) {
    const parent = NodePath.dirname(repository);
    if (parent === repository) throw new Error(`The listing record ${path} is not inside a git repository.`);
    repository = parent;
  }
  return {
    path: resolved,
    version: text(version.versionString, "record version.versionString"),
    locale,
    text: {
      name: optionalText(localization.name, "record localization.name"),
      subtitle: optionalText(localization.subtitle, "record localization.subtitle"),
      description: optionalText(copy.description, "record copy.description"),
      keywords: optionalText(copy.keywords, "record copy.keywords"),
      promotionalText: optionalText(copy.promotionalText, "record copy.promotionalText"),
      privacyPolicyUrl: optionalText(localization.privacyPolicyUrl, "record localization.privacyPolicyUrl"),
      supportUrl: optionalText(copy.supportUrl, "record copy.supportUrl"),
    },
    screenshotsDir: NodePath.join(repository, dir),
  };
}

/// Where two texts first differ, with a little context on each side, so a long description's one changed
/// sentence is visible in a terminal.
function firstDifference(have: string, want: string) {
  let index = 0;
  while (index < have.length && index < want.length && have[index] === want[index]) index++;
  const excerpt = (value: string) =>
    JSON.stringify(value.slice(Math.max(0, index - 30), index + 50)) + (value.length > index + 50 ? "…" : "");
  return `differs at character ${index + 1}: the listing has ${excerpt(have)}; the record says ${excerpt(want)}`;
}

/// One line per mismatch, each starting with the field's name; an empty list means the listing is the record.
export function compareListing(want: ListingText, have: ListingText, local: LocalShot[], remote: RemoteShot[]) {
  const problems: string[] = [];
  for (const field of LISTING_FIELDS) {
    const wanted = want[field] ?? "";
    const actual = have[field] ?? "";
    if (wanted !== actual) problems.push(`${field}: ${firstDifference(actual, wanted)}.`);
  }
  for (const problem of compareScreenshots(local, remote)) problems.push(`screenshots: ${problem}`);
  return problems;
}

// All writes use the app and groups verified by status(). No credentials go into command output.
export function createTestFlightClient(
  config: TestFlightConfig,
  privateKey: NodeCrypto.KeyObject,
  fetchImpl: typeof fetch = fetch,
) {
  async function request(path: string, method = "GET", body?: unknown): Promise<unknown> {
    const url = new URL(path, API_ORIGIN);
    if (url.origin !== API_ORIGIN || !url.pathname.startsWith("/v1/")) {
      throw new Error("Refusing an App Store Connect request to an unexpected URL.");
    }
    const operation = `${method} ${url.pathname}`;
    const uncertainWrite =
      method === "GET" ? "" : " The write was not retried. Run status before trying again.";
    const token = makeAppStoreToken(config, privateKey);
    let response: Response;
    try {
      response = await fetchImpl(url, {
        method,
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        redirect: "error",
        signal: AbortSignal.timeout(30_000),
      });
    } catch {
      throw new Error(`App Store Connect ${operation} failed or timed out.${uncertainWrite}`);
    }
    if (response.status === 204 && response.ok) return undefined;
    const result: unknown = await response.json().catch(() => undefined);
    if (!response.ok) {
      const details =
        isRecord(result) && Array.isArray(result.errors)
          ? result.errors
              .filter(isRecord)
              .map((error) =>
                [error.code, error.title, error.detail]
                  .filter((part): part is string => typeof part === "string")
                  .join(": "),
              )
              .join("; ")
              .replaceAll(token, "[redacted token]")
          : "";
      throw new Error(
        `App Store Connect ${operation}: HTTP ${response.status}${details ? ` (${details.slice(0, 1_000)})` : ""}.${uncertainWrite}`,
      );
    }
    return result;
  }

  async function getResource(path: string, type: string) {
    const response = record(await request(path), type);
    return resource(response.data, type);
  }

  async function list(path: string, type: string) {
    const result: Resource[] = [];
    const visited = new Set<string>();
    let next: string | undefined = path;
    while (next) {
      if (visited.has(next)) throw new Error("App Store Connect repeated a page URL.");
      visited.add(next);
      const response = record(await request(next), `${type} response`);
      result.push(...items(response.data, type).map((item) => resource(item, type)));
      const links = record(response.links ?? {}, `${type} links`);
      next =
        links.next === undefined || links.next === null ? undefined : text(links.next, "next page");
    }
    return result;
  }

  async function group(id: string, internal: boolean) {
    const value = await getResource(
      `/v1/betaGroups/${encodeURIComponent(id)}?include=app`,
      "betaGroups",
    );
    if (value.id !== id || relationship(value, "app", "apps") !== config.appId) {
      throw new Error("Refusing a beta group that does not belong to the configured RichOS app.");
    }
    if (boolean(value.attributes.isInternalGroup, "isInternalGroup") !== internal) {
      throw new Error("The configured beta group has the wrong internal/external type.");
    }
    return {
      id,
      name: text(value.attributes.name, "group name"),
      internal,
      allBuilds: value.attributes.hasAccessToAllBuilds === true,
      publicLinkEnabled: value.attributes.publicLinkEnabled === true,
    };
  }

  /// The configured groups, internal first; the public group only when one is configured.
  function groups() {
    return Promise.all([
      group(config.internalGroupId, true),
      ...(config.publicGroupId ? [group(config.publicGroupId, false)] : []),
    ]);
  }

  async function verifyApp() {
    const app = await getResource(`/v1/apps/${encodeURIComponent(config.appId)}`, "apps");
    if (app.id !== config.appId || app.attributes.bundleId !== config.bundleId) {
      throw new Error(`Refusing to use an app other than ${config.bundleId}. Check appId.`);
    }
    return app;
  }

  function buildQuery(selection: BuildSelection) {
    validateBuildSelection(selection);
    return new URLSearchParams({
      "filter[app]": config.appId,
      "filter[version]": selection.build,
      "filter[preReleaseVersion.version]": selection.version,
      "filter[preReleaseVersion.platform]": "IOS",
      limit: "2",
    });
  }

  async function verifyUpload(selection: BuildSelection) {
    await Promise.all([verifyApp(), groups()]);
    const existing = await list(`/v1/builds?${buildQuery(selection)}`, "builds");
    if (existing.length > 0) {
      throw new Error(
        `Build ${selection.version} (${selection.build}) already exists. Run status; refusing another upload.`,
      );
    }
  }

  async function status(selection: BuildSelection) {
    const [app, configuredGroups] = await Promise.all([verifyApp(), groups()]);
    const query = buildQuery(selection);
    query.set("include", "app,preReleaseVersion,buildBetaDetail,betaAppReviewSubmission");
    const response = record(await request(`/v1/builds?${query}`), "builds response");
    const builds = items(response.data, "builds");
    if (builds.length !== 1) {
      throw new Error(
        `Expected one RichOS build ${selection.version} (${selection.build}); found ${builds.length}.`,
      );
    }
    const build = resource(builds[0], "builds");
    if (
      relationship(build, "app", "apps") !== app.id ||
      build.attributes.version !== selection.build
    ) {
      throw new Error("App Store Connect returned a different app or build. Nothing was changed.");
    }
    const included = items(response.included ?? [], "included resources").map((value) =>
      record(value, "included resource"),
    );
    function includedResource(name: string, type: string) {
      const id = relationship(build, name, type);
      if (!id) return undefined;
      const value = included.find((item) => item.type === type && item.id === id);
      if (!value) throw new Error(`App Store Connect omitted the build's ${name}.`);
      return resource(value, type);
    }
    const version = includedResource("preReleaseVersion", "preReleaseVersions");
    if (
      version?.attributes.version !== selection.version ||
      version.attributes.platform !== "IOS"
    ) {
      throw new Error(
        "App Store Connect returned a different version or platform. Nothing was changed.",
      );
    }
    async function optionalBuildResource(name: string, type: string) {
      const relation = build.relationships[name];
      if (isRecord(relation) && relation.data !== undefined) return includedResource(name, type);
      // A missing linkage is not proof that no review exists. Read the filtered collection.
      const query = new URLSearchParams({ "filter[build]": build.id, limit: "2" });
      const values = await list(`/v1/${type}?${query}`, type);
      if (values.length > 1)
        throw new Error(`App Store Connect returned multiple ${name} resources.`);
      return values[0];
    }
    const [detail, review] = await Promise.all([
      optionalBuildResource("buildBetaDetail", "buildBetaDetails"),
      optionalBuildResource("betaAppReviewSubmission", "betaAppReviewSubmissions"),
    ]);
    // Apple accepts one relationship filter. The build's app was checked above.
    const membershipQuery = new URLSearchParams({
      "filter[builds]": build.id,
      limit: "200",
    });
    const memberships = await list(`/v1/betaGroups?${membershipQuery}`, "betaGroups");
    const assignedGroups = configuredGroups.map((item) => ({
      ...item,
      assigned:
        memberships.some((member) => member.id === item.id) || (item.internal && item.allBuilds),
    }));
    const externalState = detail
      ? text(detail.attributes.externalBuildState, "externalBuildState")
      : undefined;
    return {
      appId: app.id,
      appName: text(app.attributes.name, "app name"),
      bundleId: config.bundleId,
      primaryLocale: text(app.attributes.primaryLocale, "primaryLocale"),
      buildId: build.id,
      ...selection,
      processingState: text(build.attributes.processingState, "processingState"),
      expired: boolean(build.attributes.expired, "expired"),
      audience: text(build.attributes.buildAudienceType, "buildAudienceType"),
      detailId: detail?.id,
      externalState,
      internalState: detail
        ? text(detail.attributes.internalBuildState, "internalBuildState")
        : undefined,
      autoNotifyEnabled: detail
        ? boolean(detail.attributes.autoNotifyEnabled, "autoNotifyEnabled")
        : false,
      reviewState: review ? text(review.attributes.betaReviewState, "betaReviewState") : undefined,
      groups: assignedGroups,
      internalTesting: assignedGroups.some((item) => item.internal && item.assigned),
      publicTesting:
        externalState === "IN_BETA_TESTING" &&
        assignedGroups.some((item) => !item.internal && item.assigned && item.publicLinkEnabled),
    };
  }

  function requirePublishable(current: Awaited<ReturnType<typeof status>>) {
    if (current.expired) throw new Error("This build has expired. Upload a new build.");
    if (current.processingState !== "VALID") {
      throw new Error(`Build processing is ${current.processingState}. Nothing was published.`);
    }
    if (!config.publicGroupId) {
      // RichOS: internal testing only. Apple needs beta details to exist before notes and groups.
      return text(current.detailId, "build beta detail ID");
    }
    if (current.audience !== "APP_STORE_ELIGIBLE") {
      throw new Error("This build is internal-only and cannot be released to the public beta.");
    }
    if (!current.groups.some((item) => !item.internal && item.publicLinkEnabled)) {
      throw new Error("The configured public beta group does not have an enabled public link.");
    }
    const allowed = [
      "READY_FOR_BETA_SUBMISSION",
      "WAITING_FOR_BETA_REVIEW",
      "IN_BETA_REVIEW",
      "BETA_APPROVED",
      "READY_FOR_BETA_TESTING",
      "IN_BETA_TESTING",
    ];
    if (
      !current.externalState ||
      !allowed.includes(current.externalState) ||
      current.reviewState === "REJECTED"
    ) {
      throw new Error(
        `Build is not ready for publication: ${current.externalState ?? "no beta details"}${current.reviewState ? ` (${current.reviewState})` : ""}.`,
      );
    }
    return text(current.detailId, "build beta detail ID");
  }

  async function publish(selection: BuildSelection, notes: string) {
    const whatsNew = notes.trim();
    if (!whatsNew || whatsNew.length > 4_000) {
      throw new Error("TestFlight notes must contain between 1 and 4,000 characters.");
    }
    let current = await status(selection);
    const detailId = requirePublishable(current);
    const localizations = await list(
      `/v1/builds/${encodeURIComponent(current.buildId)}/betaBuildLocalizations?limit=200`,
      "betaBuildLocalizations",
    );
    const localization = localizations.find(
      (item) => item.attributes.locale === current.primaryLocale,
    );
    if (localization && localization.attributes.whatsNew !== whatsNew) {
      await request(`/v1/betaBuildLocalizations/${encodeURIComponent(localization.id)}`, "PATCH", {
        data: { type: "betaBuildLocalizations", id: localization.id, attributes: { whatsNew } },
      });
    } else if (!localization) {
      await request("/v1/betaBuildLocalizations", "POST", {
        data: {
          type: "betaBuildLocalizations",
          attributes: { locale: current.primaryLocale, whatsNew },
          relationships: { build: { data: { type: "builds", id: current.buildId } } },
        },
      });
    }
    if (!current.autoNotifyEnabled) {
      await request(`/v1/buildBetaDetails/${encodeURIComponent(detailId)}`, "PATCH", {
        data: { type: "buildBetaDetails", id: detailId, attributes: { autoNotifyEnabled: true } },
      });
    }
    for (const id of [config.internalGroupId, ...(config.publicGroupId ? [config.publicGroupId] : [])]) {
      current = await status(selection);
      requirePublishable(current);
      if (current.groups.some((item) => item.id === id && item.assigned)) continue;
      await request(`/v1/betaGroups/${encodeURIComponent(id)}/relationships/builds`, "POST", {
        data: [{ type: "builds", id: current.buildId }],
      });
    }
    current = await status(selection);
    requirePublishable(current);
    if (!config.publicGroupId) return current;
    if (current.externalState === "READY_FOR_BETA_SUBMISSION" && !current.reviewState) {
      await request("/v1/betaAppReviewSubmissions", "POST", {
        data: {
          type: "betaAppReviewSubmissions",
          relationships: { build: { data: { type: "builds", id: current.buildId } } },
        },
      });
    } else if (
      current.externalState === "READY_FOR_BETA_TESTING" ||
      current.externalState === "BETA_APPROVED"
    ) {
      await request("/v1/buildBetaNotifications", "POST", {
        data: {
          type: "buildBetaNotifications",
          relationships: { build: { data: { type: "builds", id: current.buildId } } },
        },
      });
    }
    return status(selection);
  }

  /// Replaces the screenshots of the iPhone 6.9-inch set of the version being prepared. Changes nothing
  /// else: never submits, selects a build or touches testers.
  // Apple's API names the 6.7/6.9-inch iPhone family APP_IPHONE_67 (read from this listing: it has no
  // APP_IPHONE_69 set; the 1320 x 2868 images are the 6.9-inch size and live in this set).
  /// The one iOS App Store version with this version string.
  async function findVersion(version: string) {
    const versionQuery = new URLSearchParams({
      "filter[platform]": "IOS",
      "filter[versionString]": version,
      limit: "10",
    });
    const versions = await list(
      `/v1/apps/${encodeURIComponent(config.appId)}/appStoreVersions?${versionQuery}`,
      "appStoreVersions",
    );
    if (versions.length !== 1) throw new Error(`Expected one iOS version ${version}; found ${versions.length}.`);
    return versions[0]!;
  }

  async function versionLocalization(versionId: string, version: string, locale: string) {
    const localizations = await list(
      `/v1/appStoreVersions/${encodeURIComponent(versionId)}/appStoreVersionLocalizations?limit=200`,
      "appStoreVersionLocalizations",
    );
    const localization = localizations.find((item) => item.attributes.locale === locale);
    if (!localization) throw new Error(`Version ${version} has no ${locale} localization.`);
    return localization;
  }

  async function screenshotSet(localizationId: string, version: string, locale: string, displayType: string) {
    const sets = await list(
      `/v1/appStoreVersionLocalizations/${encodeURIComponent(localizationId)}/appScreenshotSets?limit=200`,
      "appScreenshotSets",
    );
    const set = sets.find((item) => item.attributes.screenshotDisplayType === displayType);
    if (!set) {
      throw new Error(
        `No ${displayType} screenshot set on version ${version} (${locale}); found: ${sets.map((item) => item.attributes.screenshotDisplayType).join(", ") || "none"}.`,
      );
    }
    return set;
  }

  /// The set's screenshots in the order Apple stores (and shows) them.
  async function remoteShots(setId: string): Promise<RemoteShot[]> {
    return (await list(`/v1/appScreenshotSets/${encodeURIComponent(setId)}/appScreenshots?limit=200`, "appScreenshots")).map(
      (item) => {
        const delivery = isRecord(item.attributes.assetDeliveryState) ? item.attributes.assetDeliveryState : {};
        return {
          fileName: String(item.attributes.fileName ?? ""),
          checksum:
            typeof item.attributes.sourceFileChecksum === "string" ? item.attributes.sourceFileChecksum : undefined,
          state: typeof delivery.state === "string" ? delivery.state : undefined,
        };
      },
    );
  }

  /// The app-level localization (name, subtitle, privacy URL) of the app info being prepared. Before the
  /// first release there is one app info; after it, the live one is READY_FOR_DISTRIBUTION and the one
  /// being prepared is the other.
  async function appInfoLocalization(locale: string) {
    const infos = await list(`/v1/apps/${encodeURIComponent(config.appId)}/appInfos?limit=10`, "appInfos");
    const live = ["READY_FOR_DISTRIBUTION", "READY_FOR_SALE", "REPLACED_WITH_NEW_INFO"];
    const states = infos.map((info) => String(info.attributes.state ?? info.attributes.appStoreState ?? "?"));
    const prepared = infos.length === 1 ? infos : infos.filter((_info, index) => !live.includes(states[index]!));
    if (prepared.length !== 1) {
      throw new Error(`Expected one app info being prepared; found ${prepared.length} (${states.join(", ") || "none"}).`);
    }
    const localizations = await list(
      `/v1/appInfos/${encodeURIComponent(prepared[0]!.id)}/appInfoLocalizations?limit=200`,
      "appInfoLocalizations",
    );
    const localization = localizations.find((item) => item.attributes.locale === locale);
    if (!localization) throw new Error(`The app info has no ${locale} localization.`);
    return localization;
  }

  /// Everything check-listing compares, read from App Store Connect. Reads only.
  async function readListing(version: string, locale: string, displayType = "APP_IPHONE_67") {
    await verifyApp();
    const prepared = await findVersion(version);
    const [localization, info] = await Promise.all([
      versionLocalization(prepared.id, version, locale),
      appInfoLocalization(locale),
    ]);
    const set = await screenshotSet(localization.id, version, locale, displayType);
    const field = (value: Resource, name: string) => {
      const raw = value.attributes[name];
      return typeof raw === "string" ? raw : null;
    };
    const listing: ListingText = {
      name: field(info, "name"),
      subtitle: field(info, "subtitle"),
      description: field(localization, "description"),
      keywords: field(localization, "keywords"),
      promotionalText: field(localization, "promotionalText"),
      privacyPolicyUrl: field(info, "privacyPolicyUrl"),
      supportUrl: field(localization, "supportUrl"),
    };
    return {
      versionId: prepared.id,
      state: String(prepared.attributes.appStoreState ?? ""),
      versionLocalizationId: localization.id,
      infoLocalizationId: info.id,
      text: listing,
      shots: await remoteShots(set.id),
    };
  }

  /// Sets only the text fields that differ from `want`; returns their names. Never touches anything else.
  async function setListingText(live: Awaited<ReturnType<typeof readListing>>, want: ListingText) {
    const changed = LISTING_FIELDS.filter((name) => (live.text[name] ?? "") !== (want[name] ?? ""));
    const pick = (names: readonly ListingField[]) =>
      Object.fromEntries(names.filter((name) => changed.includes(name)).map((name) => [name, want[name]]));
    const infoChanges = pick(APP_INFO_FIELDS);
    const versionChanges = pick(LISTING_FIELDS.filter((name) => !APP_INFO_FIELDS.includes(name)));
    if (Object.keys(infoChanges).length > 0) {
      await request(`/v1/appInfoLocalizations/${encodeURIComponent(live.infoLocalizationId)}`, "PATCH", {
        data: { type: "appInfoLocalizations", id: live.infoLocalizationId, attributes: infoChanges },
      });
    }
    if (Object.keys(versionChanges).length > 0) {
      await request(`/v1/appStoreVersionLocalizations/${encodeURIComponent(live.versionLocalizationId)}`, "PATCH", {
        data: { type: "appStoreVersionLocalizations", id: live.versionLocalizationId, attributes: versionChanges },
      });
    }
    return changed;
  }

  /// The build number of the build selected on the version, or undefined when none is.
  async function selectedBuild(versionId: string) {
    const response = record(
      await request(`/v1/appStoreVersions/${encodeURIComponent(versionId)}/build`),
      "selected build response",
    );
    if (response.data === null || response.data === undefined) return undefined;
    return text(resource(response.data, "builds").attributes.version, "selected build number");
  }

  /// Selects an already processed (VALID) build on the version being prepared. Changes nothing else.
  async function selectBuild(versionId: string, selection: BuildSelection) {
    const response = record(await request(`/v1/builds?${buildQuery(selection)}`), "builds response");
    const builds = items(response.data, "builds");
    if (builds.length !== 1) {
      throw new Error(`Expected one RichOS build ${selection.version} (${selection.build}); found ${builds.length}.`);
    }
    const build = resource(builds[0], "builds");
    if (build.attributes.processingState !== "VALID") {
      throw new Error(`Build ${selection.build} is ${String(build.attributes.processingState)}, not VALID; nothing was selected.`);
    }
    await request(`/v1/appStoreVersions/${encodeURIComponent(versionId)}/relationships/build`, "PATCH", {
      data: { type: "builds", id: build.id },
    });
    return { buildId: build.id, selected: await selectedBuild(versionId) };
  }

  /// Sends the version to App Review: one review submission holding this version, then submitted. Only
  /// `submit` (runRelease) calls it, after every refusal has had its chance. Writes are never retried.
  async function submitForReview(versionId: string) {
    const created = record(
      await request("/v1/reviewSubmissions", "POST", {
        data: {
          type: "reviewSubmissions",
          attributes: { platform: "IOS" },
          relationships: { app: { data: { type: "apps", id: config.appId } } },
        },
      }),
      "review submission response",
    );
    const submission = resource(created.data, "reviewSubmissions");
    await request("/v1/reviewSubmissionItems", "POST", {
      data: {
        type: "reviewSubmissionItems",
        relationships: {
          reviewSubmission: { data: { type: "reviewSubmissions", id: submission.id } },
          appStoreVersion: { data: { type: "appStoreVersions", id: versionId } },
        },
      },
    });
    const submitted = record(
      await request(`/v1/reviewSubmissions/${encodeURIComponent(submission.id)}`, "PATCH", {
        data: { type: "reviewSubmissions", id: submission.id, attributes: { submitted: true } },
      }),
      "review submission response",
    );
    const after = resource(submitted.data, "reviewSubmissions");
    return { submissionId: after.id, state: String(after.attributes.state ?? "") };
  }

  async function screenshots(dir: string, version: string, displayType = "APP_IPHONE_67") {
    const files = localShots(dir);
    const app = await verifyApp();
    const prepared = await findVersion(version);
    const state = text(prepared.attributes.appStoreState, "appStoreState");
    if (!["PREPARE_FOR_SUBMISSION", "DEVELOPER_REJECTED", "REJECTED", "METADATA_REJECTED"].includes(state)) {
      throw new Error(`Version ${version} is ${state}; screenshots can only change while it is being prepared.`);
    }
    const locale = text(app.attributes.primaryLocale, "primaryLocale");
    const localization = await versionLocalization(prepared.id, version, locale);
    const set = await screenshotSet(localization.id, version, locale, displayType);
    const setPath = `/v1/appScreenshotSets/${encodeURIComponent(set.id)}/appScreenshots?limit=200`;
    for (const old of await list(setPath, "appScreenshots")) {
      await request(`/v1/appScreenshots/${encodeURIComponent(old.id)}`, "DELETE");
    }
    const created: string[] = [];
    for (const file of files) {
      const reservation = record(
        await request("/v1/appScreenshots", "POST", {
          data: {
            type: "appScreenshots",
            attributes: { fileName: file.fileName, fileSize: file.size },
            relationships: { appScreenshotSet: { data: { type: "appScreenshotSets", id: set.id } } },
          },
        }),
        "appScreenshots response",
      );
      const shot = resource(reservation.data, "appScreenshots");
      const bytes = NodeFS.readFileSync(file.path);
      for (const operation of items(shot.attributes.uploadOperations, "uploadOperations")) {
        const part = record(operation, "upload operation");
        const offset = Number(part.offset);
        const length = Number(part.length);
        const headers: Record<string, string> = {};
        for (const header of items(part.requestHeaders ?? [], "requestHeaders")) {
          const pair = record(header, "request header");
          headers[text(pair.name, "header name")] = text(pair.value, "header value");
        }
        const response = await fetchImpl(text(part.url, "upload url"), {
          method: text(part.method, "upload method"),
          headers,
          body: bytes.subarray(offset, offset + length),
          signal: AbortSignal.timeout(60_000),
        });
        if (!response.ok) throw new Error(`Uploading ${file.fileName} failed: HTTP ${response.status}.`);
      }
      await request(`/v1/appScreenshots/${encodeURIComponent(shot.id)}`, "PATCH", {
        data: { type: "appScreenshots", id: shot.id, attributes: { uploaded: true, sourceFileChecksum: file.md5 } },
      });
      created.push(shot.id);
    }
    await request(`/v1/appScreenshotSets/${encodeURIComponent(set.id)}/relationships/appScreenshots`, "PATCH", {
      data: created.map((id) => ({ type: "appScreenshots", id })),
    });
    // Wait until Apple has processed every one (bounded: 5 minutes).
    let remote: RemoteShot[] = [];
    for (let attempt = 0; attempt < 60; attempt++) {
      remote = await remoteShots(set.id);
      const states = remote.map((item) => item.state);
      if (states.includes("FAILED")) break;
      if (remote.length === files.length && states.every((value) => value === "COMPLETE")) break;
      await new Promise((resolve) => setTimeout(resolve, 5_000));
    }
    return { version, locale, displayType, files, remote, problems: compareScreenshots(files, remote) };
  }

  return { status, publish, verifyUpload, screenshots, readListing, setListingText, selectedBuild, selectBuild, submitForReview };
}

export function validateUploadMetadata(infoPlist: unknown, exportOptions: unknown, bundleId: string) {
  const info = record(infoPlist, "archived app Info.plist");
  if (info.CFBundleIdentifier !== bundleId || info.DTPlatformName !== "iphoneos") {
    throw new Error("Upload requires an iPhoneOS archive of the Release RichOS app.");
  }
  const options = record(exportOptions, "export options");
  if (options.method !== "app-store-connect" || options.destination !== "upload") {
    throw new Error("Export options must use method app-store-connect and destination upload.");
  }
  if (options.manageAppVersionAndBuildNumber !== false) {
    throw new Error("Export options must set manageAppVersionAndBuildNumber to false.");
  }
  return validateBuildSelection({
    build: text(info.CFBundleVersion, "CFBundleVersion"),
    version: text(info.CFBundleShortVersionString, "CFBundleShortVersionString"),
  });
}

async function readPlist(path: string): Promise<unknown> {
  const execFile = NodeUtil.promisify(NodeChildProcess.execFile);
  try {
    const { stdout } = await execFile("/usr/bin/plutil", ["-convert", "json", "-o", "-", path], {
      encoding: "utf8",
      timeout: 10_000,
      maxBuffer: 2 * 1_024 * 1_024,
    });
    return JSON.parse(stdout);
  } catch {
    throw new Error(`Cannot read plist at ${path}.`);
  }
}

async function uploadArchive(
  archive: string,
  exportOptions: string,
  config: TestFlightConfig,
  privateKey: NodeCrypto.KeyObject,
  verifyUpload: (selection: BuildSelection) => Promise<void>,
  beforeExport: (selection: BuildSelection) => void,
) {
  const archivePath = NodePath.resolve(expandHome(archive));
  const optionsPath = NodePath.resolve(expandHome(exportOptions));
  const [info, options] = await Promise.all([
    readPlist(NodePath.join(archivePath, ARCHIVED_APP)),
    readPlist(optionsPath),
  ]);
  const selection = validateUploadMetadata(info, options, config.bundleId);
  await verifyUpload(selection);
  beforeExport(selection);
  await withTemporaryPrivateKey(
    privateKey,
    (keyPath) =>
      new Promise<void>((resolve, reject) => {
        const child = NodeChildProcess.spawn(
          "xcodebuild",
          [
            "-exportArchive",
            "-archivePath",
            archivePath,
            "-exportOptionsPlist",
            optionsPath,
            "-authenticationKeyPath",
            keyPath,
            "-authenticationKeyID",
            config.keyId,
            "-authenticationKeyIssuerID",
            config.issuerId,
          ],
          { stdio: "inherit" },
        );
        child.once("error", () => reject(new Error("Could not start xcodebuild.")));
        child.once("exit", (code) =>
          code === 0
            ? resolve()
            : reject(new Error("Xcode upload failed. Check status before trying another upload.")),
        );
      }),
  );
  process.stdout.write(
    `Xcode uploaded RichOS ${selection.version} (${selection.build}). Check status after Apple processes it.\n`,
  );
}

// RichOS addition (CEO 2026-10-03, §106: "nothing can get to the actual users without passing the
// speed tests"). Upload and publish both put a build in testers' hands, so both ask the speed gate
// (richos/mobile/perf/shipgate.py) first: it passes only with the phone speed watch's §104 PASS, cold
// and warm, for that exact app code on the iPhone test phone. It fails closed and nothing skips it.
const RELEASE_DIR = import.meta.dirname;
const SPEED_GATE = NodePath.resolve(RELEASE_DIR, "../../perf/shipgate.py");
const WALK_GATE = NodePath.resolve(RELEASE_DIR, "../../perf/reviewwalk.py");
// Which commit each upload was made from, so publish gates the same code. Outside every checkout.
export const UPLOAD_RECEIPTS = stateDir("ship-gate/testflight-uploads.json");
// The source stamp the archive carries (the keys shipgate.py's `stamp` writes).
export const STAMP_COMMIT = "RichOSSourceCommit";
export const STAMP_DIRTY = "RichOSSourceDirty";

type Spawn = typeof NodeChildProcess.spawnSync;

/// Runs the gate; returns the full commit it passed, or throws its REFUSED line.
export function speedGate(where: string[], spawn: Spawn = NodeChildProcess.spawnSync) {
  const run = spawn("python3", [SPEED_GATE, "check", "--platform", "ios", ...where], {
    encoding: "utf8",
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const refusal = String(run.stderr ?? "").trim().split("\n").pop();
  if (run.status !== 0) {
    throw new Error(
      refusal ||
        `REFUSED by the speed gate (CEO §106): iPhone: the gate did not run (${run.error?.message ?? `exit ${run.status}`}).`,
    );
  }
  let commit: unknown;
  try {
    const result = JSON.parse(String(run.stdout));
    commit = result.ok === true ? result.commit : undefined;
  } catch {
    commit = undefined;
  }
  if (typeof commit !== "string" || !/^[0-9a-f]{40}$/u.test(commit)) {
    throw new Error("REFUSED by the speed gate (CEO §106): iPhone: the gate gave no passing commit.");
  }
  return commit;
}

/// RichOS addition (CEO 2026-10-04, §107: the CEO is never the first tester). Passes only when the
/// automated walk of exactly what Apple's reviewer does (`rios review-walk --commit SHA`) passed on that
/// exact commit, as its record says (richos/mobile/perf/reviewwalk.py). Fails closed; nothing skips it.
export function walkGate(commit: string, spawn: Spawn = NodeChildProcess.spawnSync) {
  const run = spawn("python3", [WALK_GATE, "check", "--platform", "ios", "--commit", commit], {
    encoding: "utf8",
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const refusal = String(run.stderr ?? "").trim().split("\n").pop();
  if (run.status !== 0) {
    throw new Error(
      refusal ||
        `REFUSED by the review-walk gate (CEO §107): iPhone build of ${commit.slice(0, 12)}: the gate did not run (${run.error?.message ?? `exit ${run.status}`}).`,
    );
  }
  let passed: unknown;
  try {
    const result = JSON.parse(String(run.stdout));
    passed = result.ok === true ? result.commit : undefined;
  } catch {
    passed = undefined;
  }
  if (passed !== commit) {
    throw new Error(
      `REFUSED by the review-walk gate (CEO §107): iPhone build of ${commit.slice(0, 12)}: the gate gave no pass for this commit.`,
    );
  }
}

/// RichOS addition: refuses an archive App Store Connect would reject for its device family, icons or
/// orientations (Release/check_device_family.py; build 1 was rejected on 2026-10-04 with 90023/90474).
/// Runs on the archive itself, before anything contacts Apple. Fails closed.
export function deviceFamilyCheck(archive: string, spawn: Spawn = NodeChildProcess.spawnSync) {
  const path = NodePath.resolve(expandHome(archive));
  const run = spawn("python3", [NodePath.join(RELEASE_DIR, "check_device_family.py"), path], {
    encoding: "utf8",
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (run.status !== 0) {
    const lines = `${run.stdout ?? ""}\n${run.stderr ?? ""}`.split("\n").filter((line) => /FAIL|Error|error/u.test(line));
    throw new Error(
      `REFUSED by the device-family check: iPhone archive ${archive} would be rejected by App Store Connect: ${lines.slice(0, 5).join("; ") || run.error?.message || `exit ${run.status}`}.`,
    );
  }
}

function receiptKey(selection: BuildSelection) {
  return `${selection.version} (${selection.build})`;
}

function readReceipts(file: string): Record<string, unknown> {
  try {
    const data: unknown = JSON.parse(NodeFS.readFileSync(file, "utf8"));
    return isRecord(data) ? data : {};
  } catch {
    return {};
  }
}

/// Written before the export starts, so a build that reaches App Store Connect always has one.
export function recordUpload(file: string, selection: BuildSelection, commit: string) {
  const receipts = readReceipts(file);
  receipts[receiptKey(selection)] = { commit, at: new Date().toISOString() };
  NodeFS.mkdirSync(NodePath.dirname(file), { recursive: true });
  NodeFS.writeFileSync(`${file}.tmp`, `${JSON.stringify(receipts, null, 1)}\n`);
  NodeFS.renameSync(`${file}.tmp`, file);
}

/// The commit a build was uploaded from; refuses a build this tool did not upload.
export function uploadedCommit(file: string, selection: BuildSelection, action = "published") {
  const entry = readReceipts(file)[receiptKey(selection)];
  const commit = isRecord(entry) ? entry.commit : undefined;
  if (typeof commit !== "string" || !/^[0-9a-f]{40}$/u.test(commit)) {
    throw new Error(
      `REFUSED by the speed gate (CEO §106): iPhone build ${receiptKey(selection)}: no record of the commit it was uploaded from (${file}); only a build uploaded by this tool, after the speed gate, can be ${action}.`,
    );
  }
  return commit;
}

type ReleaseArgs = Exclude<ReturnType<typeof parseTestFlightArgs>, { command: "help" }>;

export type ReleaseDeps = {
  gate: (where: string[]) => string;
  walk: (commit: string) => void;
  deviceFamily: (archive: string) => void;
  receipts: string;
  checkout: string;
  readArchive: (archive: string) => Promise<unknown>;
  readEnv: typeof readTestFlightEnv;
  client: (
    config: TestFlightConfig,
    privateKey: NodeCrypto.KeyObject,
  ) => Partial<ReturnType<typeof createTestFlightClient>>;
  upload: typeof uploadArchive;
  readNotes: (path: string) => Promise<string>;
  readRecord: (path: string) => ListingRecord;
  localShots: (dir: string) => LocalShot[];
};

const RELEASE_DEPS: ReleaseDeps = {
  gate: (where) => speedGate(where),
  walk: (commit) => walkGate(commit),
  deviceFamily: (archive) => deviceFamilyCheck(archive),
  receipts: UPLOAD_RECEIPTS,
  checkout: RELEASE_DIR,
  readArchive: readArchivedInfo,
  readEnv: readTestFlightEnv,
  client: createTestFlightClient,
  upload: uploadArchive,
  readNotes: (path) => NodeFSP.readFile(expandHome(path), "utf8"),
  readRecord: readListingRecord,
  localShots,
};

function need<T>(value: T | undefined, name: string): T {
  if (value === undefined) throw new Error(`The App Store Connect client has no ${name}.`);
  return value;
}

/// The live listing beside the record: every mismatch by field name (empty when they are the same).
async function compareLiveListing(
  listing: ListingRecord,
  client: ReturnType<ReleaseDeps["client"]>,
  deps: Pick<ReleaseDeps, "localShots">,
) {
  const local = deps.localShots(listing.screenshotsDir);
  const live = await need(client.readListing, "readListing")(listing.version, listing.locale);
  return { listing, local, live, problems: compareListing(listing.text, live.text, local, live.shots) };
}

export type ListingCheck = Awaited<ReturnType<typeof compareLiveListing>>;

function listingDiffers(check: ListingCheck) {
  return new Error(
    `The App Store listing of ${check.listing.version} differs from the record ${check.listing.path} (${check.problems.length}):\n${check.problems.join("\n")}`,
  );
}

/// `submit` (CEO 2026-10-04: impossible to submit a version that is not proven). The only way this tool
/// sends a version to App Review. Every refusal comes before the one write; nothing skips any of them.
async function submitVersion(listing: ListingRecord, client: ReturnType<ReleaseDeps["client"]>, deps: ReleaseDeps) {
  // 1. The live listing is the record: text and screenshots.
  const check = await compareLiveListing(listing, client, deps);
  if (check.problems.length > 0) {
    throw new Error(
      `REFUSED by the listing check: ${listingDiffers(check).message}\nRun apply-listing, then submit again.`,
    );
  }
  // 2. The build selected on the version, and the commit its upload recorded from the archive's stamp.
  const build = await need(client.selectedBuild, "selectedBuild")(check.live.versionId);
  if (!build) {
    throw new Error(`REFUSED: version ${listing.version} has no build selected in App Store Connect; nothing was submitted.`);
  }
  const selection = { build, version: listing.version };
  const commit = uploadedCommit(deps.receipts, selection, "submitted for review");
  // 3. The speed pass (§106) for that exact commit.
  const passed = deps.gate(["--repo", deps.checkout, "--commit", commit]);
  if (passed !== commit) {
    throw new Error(`REFUSED by the speed gate (CEO §106): iPhone: the gate passed ${passed}, not build ${receiptKey(selection)}'s ${commit}.`);
  }
  // 4. The review walk (§107) for that exact commit.
  deps.walk(commit);
  const submission = await need(client.submitForReview, "submitForReview")(check.live.versionId);
  return { ...check, build: selection, commit, submission };
}

/// The archived app's Info.plist, where its source stamp is.
export function readArchivedInfo(archive: string) {
  return readPlist(NodePath.join(NodePath.resolve(expandHome(archive)), ARCHIVED_APP));
}

/// The commit an archive was built from, as the archive itself carries it (the freshness contract:
/// every artifact carries its source commit). Release/platform.yml stamps it into the archived app's
/// Info.plist (richos/mobile/perf/shipgate.py stamp). Refuses an archive with no stamp, or one built
/// with uncommitted app code, which no speed measurement can cover.
export function archiveSourceCommit(info: unknown, archive: string) {
  const data = isRecord(info) ? info : {};
  const commit = data[STAMP_COMMIT];
  if (typeof commit !== "string" || !/^[0-9a-f]{40}$/u.test(commit)) {
    throw new Error(
      `REFUSED by the speed gate (CEO §106): iPhone archive ${archive} carries no source commit (${STAMP_COMMIT} in its app's Info.plist); archive it again from a checkout of this repository, whose project stamps it (Release/platform.yml).`,
    );
  }
  if (data[STAMP_DIRTY] !== false) {
    throw new Error(
      `REFUSED by the speed gate (CEO §106): iPhone archive ${archive} was built from ${commit.slice(0, 12)} with uncommitted app code (${STAMP_DIRTY} is ${JSON.stringify(data[STAMP_DIRTY] ?? null)}); commit it, let the speed watch measure it, then archive again.`,
    );
  }
  return commit;
}

/// One release command. Upload and publish ask the speed gate before credentials, network or Xcode;
/// upload also asks the review-walk gate.
export async function runRelease(args: ReleaseArgs, deps: ReleaseDeps = RELEASE_DEPS) {
  if (args.command === "upload") {
    // The commit the archive carries, never this checkout's HEAD: the archive may be older or newer.
    const stamped = archiveSourceCommit(await deps.readArchive(args.archive), args.archive);
    const commit = deps.gate(["--repo", deps.checkout, "--commit", stamped]);
    if (commit !== stamped) {
      throw new Error(`REFUSED by the speed gate (CEO §106): iPhone: the gate passed ${commit}, not the archive's ${stamped}.`);
    }
    // The reviewer walk on that same commit (§107), before credentials, network or Xcode.
    deps.walk(stamped);
    // What App Store Connect would reject on processing: device family, icons, orientations.
    deps.deviceFamily(args.archive);
    const { config, privateKey } = await deps.readEnv(args.envFile);
    const client = deps.client(config, privateKey);
    await deps.upload(args.archive, args.exportOptions, config, privateKey, need(client.verifyUpload, "verifyUpload"), (selection) =>
      recordUpload(deps.receipts, selection, commit),
    );
    return undefined;
  }
  if (args.command === "screenshots") {
    // Listing artwork only: no build is uploaded or published, so neither gate applies.
    const { privateKey, config } = await deps.readEnv(args.envFile);
    const result = await need(deps.client(config, privateKey).screenshots, "screenshots")(expandHome(args.dir), args.version);
    return { config, result };
  }
  if (args.command === "select-build") {
    // Selects the processed build on the record's version; sends nothing to review.
    const listing = deps.readRecord(args.record);
    const { config, privateKey } = await deps.readEnv(args.envFile);
    const client = deps.client(config, privateKey);
    const live = await need(client.readListing, "readListing")(listing.version, listing.locale);
    const selection = validateBuildSelection({ build: args.build, version: listing.version });
    return { config, result: { ...(await need(client.selectBuild, "selectBuild")(live.versionId, selection)), selection } };
  }
  if (args.command === "check-listing" || args.command === "apply-listing" || args.command === "submit") {
    // The record is read first: a missing or malformed record refuses before credentials or network.
    const listing = deps.readRecord(args.record);
    const { config, privateKey } = await deps.readEnv(args.envFile);
    const client = deps.client(config, privateKey);
    if (args.command === "submit") return { config, result: await submitVersion(listing, client, deps) };
    let changed: string[] = [];
    let screenshotsReplaced = false;
    if (args.command === "apply-listing") {
      // Never submits: text first, then the screenshots only when they differ, then the full check.
      const before = await compareLiveListing(listing, client, deps);
      changed = await need(client.setListingText, "setListingText")(before.live, listing.text);
      if (compareScreenshots(before.local, before.live.shots).length > 0) {
        const shots = await need(client.screenshots, "screenshots")(listing.screenshotsDir, listing.version);
        if (shots.problems.length > 0) throw new Error(`Screenshot read-back does not match:\n${shots.problems.join("\n")}`);
        screenshotsReplaced = true;
      }
    }
    const check = await compareLiveListing(listing, client, deps);
    if (check.problems.length > 0) throw listingDiffers(check);
    return { config, result: { ...check, changed, screenshotsReplaced } };
  }
  const selection = { build: args.build, version: args.version };
  if (args.command === "publish") {
    deps.gate(["--repo", deps.checkout, "--commit", uploadedCommit(deps.receipts, selection)]);
  }
  const { config, privateKey } = await deps.readEnv(args.envFile);
  const client = deps.client(config, privateKey);
  return args.command === "publish"
    ? { config, result: await need(client.publish, "publish")(selection, await deps.readNotes(args.notesFile)) }
    : { config, result: await need(client.status, "status")(selection) };
}

// Xcode needs a file. REST requests use only the in-memory key.
export async function withTemporaryPrivateKey(
  privateKey: NodeCrypto.KeyObject,
  operation: (keyPath: string) => Promise<void>,
) {
  const directory = await NodeFSP.mkdtemp(NodePath.join(NodeOS.tmpdir(), "richos-testflight-"));
  try {
    await NodeFSP.chmod(directory, 0o700);
    const keyPath = NodePath.join(directory, "AuthKey.p8");
    await NodeFSP.writeFile(keyPath, privateKey.export({ format: "pem", type: "pkcs8" }), {
      mode: 0o600,
      flag: "wx",
    });
    await operation(keyPath);
  } finally {
    await NodeFSP.rm(directory, { recursive: true, force: true });
  }
}

export function parseTestFlightArgs(args: string[]) {
  const { values, positionals } = NodeUtil.parseArgs({
    args,
    allowPositionals: true,
    options: {
      "env-file": { type: "string" },
      build: { type: "string" },
      version: { type: "string" },
      "notes-file": { type: "string" },
      archive: { type: "string" },
      "export-options": { type: "string" },
      dir: { type: "string" },
      record: { type: "string" },
      help: { type: "boolean" },
    },
  });
  if (values.help) return { command: "help" as const };
  const command = positionals[0];
  const commands = ["status", "publish", "upload", "screenshots", "check-listing", "apply-listing", "submit", "select-build"] as const;
  if (positionals.length !== 1 || !commands.includes(command as never)) {
    throw new Error(`Choose ${commands.join(", ")}. Use --help for usage.`);
  }
  const envFile =
    values["env-file"] ??
    process.env.RICHOS_IOS_TESTFLIGHT_ENV_FILE ??
    NodePath.join(NodeOS.homedir(), ".config/richos/testflight.env");
  if (command === "select-build") {
    if (values.version || values.archive || values["export-options"] || values["notes-file"] || values.dir) {
      throw new Error("select-build takes only --record, --build and --env-file.");
    }
    return { command, envFile, record: text(values.record, "--record"), build: text(values.build, "--build") };
  }
  if (command === "check-listing" || command === "apply-listing" || command === "submit") {
    // The version, the text and the screenshots all come from the record; nothing else is accepted, and
    // there is no option that skips a check.
    if (values.build || values.version || values.archive || values["export-options"] || values["notes-file"] || values.dir) {
      throw new Error(`${command} takes only --record and --env-file; the version and everything else come from the record.`);
    }
    return { command, envFile, record: text(values.record, "--record") };
  }
  if (values.record) throw new Error("--record is only valid with check-listing, apply-listing and submit.");
  if (command === "screenshots") {
    if (values.build || values.archive || values["export-options"] || values["notes-file"]) {
      throw new Error("Screenshots takes only --dir, --version and --env-file.");
    }
    return {
      command: "screenshots" as const,
      envFile,
      dir: text(values.dir, "--dir"),
      version: text(values.version, "--version"),
    };
  }
  if (values.dir) throw new Error("--dir is only valid with screenshots.");
  if (command === "upload") {
    if (values.build || values.version || values["notes-file"]) {
      throw new Error(
        "Upload reads the build and version from the archive; do not supply --build, --version, or --notes-file.",
      );
    }
    return {
      command: "upload" as const,
      envFile,
      archive: text(values.archive, "--archive"),
      exportOptions: text(values["export-options"], "--export-options"),
    };
  }
  if (values.archive || values["export-options"])
    throw new Error("Archive options are only valid with upload.");
  const selection = validateBuildSelection({
    build: text(values.build, "--build"),
    version: text(values.version, "--version"),
  });
  if (command === "status" && values["notes-file"]) {
    throw new Error("--notes-file is only valid with publish.");
  }
  return command === "publish"
    ? {
        command: "publish" as const,
        envFile,
        ...selection,
        notesFile: text(values["notes-file"], "--notes-file"),
      }
    : { command: "status" as const, envFile, ...selection };
}

function expandHome(path: string) {
  return path.startsWith("~/") ? NodePath.join(NodeOS.homedir(), path.slice(2)) : path;
}

async function main() {
  const args = parseTestFlightArgs(process.argv.slice(2));
  if (args.command === "help") {
    process.stdout.write(`Usage:
  node Release/testflight.ts status --build 1 --version 1.0.0
  node Release/testflight.ts publish --build 1 --version 1.0.0 --notes-file /path/to/notes.txt
  node Release/testflight.ts screenshots --version 1.0.0 --dir /path/to/pngs   (replaces the iPhone 6.9-inch set in file-name order, then reads it back)
  node Release/testflight.ts upload --archive /path/to/RichOSNative.xcarchive --export-options Release/ExportOptions.plist
  node Release/testflight.ts check-listing --record /path/to/listing-state.json   (exit 1, every mismatch by field, unless the live listing is the record)
  node Release/testflight.ts apply-listing --record /path/to/listing-state.json   (sets the listing from the record, then check-listing; never submits)
  node Release/testflight.ts select-build --record /path/to/listing-state.json --build 1   (selects that processed build on the record's version; never submits)
  node Release/testflight.ts submit --record /path/to/listing-state.json          (the only way to send a version to App Review)

The listing record is the private record repository's docs/operations/*-listing-state.json: name,
subtitle, description, keywords, promotional text, privacy and support URLs, the version, and
screenshotsDir (the folder of screenshots, relative to that repository, in file-name order).
Submit is REFUSED, with no option to skip, unless check-listing passes, the build selected on the
version was uploaded by this tool (its archive's stamped commit is recorded), that commit has the
phone speed watch's pass (shipgate.py, CEO §106) and the review walk passed on it (reviewwalk.py,
CEO §107).

Optional: --env-file /path/to/file or RICHOS_IOS_TESTFLIGHT_ENV_FILE.
Default env file: ~/.config/richos/testflight.env (mode 600, never inside a git working tree).
Required variables: RICHOS_IOS_ASC_KEY_ID, RICHOS_IOS_ASC_ISSUER_ID,
RICHOS_IOS_ASC_PRIVATE_KEY_BASE64, RICHOS_IOS_ASC_APP_ID, RICHOS_IOS_ASC_BUNDLE_ID,
RICHOS_IOS_ASC_INTERNAL_GROUP_ID. Optional: RICHOS_IOS_ASC_PUBLIC_GROUP_ID (external testing).
Only the Release RichOS app and the configured existing groups are supported.
Publish does not upload, create testers, change signing, or expire other builds.
Upload and publish are REFUSED without the phone speed watch's §104 PASS, cold and warm, for the
exact app code (richos/mobile/perf/shipgate.py, CEO §106). Upload gates the commit the archive
carries (RichOSSourceCommit in the archived app's Info.plist, stamped when it is archived; an archive
without it, or built with uncommitted app code, is refused) and records it; publish gates the commit
its upload recorded. Upload is also REFUSED unless the automated walk of exactly what Apple's
reviewer does passed on the archive's commit (rios review-walk --commit SHA; CEO §107), and unless
Release/check_device_family.py passes the archive (iPhone-only device family, icons, orientations).
`);
    return;
  }
  const outcome = await runRelease(args);
  if (!outcome) return;
  if (args.command === "check-listing" || args.command === "apply-listing" || args.command === "submit") {
    // A mismatch never reaches here: runRelease threw it, every field by name, and the exit is 1.
    const check = outcome.result as ListingCheck & {
      changed?: string[];
      screenshotsReplaced?: boolean;
      build?: BuildSelection;
      commit?: string;
      submission?: { submissionId: string; state: string };
    };
    if (args.command === "apply-listing") {
      process.stdout.write(
        `Set from the record: ${check.changed?.length ? check.changed.join(", ") : "no text field (all already matched)"}; screenshots ${check.screenshotsReplaced ? "replaced" : "already matched, left as they are"}.\n`,
      );
    }
    process.stdout.write(`Record: ${check.listing.path}\nVersion ${check.listing.version} (${check.live.state}), ${check.listing.locale}\n`);
    for (const field of LISTING_FIELDS) {
      const value = check.live.text[field] ?? "";
      const shown = value.length > 70 ? `${JSON.stringify(value.slice(0, 70))}… (${value.length} characters)` : JSON.stringify(value);
      process.stdout.write(`  ok  ${field}: ${shown}\n`);
    }
    check.local.forEach((file, index) => {
      process.stdout.write(`  ok  screenshot ${index + 1}: ${file.fileName}  md5 ${file.md5}  ${check.live.shots[index]?.state}\n`);
    });
    process.stdout.write(
      `The listing matches the record: ${LISTING_FIELDS.length} text fields and ${check.local.length} screenshots (${check.listing.screenshotsDir}).\n`,
    );
    if (args.command === "submit" && check.submission && check.build) {
      process.stdout.write(
        `Submitted ${check.listing.version} (${check.build.build}), commit ${check.commit}, to App Review: submission ${check.submission.submissionId} is ${check.submission.state}.\n`,
      );
    }
    return;
  }
  if (args.command === "select-build") {
    const done = outcome.result as { selection: BuildSelection; buildId: string; selected?: string };
    if (done.selected !== done.selection.build) throw new Error(`Read-back shows build ${done.selected ?? "none"} selected, not ${done.selection.build}.`);
    process.stdout.write(`Selected build ${done.selection.version} (${done.selected}), id ${done.buildId}; read back from App Store Connect.\n`);
    return;
  }
  if (args.command === "screenshots") {
    const shots = outcome.result as { remote: RemoteShot[]; files: LocalShot[]; problems: string[] };
    shots.files.forEach((file, index) => {
      const have = shots.remote[index];
      process.stdout.write(
        `${index + 1}. ${file.fileName}  local md5 ${file.md5}  Apple ${have?.checksum ?? "(none)"}  ${have?.state ?? "(missing)"}\n`,
      );
    });
    if (shots.problems.length > 0) throw new Error(`Read-back does not match:\n${shots.problems.join("\n")}`);
    process.stdout.write(
      `Read-back matches: ${shots.files.length} screenshots, in order, every checksum equal and COMPLETE.\n`,
    );
    return;
  }
  const { config, result } = outcome as { config: TestFlightConfig; result: Awaited<ReturnType<ReturnType<typeof createTestFlightClient>["status"]>> };
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  if (args.command === "publish") {
    process.stdout.write(
      config.publicGroupId
        ? result.publicTesting
          ? "Public beta testing is active.\n"
          : `Public testing is not active yet. Apple reports ${result.externalState ?? result.processingState}. Run status to check again.\n`
        : result.internalTesting
          ? "Internal testing is active.\n"
          : `Internal testing is not active yet. Apple reports ${result.internalState ?? result.processingState}. Run status to check again.\n`,
    );
  }
}

if (import.meta.main) {
  main().catch((error: unknown) => {
    process.stderr.write(
      `${error instanceof Error ? error.message : "TestFlight command failed."}\n`,
    );
    process.exitCode = 1;
  });
}
