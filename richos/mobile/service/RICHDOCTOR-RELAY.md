# RichDoctor relay, protocol 2

The relay turns a report the user approved in RichDoctor into one public GitHub issue in one fixed repository, so the user needs no GitHub account. The user's approved screenshots go privately to RichOS support: they are kept in a private R2 bucket and the issue links them for staff only (RichDoctor PRD v6 §6-§8, criteria C2 and C3). It is the Worker `richos-report` at `https://report.richos.ceo`, with its own D1 database and its own bucket. It shares no code at runtime, binding, database, bucket or credential with Connect (`connect-worker.mjs`). Its only credential is a GitHub App private key held as a Worker secret; the App may write issues in the fixed repository and nothing else. No credential ships in any app.

Source: `report-worker.mjs` (main module), `report/checks.mjs`, `report/images.mjs`, `report/store.mjs`, `report/github.mjs`, `report/access.mjs`, `report/schema.sql`, golden screenshots in `report/fixtures/`. Operator tool: `cli/report.mjs`. Tests: `test/report-relay.test.js` and `test/report-screenshots.test.js` (Node, real SQLite, fake GitHub, R2 and Access) and `test/report-relay-runtime.test.js` (the exact upload in workerd, with local D1 and R2).

## Requests

JSON in and out, UTF-8, unless stated. Key on `state`, never on the HTTP status; a reply that is not JSON or has no known `state` is uncertain. Create, status, complete and retire time out after 30 seconds on the client; an upload uses a 60-second no-progress timeout and a 5-minute cap.

| Method and path | Purpose |
| --- | --- |
| `POST /v1/reports` | Register (protocol 2) or create in one step (protocol 1, text only). Never a second issue |
| `PUT /v1/reports/{report_id}/attachments/{n}` | Upload screenshot n: the exact approved bytes, headers `X-Receipt-Secret`, `Content-Type`, `X-Content-SHA256` |
| `POST /v1/reports/{report_id}/complete` | `{"receipt_secret"}`. Creates the issue, only when every approved screenshot is stored |
| `POST /v1/reports/{report_id}/status` | `{"receipt_secret"}`. Never creates |
| `POST /v1/reports/{report_id}/retire` | `{"receipt_secret"}`. Retire this ID before sending a changed revision |
| `GET /v1/reports/{report_id}/attachments/{n}` | Staff only, behind Cloudflare Access: the image itself |
| `GET /v1/health` | `{"ok", "service": "richos-report", "protocol": 2, "accepting"}` |

The receipt secret never appears in a URL, the issue or a log.

### Register body

| Field | Rule |
| --- | --- |
| `schema` | `"richdoctor-report/2"` (`"richdoctor-report/1"`: the same fields without `manifest`, created at once) |
| `report_id` | `RD-XXXX-XXXX` from `23456789ABCDEFGHJKMNPQRSTVWXYZ`, random |
| `receipt_secret` | 43-character base64url of 32 random bytes. The relay keeps only its SHA-256 |
| `content_sha256` | hex SHA-256 of the UTF-8 bytes of `title + "\n\n" + body + "\n\n" + M`, where M is the manifest as a JSON array of objects whose keys are sorted, with no spaces (`[]` when empty). Protocol 1: `title + "\n\n" + body` |
| `installation_id` | random UUID; a weak rate-limit key, never authentication, never stored |
| `app_version` | e.g. `1.0.0` |
| `approved_at` | integer milliseconds since the epoch when the user pressed Send. Not hashed |
| `title` | 1 to 200 characters, no control characters |
| `body` | at most 60,000 UTF-8 bytes; no control characters except newline and tab |
| `manifest` | 0 to 10 items `{"attachment_id": "n", "bytes", "filename": "screenshot-n.png" or ".jpg", "height", "media_type": "image/png" or "image/jpeg", "order": n, "sha256", "width"}`, n = 1..k in order. Each at most 5 MiB (5,242,880 bytes) and 40 megapixels, together at most 25 MiB |

When k ≥ 1, the lines just before the footer are exactly `Screenshots: k, shared privately with RichOS support (staff sign-in required)` and, for each n, `n. <relay origin>/v1/reports/<report_id>/attachments/n` (the CEO-approved wording, design round 40). The origin is the one the client posts to. The client persists the ID, receipt, exact text and exact image bytes before its first request.

## Replies

Every reply about a report carries `report_id`, `state`, `stage` (receiving, ready, creating, unconfirmed, delivered, created_checking_images, not_sent or retired) and the screenshot lists `stored` and `missing` (attachment IDs).

| `state` | Meaning |
| --- | --- |
| `receiving` | Registered; some approved screenshots are not stored. Upload the `missing` ones |
| `ready` | Every approved screenshot is stored. Complete it |
| `pending` | Creation began and is not yet confirmed. `check_after_seconds` (5). Poll status |
| `unconfirmed` | The 2-minute reconciliation bound passed without a confirmed issue. It may exist. It is on the operator's list |
| `delivered` | The issue (`issue_url`, `issue_number`) exists AND every approved screenshot is stored: `images_confirmed` = `images_expected`, kept until `images_expire_at` (one year). After that, `images_expired: true`. The only state that means "Report submitted". The client checks `issue_url` against its configured repository |
| `created_checking_images` | The issue exists, but fewer screenshots are confirmed than approved: "Report created; checking screenshots". Never created again |
| `rejected` | Definitive non-creation. `reason`: `content`, `schema` or `size` (with a plain `detail` and an `item`), `attachment` (an upload refused: `item` "screenshot n") or `delivery` (GitHub definitively refused or the attempt stopped before anything was sent; may carry `retry_after_seconds`) |
| `closed` | Definitive non-creation. `reason`: `operator_off` or `daily_cap` (reports or stored screenshot bytes), with `retry_after_seconds` when known |
| `retired` | This ID was retired. Nothing more is ever created, stored or shown for it |
| `unknown_id` | The relay has no record. Definitive, because the relay writes its record before it ever calls GitHub and keeps records for at least a year |
| `limited` | This request was not processed (throttled, the receipt was busy, a cleanup was running or a needed listing was incomplete). Says nothing about the report. Send the same request again after `retry_after_seconds` |
| `conflict` | The ID is bound to different bytes or a different manifest |
| `forbidden` | The receipt does not match the ID |

HTTP codes, for logs only: 201 first delivered, 200 replay or status, 202 pending, 400 schema, 413 size, 422 content or attachment, 409 conflict, 403 forbidden, 404 unknown_id, 429 limited, 503 closed or unavailable.

**The client's rules.** One Send report is: register, upload each screenshot, complete. Only `delivered` shows "Report submitted". `rejected` and `closed` allow Try again with the same ID, receipt and bytes. To send a CHANGED revision, first retire the old ID and wait for `retired`; if the retire reply is lost or refused, keep the revision unsent and check the old ID's status. An identical registration retry answers the current state. Anything else is uncertain: check status with the same ID, never create under a new one. Stop automatic polling 2 minutes after the completion request and show "Delivery could not be confirmed".

## How the relay decides

1. Requests are throttled per IP and per installation (5 creates, 60 status or retire requests and 30 uploads per minute, Cloudflare's per-location limiter).
2. The schema, sizes and fingerprint are checked.
3. **The record is looked up first.** A known ID with the right receipt and bytes answers its real state, whatever the switch, cap or content rules say. A retired ID answers `retired`. A wrong receipt answers `forbidden`, other bytes `conflict`.
4. The content rules run. The relay accepts the exact words or rejects them; it never rewrites them.
5. The operator switch is checked. An unknown ID approved more than 15 minutes ago is first looked for by its footer; if found it is recorded as delivered.
6. Registration stores the receipt, the words (only until the report is posted or retired) and the manifest in one transaction. It never calls GitHub.
7. **One per-receipt lock** serializes upload, completion, retirement and cleanup.
   - An upload is checked before the lock: the receipt, the type, the exact size and hash and the image profile with the manifest's width and height. Under the lock, the object is written once per hash (an identical retry finds it and only records it), then marked stored.
   - Completion verifies every approved screenshot in the bucket itself. A stored flag is never taken as proof, because a database write can fail after the object is stored. A missing one turns the receipt back to `receiving` with its name. Otherwise completion takes a cap slot and claims the creation attempt (the durable pending record), releases the lock and calls GitHub.
8. The attempt mints an installation token (a failure here is a definitive non-creation), records `sent` (only while its lease holds) and posts the issue with the fixed moderation label. The whole request is registered with `waitUntil` before its first step, so a client that disconnects after sending does not stop it. A 201 with the matching issue URL is delivered and a 4xx is a definitive refusal. Anything else is unknown: the relay never posts again, it looks for the footer.

**Reconciliation** lists the fixed repository's issues created by the App's bot (`creator` filter, `state=all`, newest first) back to five minutes before the attempt, judged by each issue's own creation time. It follows pagination up to ten pages and matches the exact footer line. It never uses search or GitHub's `since` filter, which answered an empty list for minutes-old issues on 2026-10-22. A failed or incomplete listing or no match, never permits another create. A copied footer in someone else's issue is ignored.

**A pending record moves forward only by status or a repeated request.** If `sent` is not recorded after its lease (60 s), it becomes an authoritative non-creation. If `sent` is recorded, the relay lists after the owner's grace (24 s), at most every 10 s and at 2 minutes after the attempt the report becomes `unconfirmed`. An unconfirmed report is listed at most once a minute, so a delayed success still returns its issue.

## Screenshots: profile, storage, access, retention

**Shared profile, version 1** (`report/images.mjs`, checked from structure only, never by decoding pixels):
- PNG: the signature; IHDR first; then only PLTE, IDAT, tRNS, sRGB, gAMA and pHYs; IEND last, with nothing after it.
- JPEG: SOI; then only SOF0-3, SOF5-7, SOF9-11, SOF13-15, DHT, DQT, DRI, DNL, SOS with its entropy-coded data and RST0-7 and at most one APP0 whose identifier is `JFIF\0`; EOI last, with nothing after it. No COM, APP1-APP15 or JFXX.
- The width and height in IHDR/SOF must equal the manifest.

Golden files: `report/fixtures/relay/accept-*` and `reject-*` (made by `dev/report-image-fixtures.mjs`) and the client encoder's own in `report/fixtures/client/`. Every test reads both folders.

**Storage:** one private R2 bucket bound as `IMAGES`, with public access off. Objects are keyed by report ID, attachment ID and hash and are never transformed.

**Staff access:** every image read needs a Cloudflare Access JWT (`Cf-Access-Jwt-Assertion`). The Worker verifies its RS256 signature against the team's published keys, its issuer, audience and expiry, on every hostname, so a direct workers.dev request is refused. A report ID or link grants nothing. Images are served with their fixed type, `nosniff` and `private, no-store`, with no page, listing or presigned link. A deleted image answers 410 with the reason. A retired report's images answer 410 at once.

**Retention and cleanup** run hourly from the Worker's own schedule:
- Delivered screenshots are kept one year after delivery. The operator can extend that before expiry; an extension is refused once the cleanup has been claimed.
- Uploads of a report never begun (or refused) are deleted after seven days without activity and a retired report's at the next run. Nothing is deleted while creation is active or uncertain.
- A cleanup is claimed under the receipt's lock, marks the images unavailable before deleting anything and blocks uploads and completion until it has finished. A run that stopped midway is resumed first.
- After an abandoned upload is cleaned up, the same manifest can be uploaded again and completed.

## What is stored

D1 holds, per report: the ID, the receipt's hash, the content hash, the state, the words until posting or retirement, the attempt and its times, the lock, the cleanup mark, the issue number and URL and the delivery and expiry times. Per screenshot it holds the manifest entry and whether it is stored or deleted. It never holds the receipt itself, an IP address or an installation ID. The relay writes no log line; deploy it with Logs, Traces and Logpush off and no tail consumers.

## Operating it

The profile is private, identifiers only: `stage` (`production` or `test`), `accountId`, `databaseId`, `hostname`, `worker`, `repository`, `bucket`, `appId`, `installationId`, `botLogin`, `label` and for staff reads `accessTeamDomain` (`https://<team>.cloudflareaccess.com`) and `accessAud`. A production profile must name exactly `richos-report`, `report.richos.ceo`, `WebDevBooster/richos` and the bucket `richos-report`; a test profile may use none of them.

```sh
node richos/mobile/cli/mobile.mjs report artifact <profile.json>     # exact upload: modules, hashes, bindings, schedule, schema
RICHOS_REPORT_APP_KEY_FILE=… node richos/mobile/cli/mobile.mjs report app-identity <App ID> <owner/repository>
RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report create-database <account ID> <Worker name>
RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report create-bucket <account ID> <bucket name>
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… RICHOS_REPORT_APP_KEY_FILE=… node richos/mobile/cli/mobile.mjs report deploy
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report control on '<operator name>' <daily reports> <daily screenshot MB>
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report control off
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report pending
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report resolve <report ID> <issue number>
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report extend <report ID> <days from now>
```

- The Cloudflare token needs Workers Scripts Edit, D1 Edit and Workers R2 Storage Edit on the one account, short-lived, in a mode-0600 file.
- `app-identity` reads the bot login and installation ID with the App's own key. It refuses an App that can do more than write issues or whose installation reaches any other repository.
- `deploy` applies the schema and uploads the exact `metadata` and `modules` (`keep_bindings: ["secret_text"]` keeps the key). It installs the key as the secret `GITHUB_APP_PRIVATE_KEY` (GitHub's PKCS#1 download is accepted as is), sets workers.dev (on only for a test relay named by its workers.dev host), sets the hourly schedule, reads back the bindings and logging settings and waits for health.
- A production relay then gets its Custom Domain and its Access application.
- A new database starts switched off with caps of zero. Rich names the operator and the caps and `control on` records them.
- `pending` is the operator's review list. `resolve` accepts only an issue the relay's bot made with that report's footer.
- Account identifiers and deployment receipts belong in the private operations record.

## Testing it

The Node tests cover the rules. The workerd test runs the exact upload with local D1 and R2 and proves A9:
- a 5 MiB screenshot at 1 Mbit/s accepted after more than 30 seconds;
- a cut upload, a lost acknowledgment and GitHub creating the issue before a relay restart;
- cleanup racing upload and completion;
- retirement, including an ID the relay never saw;
- staff reads with and without a valid token.

`node richos/mobile/dev/report-relay-local.mjs <empty state directory>` runs the same upload as a loopback stand-in for the RichDoctor client. `dev/report-a9-live.mjs` runs A9 against a deployed **test** relay and its real test repository. It refuses a production profile and turns the test relay's switch off when it ends.
