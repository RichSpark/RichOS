# Connect control protocol v1

The Worker allocates Cloudflare tunnels and DNS records. It never proxies conversations, message bodies or event streams. Cloudflare terminates public HTTPS. A supervised Mac connector forwards the tunnel to a dedicated phone-only listener on `127.0.0.1:18443`.

Every control request uses a P-256 Mac identity and signs the protocol marker, millisecond timestamp, random 128-bit nonce, HTTP method, path and SHA-256 of the exact body. See `connect/auth.mjs` for canonical encoding. Requests expire after 60 seconds. D1 rejects nonce replay. Host IDs derive from the public key; every operation additionally compares the full key.

Routes:

| Method and path | Purpose |
| --- | --- |
| GET /healthz | Non-secret configuration readiness |
| POST /v1/hosts | Enroll or retry enabling the signing Mac |
| GET /v1/host | Read the signing Mac's state |
| POST /v1/host/token | Retrieve that Mac's scoped tunnel token |
| PUT /v1/host/device | Record its paired public-key hash for the current generation |
| DELETE /v1/host | Disable and remove its allocation |

Any Mac with a valid signed identity enrolls; there is no admission step (CEO 2026-10-06: nothing is in a private pilot). `HOST_CAPACITY` is an atomic ceiling on lifetime host records: a slot is any row in `hosts`, of any phase. A full service answers `POST /v1/hosts` with 503 `capacity_reached`, makes no provider call and writes no row. Signed DELETE and scheduled reconciliation can always turn a host off. Disabled identities keep their record for ownership and retry safety. End users do not need a Cloudflare account. The operator's Tunnel Write and zone DNS Write credential exists only as the Worker secret `CF_API_TOKEN`.

D1 holds desired state before provisioning or teardown. A bounded lease serializes operations. Provider names include the host ID and generation; retries recover resource IDs after a lost response. Reenabling a fully disabled allocation creates a new generation and hostname. Cleanup checks ownership before deleting anything. Scheduled reconciliation retries interrupted work and repairs configuration/DNS drift. An externally deleted tunnel requires operator investigation; it is not silently replaced. No operation deletes unrelated records.

Phone authentication remains enforced on the Mac. The control service's device-key record is not an alternate authority. Local disable must close the listener and stop its connector before remote cleanup. Revoking a phone must close existing streams and persist local revocation. Tunnel tokens are secrets, never log fields or command-line arguments.

## Deployment

Apply `connect/schema.sql` to D1. Prepare an upload with `node richos/mobile/cli/mobile.mjs connect managed-artifact <private-profile.json>`. Keep account identifiers and operational evidence in the private repository. Upload the exact modules and metadata, preserve `secret_text` bindings and configure the returned schedule. Disable workers.dev, preview URLs, request logging and tracing. Verify source hashes, bindings and HTTPS health after deployment. Health indicates configuration readiness, not that a particular Mac is reachable.

The Free-plan rate limiter is a per-location abuse bound, not a global quota. `HOST_CAPACITY` bounds resource allocations independently. Conversation traffic bypasses Worker invocations.

## Recovery and rollback

A provider outage leaves a bounded error code on the host row and desired state intact. Existing cached connector credentials can keep the data path running. A failed disable denies new token retrieval immediately; scheduled reconciliation finishes deletion. Check the host row and owned tunnel/DNS records before intervening. Preserve identity and generation records during recovery.

To suspend new control operations, deploy the closed bootstrap artifact while retaining the D1 database and secret. This does not revoke existing tunnels. To revoke a host, first disable its local listener and connector, then use its signed DELETE request or explicitly remove its owned provider resources and record the operator action privately. Do not treat a Worker rollback as a conversation-access revocation.

Run `mobile-headless.test.sh` for signature, real SQLite schema, ownership, replay, capacity, interrupted provisioning and generation tests. Live provider and Mac/phone evidence is recorded separately; these headless tests do not establish tunnel or physical-device success.

## Part 4 notification boundary

The following route names are reserved and return 404 in protocol v1:

- `PUT /v1/host/notification-device`: register or rotate an APNs token for the current host generation and paired device public-key hash.
- `POST /v1/host/notifications`: request a generic reply-ready alert for that authorized phone.
- `DELETE /v1/host/notification-device`: remove the registration.

Part 4 must use the same signed host identity, replay checks and generation lease. Registration also needs proof from the paired phone's key; a caller-provided device ID is insufficient. Bind tokens to the official bundle ID and APNs environment. The Mac never receives the operator's APNs signing credential. Notification requests carry an idempotency ID and opaque thread/event references, with no prompt, transcript, title or reply body. Disable, phone replacement and revocation invalidate the registration. Retry jobs expire after one hour; invalid tokens are deleted immediately and redacted delivery diagnostics expire after seven days. These are integration requirements, not an implemented APNs service or advertised capability.

## Mac lifecycle and message safety

The signed Mac bundle includes a digest-pinned, separately signed Apache-2.0 cloudflared helper and its license. The connector binds only to the dedicated phone listener. A small child guard observes the Mac process's pipe: normal shutdown or abrupt process exit closes it, so the guard kills and reaps only its own helper. Restart backoff is bounded at 60 seconds. No USB, network-interface or unrelated process resets are used. Local configuration and scoped credentials survive restart; the private identity and token are held in the Mac's Keychain.

Phone commands retain their existing signed protocol. Challenges are reusable for bounded concurrency and SSE reconnection, valid for ten minutes. A durable receipt binds each logical message ID to its device and exact body. A lost acknowledgement or completed Mac restart replays the original receipt without a second intake. If the Mac crashes in the narrow interval where intake acceptance is uncertain, the phone retains the text and asks the person to inspect the conversation before resending. This is a deliberate safety hold, not a promise of unlimited exactly-once delivery.

The native client and PWA share connection diagnosis and queue semantics. Public service readiness does not prove that an individual Mac is reachable. An independent health probe can distinguish a reported service outage from an unreachable Mac; an ambiguous network error remains ambiguous. Existing paired clients do not need the control Worker in their normal message/SSE path.
