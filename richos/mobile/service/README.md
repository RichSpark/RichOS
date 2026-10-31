# Mobile update policy service

This service distributes update metadata independently of a user's Mac. It never carries conversation content. Executable application changes stay in signed app bundles distributed through Apple. Policies can choose notices and disable existing actions; they cannot supply scripts, screens or arbitrary navigation destinations.

## Run and publish

The module exports `serve`, `preview`, `publish` and `readPolicy`. It needs Node with no additional packages. On a development Mac, use the mobile CLI:

```sh
RICHOS_UPDATE_DIRECTORY=/Volumes/E1TB/caches/richos-policy node richos/mobile/cli/mobile.mjs update serve
RICHOS_UPDATE_DIRECTORY=/Volumes/E1TB/caches/richos-policy node richos/mobile/cli/mobile.mjs update preview /path/to/request.json
RICHOS_UPDATE_DIRECTORY=/Volumes/E1TB/caches/richos-policy node richos/mobile/cli/mobile.mjs update publish /path/to/request.json
```

The operator owns a mode-0700 directory. Publication uses a local exclusive lock, archives a revision with the operator identity and atomically activates it. HTTP has no publication route. Separate CLI locks allow publication while the service runs. A failed activation can consume a revision; the next attempt must use a higher number. Never manually replace `active.json` with an older record to roll back.

`serve` defaults to an ephemeral loopback port. `RICHOS_UPDATE_PORT` can select a fixed port. It is the development and physical-lab service; the public service is the hosted Worker below. No public deployment is performed by the development CLI.

## Hosted service (Cloudflare Worker)

Release builds read `https://updates.richos.ceo/v1/policy`, served by the Worker `richos-update-policy` from the D1 database `richos-update-policy`, on Cloudflare's network and independent of every user's Mac. Both the Connect and Tailscale routes reach it the same way: the app's update session goes straight to that public HTTPS name, never through the paired Mac.

- `policy-worker.mjs` (main module), `policy-store.mjs` and `../core/updates.js` are the whole upload. The Worker serves `GET /v1/policy` (newest revision for the preserved app, revalidated), `GET /v1/events` (change hints), the same pair per native app under `/<target>` (see Targets below) and `GET /healthz`. Every other method returns 405 without reading the body. It holds SELECT statements only, reads no header, cookie, query or address and logs nothing. Upload with logs, traces and Logpush off, no tail consumers and workers.dev and preview URLs disabled.
- The store is append-only (`policy-schema.sql`): one row per revision with its audit record; triggers refuse UPDATE and DELETE. The served policy is always the highest revision, so an older row can never be served over a newer one.
- Each hint stream polls the store every 3 seconds, sends a keepalive every 12 seconds (inside the iPhone client's 15-second idle timeout) and ends after 105 seconds (before its 120-second resource timeout). The client reconnects 15 seconds after a close and keeps its 60-second foreground fallback. One stream costs at most 36 D1 queries, under the Workers Free limit of 50 per invocation. The current iPhone client refetches on any stream bytes, so each keepalive also produces one policy request per active phone.
- `POST /v1/metrics` is not hosted: client metrics stay off (`metrics: false`) until the privacy review above is done. Cloudflare's aggregate request analytics are the only delivery counts.

Operator commands (identifiers in a private profile `{ "accountId", "databaseId", "hostname" }`; the token lives only in a mode-0600 file named by `RICHOS_POLICY_TOKEN_FILE`, never in argv, output or records):

```sh
node richos/mobile/cli/mobile.mjs update worker-artifact <profile.json>      # exact upload, module hashes, schema
RICHOS_POLICY_PROFILE=<profile.json> RICHOS_POLICY_TOKEN_FILE=<token> node richos/mobile/cli/mobile.mjs update schema-hosted
node richos/mobile/cli/mobile.mjs update preview <request.json>              # review decisions, copy the digest
RICHOS_POLICY_PROFILE=<profile.json> RICHOS_POLICY_TOKEN_FILE=<token> node richos/mobile/cli/mobile.mjs update publish-hosted <request.json>
```

`publish-hosted` applies the same authorization as `publish` (matching preview digest, operator identity, availability receipt for a release) before any network call, then inserts one row with a single statement that succeeds only if its revision is higher than every stored one, reads it back and reports what the public route serves. Rollback and withdrawal are a new, higher revision, as with the local store. Deploy and withdrawal steps: `richos-hq/docs/operations/2026-09-22-richos-update-policy-service.md`.

### Targets: one record, one app

Every record is for exactly one app, named by the policy's optional `target` field:

| `target` | App | Routes (hosted and local) |
| --- | --- | --- |
| absent or `ios-preserved` | the earlier iPhone app (removed 2026-10-31; phones that still have it keep working) | `GET /v1/policy`, `GET /v1/events`, unchanged |
| `ios-native` | the new native iPhone app | `GET /v1/policy/ios-native`, `GET /v1/events/ios-native` |
| `android-native` | the new native Android app | `GET /v1/policy/android-native`, `GET /v1/events/android-native` |

Isolation is enforced by the service, not by the clients: the preserved app's routes only ever serve untargeted or `ios-preserved` records and announce only their revisions, so a notice, block or feature switch meant for a native app can never reach it, whatever its code does with fields it does not know. Every record published before targets existed is untargeted, so the preserved app sees no change. The target is part of the previewed and digested policy and the hosted store reads it from the stored policy JSON, so the D1 schema is unchanged.

Revisions are one increasing sequence across all targets (the insert statement is unchanged), so each app's own revisions still only increase. A native client must also check that a served policy's `target` is its own and refuse it otherwise. `ios-native` uses the App Store rules above. `android-native` uses the same rules except that `latest` names a Google Play release, `{packageName, version, build, minimumSdk, verifiedAt}`, where `build` is the Android version code as a decimal string and `minimumSdk` is the lowest supported API level and its availability receipt attests those same fields. Preview evaluates client decisions with the shared JavaScript rules, which know App Store listings only, so an `android-native` preview takes no `clients`; the Android core evaluates them.

Locally, `publish` writes an `ios-native` or `android-native` record to `active-<target>.json` beside `active.json` and `serve` answers the same six routes.

`node --test richos/mobile/test/policy-worker-runtime.test.js` runs the exact upload in workerd, Cloudflare's runtime, through the Miniflare that ships with an installed Wrangler (or `RICHOS_MINIFLARE`). It is skipped, with that reason, where no local runtime exists.

Public routes are `GET /v1/policy`, `GET /v1/events` and the bounded optional `POST /v1/metrics`. The stream checks activated revisions every second and sends keepalives every 15 seconds. Connections are capped. Native clients refetch on stream hints; the hint is not policy authority. They retain a 60-second foreground fallback and reconnect a failed hint stream after 15 seconds.

A publication request contains `policy`, `operator`, `previewDigest` and, when recommending a release, `availability`. Preview accepts the same `policy` plus a `clients` array and returns the digest and evaluated decisions. Copy that digest into the publication request only after reviewing the affected clients. Every policy change, including withdrawal and rollback, needs a new increasing revision.

## Policy and availability

Schema 1 requires:

- `revision`: positive integer, monotonically increasing.
- `issuedAt` and `expiresAt`: timestamps with a lifetime no longer than 24 hours.
- `severity`: `none`, `banner`, `dialog` or `blocking`.
- `title` and `message`: plain text, limited to 100 and 1,200 characters.
- `allowDismiss`: boolean, with `remindAfterSeconds` between 60 and 604,800.
- Optional `features`: boolean switches for `text` and `recording`.
- For a notice, `latest`: numeric `version`, `build`, `minimumOS`, fixed numeric `appId`, three-letter App Store `storefronts` and `verifiedAt`.
- Optional `minimum` version/build and `blockedBuilds` entries formatted `version+build`.

The minimum cannot exceed the replacement and the replacement cannot itself be blocked. A blocking policy blocks only affected builds with an eligible replacement. Other eligible clients receive a dialog. Marketing versions and builds compare numerically. The client uses its actual StoreKit storefront, not a locale guess. Unknown storefront, unsupported OS, mismatched listing ID or an already-installed replacement prevents promotion. Version 1 supports iPhones meeting the OS floor without additional hardware requirements; release verification must confirm that remains true before publishing a receipt.

Availability must be verified recently, at most 24 hours before policy issue. The `availability` receipt repeats all `latest` fields, sets `downloadVerified: true` and includes an `evidence` reference. A trusted release operator must actually verify download availability for every included storefront and the minimum supported device/OS. This is an operator attestation, not an automated Apple availability oracle. Approval or a scheduled release is insufficient. Keep actual download evidence privately. No receipt may be created from a synthetic test policy for production use.

The app's compiled configuration supplies the Store ID and fixed support endpoint. Policy data cannot redirect either destination. The Update button opens `https://apps.apple.com/app/id<configured ID>`. Store authentication, download and installation remain Apple's controls.

## Failure and recovery

Ordinary notices wait while recording or sending. Critical policies stop affected new actions, retain captured audio and preserve drafts and queued work. Feature switches are checked at action and transport boundaries. Existing in-flight sends may already have reached the Mac; their idempotency IDs preserve safe acknowledgement/retry handling.

Invalid, stale or out-of-order policy responses are rejected. An outage retains the last validated policy without extending its expiry. On expiry, that policy's block and feature restrictions are released; the app keeps trying to fetch fresh policy. This is deliberate protection against a permanent lockout caused by a dead service. The Mac can still reject unsupported operations. Messages held after a disabled action remain visible and can be retried explicitly once it is restored.

To recover from a bad policy, preview and publish a higher revision restoring the intended switches and prominence. Do not reuse an old revision. Test cache expiry and offline recovery, including a phone that never received the withdrawal. Keep backward-compatible Mac and service contracts while an app fix is in review.

## Metrics and release operations

Client reporting is off in the development configuration. When enabled after privacy review, it sends only an event name, numeric app version/build and policy revision. Allowed events are policy-visible, policy-failed, store-opened and store-failed. The native boundary constructs the payload; there is no arbitrary analytics body. The service also counts policy responses. It aggregates by UTC day, event, build and revision, retains at most 30 days and writes counters every minute. Reports count events, not unique people or installs. No identity, stable device ID, conversation text or pairing information is collected.

`update metrics` reads the private aggregate report. Before enabling collection publicly, document the provider/proxy logging settings, retention, purpose and user-facing privacy disclosure. Review delivery failures and build adoption by counts; do not imply these counts prove installations or unique-user adoption.

Before the first public release: create the actual app listing, deploy the hosted policy Worker and a support endpoint, verify distribution rights and current Apple submission requirements, complete signing/artwork/privacy records and verify the actual Store Update button on a supported iPhone. Exercise an old-to-new Store installation once two released builds exist. Keep Connect and Tailscale coverage separate. Prepare a known-good higher-revision withdrawal and evidence for an expedited-review request before enabling a mandatory production block.

Apple's [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) and [StoreKit storefront documentation](https://developer.apple.com/documentation/storekit/skstorefront) govern the distribution and storefront checks. Remote policy controls do not bypass App Review.
