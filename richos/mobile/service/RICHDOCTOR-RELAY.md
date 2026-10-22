# RichDoctor relay, protocol 1

The relay turns a report the user approved in RichDoctor into one public GitHub issue in one fixed repository, so the user needs no GitHub account (RichDoctor PRD §7). It is the Worker `richos-report` at `https://report.richos.ceo`, with its own D1 database. It shares no code at runtime, binding, database or credential with Connect (`connect-worker.mjs`). Its only credential is a GitHub App private key held as a Worker secret; the App may write issues in the fixed repository and nothing else. No credential ships in any app.

Source: `report-worker.mjs` (main module), `report/checks.mjs`, `report/store.mjs`, `report/github.mjs`, `report/schema.sql`. Operator tool: `cli/report.mjs`. Tests: `test/report-relay.test.js` (Node, real SQLite, fake GitHub) and `test/report-relay-runtime.test.js` (the exact upload in workerd).

## Requests

JSON in and out, UTF-8. Clients time out each request after 30 seconds. Key on `state`, never on the HTTP status; a reply that is not JSON or has no known `state` is uncertain.

| Method and path | Purpose |
| --- | --- |
| `POST /v1/reports` | Create or repeat a create (never a second issue) |
| `POST /v1/reports/{report_id}/status` | Status by ID; body `{"receipt_secret": "..."}`. Never creates |
| `GET /v1/health` | `{"ok", "service": "richos-report", "protocol": 1, "accepting"}`; `ok` false (503) means not configured, `accepting` false means the operator switch is off or the cap is zero |

A create body has exactly these fields:

| Field | Rule |
| --- | --- |
| `schema` | `"richdoctor-report/1"` |
| `report_id` | `RD-XXXX-XXXX` from `23456789ABCDEFGHJKMNPQRSTVWXYZ`, random |
| `receipt_secret` | 43-character base64url of 32 random bytes. The relay keeps only its SHA-256. Never shown to the user, never in the issue |
| `content_sha256` | hex SHA-256 of the UTF-8 bytes of `title + "\n\n" + body` |
| `installation_id` | random UUID; a weak rate-limit key, never authentication, never stored |
| `app_version` | e.g. `1.0.0` |
| `approved_at` | integer milliseconds since the epoch when the user pressed Send. Not part of the hash |
| `title` | 1 to 200 characters, no control characters |
| `body` | at most 60,000 UTF-8 bytes; no control characters except newline and tab. Its last line is the footer |

The client persists the ID, receipt and exact bytes before its first request and keeps them until it has a definitive answer.

## Replies

| `state` | Meaning |
| --- | --- |
| `delivered` | `issue_url` and `issue_number` of the issue GitHub created or the relay found for this ID. The only state that means "Report submitted". The client checks `issue_url` against its configured repository |
| `pending` | An attempt is in progress or being reconciled. `check_after_seconds` (5). Poll status |
| `unconfirmed` | The 2-minute reconciliation bound passed without a confirmed issue. The report may exist. It is on the operator's list |
| `rejected` | Definitive non-creation. `reason`: `content`, `schema` or `size` (with a plain `detail` and, for content, `item`: `title`, `line N` or `body`) or `delivery` (GitHub definitively refused or the attempt stopped before anything was sent; may carry `retry_after_seconds`) |
| `closed` | Definitive non-creation. `reason`: `operator_off` or `daily_cap` (with `retry_after_seconds` to the next UTC midnight) |
| `unknown_id` | Status only: the relay has no record. Definitive, because the relay writes its record before it ever calls GitHub and keeps records for at least a year |
| `limited` | This request was not processed (throttled or a needed GitHub listing was incomplete). Says nothing about the report |
| `conflict` | The ID is bound to different bytes |
| `forbidden` | The receipt does not match the ID |

HTTP codes, for logs only: 201 first delivered, 200 replay or status, 202 pending, 400 schema, 413 size, 422 content, 409 conflict, 403 forbidden, 404 unknown_id, 429 limited, 503 closed or unavailable.

**The client's rules.** Only `delivered` shows "Report submitted". `rejected`, `closed` and `unknown_id` allow Try again with the same ID, receipt and bytes and allow approving a changed revision under a new ID. Everything else (pending, unconfirmed, limited, a timeout, no JSON) is uncertain: check status with the same ID, never create under a new one. Stop automatic polling 2 minutes after the send and show "Delivery could not be confirmed" with the report ID, Check status and Email support.

## How the relay decides

1. The request is throttled per IP and per installation (5 creates and 60 status requests per minute each, Cloudflare's per-location rate limiter).
2. The schema, sizes and fingerprint are checked.
3. **The record is looked up before anything else.** A known ID with the right receipt and bytes answers its real state (a pending record may move forward, see below), whatever the switch, cap or content rules say. A wrong receipt answers `forbidden` and other bytes answer `conflict`.
4. The content rules run. The relay accepts the exact words or rejects them; it never rewrites them.
5. The operator switch and the global daily cap (UTC day) are checked. An unknown ID approved more than 15 minutes ago is first looked for by its footer; if found it is recorded as delivered and if the listing is incomplete the request is `limited`.
6. A cap slot is taken and the **pending record is written before GitHub is called**, by one conditional statement, so concurrent requests for one ID serialize in D1 and exactly one owns the attempt. A retry is allowed only from a persisted `not_created` record, as one new attempt with the same ID, receipt and bytes.
7. The owner mints an installation token (a failure here is a definitive non-creation), records `sent` (only while its lease holds) and posts the issue with the fixed moderation label. The attempt is registered with `waitUntil`, so a client that disconnects or quits does not stop it.
8. A 201 with the matching issue URL is `delivered`. A 4xx is a definitive refusal. Anything else (lost connection, timeout, 5xx, redirect, malformed reply) is unknown: the relay never posts again, it looks for the footer.

**Reconciliation** lists the fixed repository's issues created by the App's bot (`creator` filter, `state=all`, `since` five minutes before the attempt), follows pagination up to ten pages and matches the exact footer line. It never uses search. A failed or incomplete listing or no match, is never proof of non-creation and never permits another create. A copied footer in an issue someone else made is ignored.

**A pending record moves forward only by status or a repeated create**: with `sent` not recorded after its lease (60 s), it becomes an authoritative non-creation; with `sent` recorded, the relay lists after the owner's grace (24 s) and at most every 10 s; at 2 minutes after the attempt it becomes `unconfirmed`. An unconfirmed report is listed at most once a minute, so a delayed success still returns its issue. Nothing runs on a schedule.

## Content rules

- The body's last line is exactly `Sent with RichDoctor <major.minor[.patch]> · Report ID <report_id>` (a U+00B7 middle dot between single spaces). It is the disclosed footer reconciliation looks for.
- Markdown is escaped as Bust a bug's `escaped_lines` writes it. Any `<`, `[` or `]` not escaped by an odd number of backslashes is refused, so there is no hidden image, link or HTML. Any `@` not followed by U+200B is refused (no mentions).
- Credential-shaped text is refused: GitHub, Slack, AWS, Anthropic and OpenAI-style tokens, PEM private-key blocks, JWTs, `Authorization:`, `Cookie:` and `Bearer` values, password-style assignments and the report's own receipt. Email addresses are refused, except `support@richos.ceo`. A `/Users/<name>` path is refused, except `/Users/Shared` and a scrubbed stand-in (`/Users/\[...`). These checks read the words as GitHub shows them (escapes removed, U+200B removed).
- The checks are linear in the body's length.

## What is stored

D1 holds, per report: the ID, the receipt's hash, the content hash, the state, the attempt number and its times, the `sent` mark, the refusal reason and retry time and the issue number and URL. Never the title, the body, the receipt itself, an IP address or an installation ID. Rows are kept (PRD §7: at least a year). The relay writes no log line; deploy it with Logs, Traces and Logpush off and no tail consumers.

## Operating it

The profile is private, identifiers only: `stage` (`production` or `test`), `accountId`, `databaseId`, `hostname`, `worker`, `repository`, `appId`, `installationId`, `botLogin`, `label`. A production profile must name exactly `richos-report`, `report.richos.ceo` and `WebDevBooster/richos`; a test profile may use none of them.

```sh
node richos/mobile/cli/mobile.mjs report artifact <profile.json>     # exact upload: modules, hashes, bindings, schema
RICHOS_REPORT_PROFILE=<profile.json> RICHOS_REPORT_TOKEN_FILE=<0600 D1 token> node richos/mobile/cli/mobile.mjs report schema
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report control on '<operator name>' <daily cap>
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report control off
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report pending
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report resolve <report ID> <issue number>
```

A new database starts switched off with a cap of zero; Rich names the operator and the initial cap and `control on` records both. `pending` is the operator's review list. `resolve` records an issue the operator found: it re-reads the issue and accepts it only if the relay's bot made it and its footer names that report. The operator never marks an unconfirmed report as not created; only the authoritative cases above permit another create.

Deploying is three commands with a short-lived Cloudflare token (Workers Scripts Edit and D1 Edit on the one account) and the GitHub App's private key, each in a mode-0600 file. The first reads the profile's bot login and installation ID from GitHub with the App's key and refuses an App that can do more than write issues or whose installation reaches any other repository:

```sh
RICHOS_REPORT_APP_KEY_FILE=… node richos/mobile/cli/mobile.mjs report app-identity <App ID> <owner/repository>
RICHOS_REPORT_TOKEN_FILE=… node richos/mobile/cli/mobile.mjs report create-database <account ID> <Worker name>   # once; put the ID in the profile
RICHOS_REPORT_PROFILE=… RICHOS_REPORT_TOKEN_FILE=… RICHOS_REPORT_APP_KEY_FILE=… node richos/mobile/cli/mobile.mjs report deploy
```

`deploy` applies the schema, uploads the artifact's exact `metadata` and `modules` with Cloudflare's Scripts API (`keep_bindings: ["secret_text"]` keeps the key across uploads), installs the key as the Worker secret `GITHUB_APP_PRIVATE_KEY` (GitHub's PKCS#1 download is accepted as is), sets workers.dev (on only for a test relay whose hostname is its workers.dev name, which needs no DNS), reads back the bindings and the logging settings and waits for `GET /v1/health`. A production relay then gets its Custom Domain. Create the moderation label in the repository first, then turn the relay on with `control on`. Account identifiers and deployment receipts belong in the private operations record.

## Testing it

`node --test richos/mobile/test/report-relay.test.js` covers the rules on real SQLite. `node --test richos/mobile/test/report-relay-runtime.test.js` runs the exact upload in workerd through the Wrangler-installed Miniflare (or `RICHOS_MINIFLARE`) with a persisted D1, a 4-second window and a fake GitHub that checks the App's JWT: a client gone before and after acceptance, GitHub creating the issue and the relay restarting before it answered, retries and edits during uncertainty, lost listings into operator review and a late find, a refusal then Try again, the largest body, the cap and the switch. It is skipped, with that reason, where no local runtime exists. These prove the relay's logic; they do not prove delivery to GitHub.

`node richos/mobile/dev/report-relay-local.mjs <empty state directory>` runs the same upload as a loopback stand-in for the RichDoctor client and prints its relay base, the test repository and a control base for injecting faults.

`dev/report-a9-live.mjs` runs acceptance A9 against a deployed **test** relay and its real test repository. It refuses a production profile and it turns the test relay's switch off when it ends.
