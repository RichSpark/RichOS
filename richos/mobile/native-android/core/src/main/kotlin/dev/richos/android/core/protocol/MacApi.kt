package dev.richos.android.core.protocol

import dev.richos.android.core.ConversationThread
import dev.richos.android.core.TransportFailure
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive
import java.io.IOException

/** One HTTP exchange. Header names in a response are lowercase. */
class HttpRequest(val method: String, val url: String, val headers: Map<String, String>, val body: ByteArray?)

class HttpResponse(val status: Int, val headers: Map<String, String>, val body: ByteArray) {
    val text: String get() = String(body, Charsets.UTF_8)
}

/** The wire. A transport-level failure (no route, timeout, TLS) throws [IOException]. */
fun interface Http {
    suspend fun send(request: HttpRequest): HttpResponse
}

/**
 * The phone's P-256 identity, one key per paired origin (contract §2.2). On Android this is a
 * Keystore key the app cannot export; [sign] returns what the platform returns (ASN.1 DER).
 */
interface DeviceKeys {
    /** The 65-byte uncompressed public point, creating the key if there is none for [origin]. */
    suspend fun publicPoint(origin: String): ByteArray

    /** Throws [MissingIdentity] when this phone holds no key for [origin]. */
    suspend fun sign(origin: String, data: ByteArray): ByteArray

    suspend fun delete(origin: String)
}

/**
 * This phone has no identity for [origin] any more (the key is gone from the Keystore, or can no
 * longer be used): it cannot prove who it is, so it must pair again.
 */
class MissingIdentity(origin: String) : IOException("no identity for $origin; pair again")

/** The 200 answer to a pairing code (contract §2.3). Unknown keys are ignored. */
@Serializable
data class PairAnswer(
    @SerialName("device_id") val deviceId: String,
    @SerialName("ca_fingerprint_sha256") val caFingerprint: String,
    val challenge: String,
    @SerialName("api_base") val apiBase: String,
    @SerialName("thread_id") val threadId: String? = null,
    val threads: List<ConversationThread> = emptyList(),
    /** Additive on newer Macs (Echo 194fcb75); absent on older ones. */
    val capabilities: List<String>? = null,
    @SerialName("protocol_version") val protocolVersion: Long? = null,
    @SerialName("attachment_limits") val attachmentLimits: dev.richos.android.core.AttachmentLimits? = null,
    val build: String? = null,
    /** Pairing v2: how long the Mac keeps its "They match" press open ([MacWait.boundMs]). */
    @SerialName("confirm_within_seconds") val confirmWithinSeconds: Double? = null,
    @SerialName("pairing_version") val pairingVersion: Long? = null,
)

/**
 * The capability a v2 phone requires (Sage's pairing review §3.5). A Mac that does not name it
 * derives the old six words, which bind nothing about the connection, and a relay can strip a
 * capability, so this phone refuses such a Mac rather than falling back (`api.js` `PAIR_V2`).
 */
const val PAIR_V2 = "pair-v2"

/**
 * The Mac answered the pairing code but does not offer [PAIR_V2]: refused, final, never fallen
 * back to. [answer] is kept so the caller can sign the one `fingerprint_confirmed: false` that
 * makes that Mac forget the key it just registered (`api.js` `pair`, `macNeedsUpdate`).
 */
class MacNeedsPairV2(val answer: PairAnswer) : dev.richos.android.core.TransportFailure("refused", retryable = false)

/**
 * **HOW THIS PHONE WAITS FOR THE PRESS ON THE MAC** (review §3.1 step 5; CEO ruling §81). The
 * schedule is `api.js`'s `macWaitDelayMs` and `macWaitBoundMs`, and the conformance corpus's
 * `mac_confirmation.wait_schedule_ms`: 2, 3, 5, 8 and 13 s, then every 15 s, never past the bound
 * the Mac gave (`confirm_within_seconds`, at most the five-minute window), and only while the app
 * is on screen. 2 + 3 + 5 + 8 + 13 = 31 s, then floor((300 - 31) / 15) = 17 more: at most
 * [MAX_REQUESTS] = 22 in the whole window (the corpus's `max_requests_in_the_window`).
 *
 * **With `pair_wait` (Echo's handoff, 2026-09-24)** that schedule is what a Mac WITHOUT [PAIR_WAIT]
 * gets between the press (the first ask) and one last ask at the deadline: 24 asks in all. A Mac
 * naming it holds each ask up to [HOLD_SECONDS_MAX] and the asks keep [MIN_SPACING_MS] apart
 * ([nextDelayMs], [nextAskAt]); whatever a Mac or relay does, never more than [MAX_ASKS].
 */
object MacWait {
    val DELAYS_MS: List<Long> = listOf(2_000, 3_000, 5_000, 8_000, 13_000)
    const val CAP_MS = 15_000L
    const val WINDOW_MS = 300_000L
    const val MAX_REQUESTS = 22

    /** The wait before ask number [attempt] (0-based), counted from the previous one. */
    fun delayMs(attempt: Int): Long = if (attempt in DELAYS_MS.indices) DELAYS_MS[attempt] else if (attempt < 0) DELAYS_MS[0] else CAP_MS

    /** A Mac that says nothing gets the window; a Mac that says more than the window is not believed. */
    fun boundMs(confirmWithinSeconds: Double?): Long {
        val s = confirmWithinSeconds ?: return WINDOW_MS
        return if (s.isFinite() && s > 0) minOf((s * 1000).toLong(), WINDOW_MS) else WINDOW_MS
    }

    /**
     * **THE MAC CAN HOLD THIS PHONE'S ASK UNTIL THE PRESS** (`api.js` `PAIR_WAIT_SECONDS`; the
     * corpus's `pair_wait.hold_seconds_max`). A Mac naming [PAIR_WAIT] is sent `Prefer: wait=N`
     * with N at most this, and answers the moment the person presses on the Mac. Why 14: the event
     * stream already crosses both routes with at most 15 s between bytes (Sage's review §1).
     */
    const val HOLD_SECONDS_MAX = 14

    /**
     * **THE NO-SPIN RULE WHILE THE MAC HOLDS** (`api.js` `MAC_WAIT_MIN_SPACING_MS`;
     * `pair_wait.min_ask_spacing_ms`): two asks never start closer than this, so a relay that
     * forges `pair-wait` and answers at once gets one ask every 7 s at most, never a tight loop.
     */
    const val MIN_SPACING_MS = 7_000L

    /**
     * A ceiling this phone keeps whatever any Mac or relay does, derived rather than chosen: while
     * a Mac holds, one ask per [MIN_SPACING_MS] in the whole window, floor(300 / 7) + 1 = 43, then
     * the last ask = 44. The corpus's plans make 23 and 24; a Mac that does not hold, 24.
     */
    const val MAX_ASKS = (WINDOW_MS / MIN_SPACING_MS).toInt() + 1 + 1

    /**
     * **HOW LONG AFTER AN ANSWER THE NEXT ASK GOES** (`api.js` `macWaitNextDelayMs`;
     * `pair_wait.next_delay_cases`). [attempt] counts from 0 at the press; [tookMs] is how long the
     * previous ask took; [holds] is whether the Mac named [PAIR_WAIT].
     *  - Not holding: today's schedule, [delayMs], unchanged.
     *  - Holding, and the answer took 7 s or more: at once (the next hold starts where this ended).
     *  - Holding, and it came sooner: the schedule, but never less than keeps the asks 7 s apart.
     */
    fun nextDelayMs(attempt: Int, tookMs: Long, holds: Boolean): Long {
        val scheduled = delayMs(attempt)
        if (!holds) return scheduled
        val took = maxOf(0L, tookMs)
        if (took >= MIN_SPACING_MS) return 0L
        return maxOf(scheduled, MIN_SPACING_MS - took)
    }

    /**
     * **WHEN THE NEXT ASK GOES**, on the clock of its arguments (`api.js` `macWaitNextAskAt`;
     * `pair_wait.last_ask_cases`): [nextDelayMs] after the answer, never later than [until], where
     * the LAST ask goes (Sage's review §2). While the Mac holds, that last ask still keeps the 7 s
     * spacing, so it may go a few seconds after [until]; its answer is as settled, because the
     * phone's deadline is at or after the Mac's own `confirm_by` (Sage §2b).
     */
    fun nextAskAt(attempt: Int, askedAt: Long, answeredAt: Long, until: Long, holds: Boolean): Long {
        val next = answeredAt + nextDelayMs(attempt, answeredAt - askedAt, holds)
        if (next <= until) return next
        return if (holds) maxOf(until, askedAt + MIN_SPACING_MS) else until
    }

    /** `Prefer: wait=N` for an ask at [now]: whole seconds left to [until], at most [HOLD_SECONDS_MAX]; 0 sends none. */
    fun preferWaitSeconds(now: Long, until: Long): Int =
        if (now >= until) 0 else minOf(HOLD_SECONDS_MAX.toLong(), (until - now) / 1000).toInt()
}

/** The capability a Mac names when it can hold the phone's "They match" until the press (`pair_wait.capability`). */
const val PAIR_WAIT = "pair-wait"

/** The Mac's answer to "They match": whether it is still waiting for its own press, and the newest challenge. */
class Confirmation(val awaitingMac: Boolean, val challenge: String)

/**
 * Native push registration (contract §7.2; Echo 65952d16), the FCM shape: `platform: "fcm"`, no
 * `environment`, `topic` = the Android application id. Null unregisters.
 */
@Serializable
data class NativePush(
    val platform: String = "fcm",
    val token: String,
    val topic: String,
    @SerialName("preview_key") val previewKey: String? = null,
    val previews: Boolean = true,
)

@Serializable
data class PushAnswer(@SerialName("host_id") val hostId: String? = null, val registered: Boolean = false)

/** A signed exchange's outcome and the newest challenge it taught us. */
class Signed(val response: HttpResponse, val challenge: String)

/**
 * The Mac, as the phone speaks to it (contract §2–§4): unsigned pairing, signed requests with the
 * one 404 recovery (re-sign once with the fresh challenge the refusal carried), and the reference
 * client's classification of every failure (`web/lib/api.js`): transport error → `unreachable`,
 * retryable; 403 `{"revoked":true}` → `revoked`, final; 429 → `rate-limited`, retryable; a 404
 * after the one re-sign → `refused`, final; a body with `retry:false` → `refused`, final; any
 * other non-2xx → `fault`, retryable.
 */
class MacApi(private val http: Http, private val keys: DeviceKeys) {
    private val lenient = Json { ignoreUnknownKeys = true }

    /**
     * The pairing code, from a v2 phone (Sage's pairing review §3; the corpus's `pair_v2`): the
     * body is `native_v2_body_fields`, `code`, `public_key_jwk`, `device_name`, `platform`,
     * `pairing_version: 2`, in that order, unsigned. A Mac whose answer does not list [PAIR_V2] is
     * refused with [MacNeedsPairV2] and never fallen back to. [point] is this phone's key for the
     * link's origin, the one the six words name.
     */
    suspend fun pair(link: PairLink, deviceName: String, point: ByteArray): PairAnswer {
        val jwk = Signing.publicJwk(point).entries.joinToString(",", "{", "}") { (k, v) -> "\"$k\":${JsonPrimitive(v)}" }
        val body = "{\"code\":${JsonPrimitive(link.code)},\"public_key_jwk\":$jwk,\"device_name\":${JsonPrimitive(deviceName)},\"platform\":\"android\",\"pairing_version\":2}"
        val response = exchange(HttpRequest("POST", link.origin + "/api/pair", mapOf("Content-Type" to "application/json"), body.toByteArray()))
        if (response.status != 200) throw classify(response, afterResign = true)
        val answer = try {
            lenient.decodeFromString(PairAnswer.serializer(), response.text)
        } catch (e: SerializationException) {
            throw TransportFailure("fault", retryable = true)
        }
        if (PAIR_V2 !in answer.capabilities.orEmpty()) throw MacNeedsPairV2(answer)
        return answer
    }

    /**
     * A signed request. [pathWithQuery] is the wire form. Returns the answer and the newest
     * challenge; throws [TransportFailure] classified as above. [unsigned] headers travel outside
     * the signature (`Prefer` is the only one).
     */
    suspend fun signed(
        apiBase: String, deviceId: String, challenge: String, method: String, pathWithQuery: String, body: ByteArray?,
        contentType: String? = null, unsigned: Map<String, String> = emptyMap(),
    ): Signed {
        var current = challenge
        repeat(2) { attempt ->
            val raw = Signing.derToRaw(signature(apiBase, Signing.signingString(current, method, pathWithQuery, body).toByteArray(Charsets.UTF_8)))
            val headers = buildMap {
                put("Authorization", Signing.authorization(deviceId, current, raw))
                if (contentType != null) put("Content-Type", contentType)
                putAll(unsigned)
            }
            val response = exchange(HttpRequest(method, apiBase + pathWithQuery, headers, body))
            val fresh = response.headers["x-richos-challenge"]
            if (response.status in 200..299) return Signed(response, fresh ?: current)
            if (response.status == 404 && attempt == 0 && fresh != null && fresh != current) {
                current = fresh
                return@repeat
            }
            throw classify(response, afterResign = true)
        }
        throw TransportFailure("refused", retryable = false)
    }

    /**
     * "They match" (true) or "They do not match" (false), contract §2.5. With "They match" an
     * Android phone names FCM as its push service when the Mac takes FCM registrations
     * ([fcm]; the corpus's `push_transport_on_confirmation`). A v2 Mac answers "They match" with
     * `{"ok":true,"awaiting_mac_confirmation":true}` until the person presses They match on the
     * Mac too (the corpus's `mac_confirmation.phone_answer_while_waiting`).
     */
    suspend fun confirm(apiBase: String, deviceId: String, challenge: String, match: Boolean, fcm: Boolean = false): Confirmation =
        confirmation(signed(apiBase, deviceId, challenge, "POST", "/api/pair", answerBody(deviceId, match, fcm), "application/json"))

    /** The phone's answer to the six words, as the Mac reads it: the one body [confirm] and [macAnswer] both send. */
    private fun answerBody(deviceId: String, match: Boolean, fcm: Boolean): ByteArray {
        val transport = if (match && fcm) ",\"push_transport\":\"fcm\"" else ""
        return "{\"device_id\":${JsonPrimitive(deviceId)},\"fingerprint_confirmed\":$match$transport}".toByteArray()
    }

    private fun confirmation(signed: Signed): Confirmation {
        val json = runCatching { lenient.parseToJsonElement(signed.response.text) as? JsonObject }.getOrNull()
        val awaiting = (json?.get("awaiting_mac_confirmation") as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull == true
        return Confirmation(awaiting, signed.challenge)
    }

    /**
     * **ASK THE MAC WITH THE ANSWER, AND LET IT HOLD THE ASK** (`api.js` `macAnswer`; the corpus's
     * `pair_wait.asks`; Sage's pair-v2 hypotheses review §1 point 1, the fix for his §2c). The wait
     * for the press on the Mac sends THIS phone's own signed "They match" again, byte for byte the
     * body [confirm] sends at the press, instead of reading a backfill row. Every Mac with the press
     * answers it on every outcome, and a lost first "They match" is recorded by the same request
     * that learns of the press (without it, notifications silently never work: `trusted()` false).
     *
     * [waitSeconds] > 0 adds `Prefer: wait=N`, UNSIGNED and outside the signature: send it only to a
     * Mac naming [PAIR_WAIT]. The awaiting answer, as a 200 or a 409, is "still waiting"; every
     * other failure is thrown as classified, so a refusal is the final answer it is.
     */
    suspend fun macAnswer(apiBase: String, deviceId: String, challenge: String, fcm: Boolean, waitSeconds: Int): Confirmation {
        val prefer = if (waitSeconds > 0) mapOf("Prefer" to "wait=${minOf(waitSeconds, MacWait.HOLD_SECONDS_MAX)}") else emptyMap()
        return try {
            confirmation(signed(apiBase, deviceId, challenge, "POST", "/api/pair", answerBody(deviceId, true, fcm), "application/json", prefer))
        } catch (e: TransportFailure) {
            if (e.awaitingMac) Confirmation(true, e.challenge ?: challenge) else throw e
        }
    }

    /**
     * **HAS THE PERSON PRESSED "They match" ON THE MAC YET?** (`api.js` `macConfirmed`). One signed
     * read of the smallest thing a paired phone may read, one backfill row
     * (`/api/events?thread_id=…&before=0&limit=1`), which the Mac answers with the awaiting 409
     * until the press. True once it is answered, false while the Mac still waits (with the fresh
     * challenge the 409 carried); every other failure is thrown as it is, so a refusal (the person
     * pressed "They do not match" on the Mac, or the window closed) is the final answer it is.
     */
    suspend fun macConfirmed(apiBase: String, deviceId: String, challenge: String, threadId: String?): Pair<Boolean, String> {
        val thread = threadId?.let { "thread_id=${Signing.encodeURIComponent(it)}&" } ?: ""
        return try {
            true to signedQuery(apiBase, deviceId, challenge, "/api/events?${thread}before=0&limit=1").challenge
        } catch (e: TransportFailure) {
            if (e.awaitingMac) false to (e.challenge ?: challenge) else throw e
        }
    }

    /**
     * A signed GET on `/api/events`, whose credential travels as the LAST query parameter `auth`
     * and is not part of the signed path (contract §3.2, §5.5). Same 404 recovery as [signed].
     */
    /**
     * The `Authorization` header for one signed GET, and nothing sent: for a file whose answer
     * arrives in parts (round 36 screen 3), read through the app's streaming GET so the ring fills.
     */
    suspend fun authorization(apiBase: String, deviceId: String, challenge: String, pathWithQuery: String): String {
        val raw = Signing.derToRaw(signature(apiBase, Signing.signingString(challenge, "GET", pathWithQuery, null).toByteArray(Charsets.UTF_8)))
        return Signing.authorization(deviceId, challenge, raw)
    }

    suspend fun signedQuery(apiBase: String, deviceId: String, challenge: String, pathWithQuery: String): Signed {
        var current = challenge
        repeat(2) { attempt ->
            val raw = Signing.derToRaw(signature(apiBase, Signing.signingString(current, "GET", pathWithQuery, null).toByteArray(Charsets.UTF_8)))
            val target = Signing.withAuthQuery(pathWithQuery, Signing.authorization(deviceId, current, raw))
            val response = exchange(HttpRequest("GET", apiBase + target, emptyMap(), null))
            val fresh = response.headers["x-richos-challenge"]
            if (response.status in 200..299) return Signed(response, fresh ?: current)
            if (response.status == 404 && attempt == 0 && fresh != null && fresh != current) {
                current = fresh
                return@repeat
            }
            throw classify(response, afterResign = true)
        }
        throw TransportFailure("refused", retryable = false)
    }

    /** Older rows of one conversation (contract §5.5): `{"messages":[row…],"more":bool}`, oldest first. */
    suspend fun backfill(apiBase: String, deviceId: String, challenge: String, threadId: String, before: Long, limit: Int = 50): Pair<Backfill, String> {
        val path = "/api/events?thread_id=${Signing.encodeURIComponent(threadId)}&before=$before&limit=$limit"
        val signed = signedQuery(apiBase, deviceId, challenge, path)
        val answer = runCatching { lenient.decodeFromString(Backfill.serializer(), signed.response.text) }.getOrElse { throw TransportFailure("fault", retryable = true) }
        return answer to signed.challenge
    }

    /** The device key's signature over [data] (ASN.1 DER, as the platform returns it). */
    suspend fun sign(apiBase: String, data: ByteArray): ByteArray = signature(apiBase, data)

    /**
     * Every signature goes through here, so no signing failure escapes as anything but a
     * [TransportFailure]. A phone with no identity for the Mac cannot prove who it is: that is the
     * Mac's `revoked` (final: pair again, the removed-from-Mac path), never a crash. Any other
     * failure of the key store is a fault, retried like one.
     */
    private suspend fun signature(apiBase: String, data: ByteArray): ByteArray = try {
        keys.sign(apiBase, data)
    } catch (e: MissingIdentity) {
        throw TransportFailure("revoked", retryable = false)
    } catch (e: kotlinx.coroutines.CancellationException) {
        throw e
    } catch (e: TransportFailure) {
        throw e
    } catch (e: Exception) {
        throw TransportFailure("fault", retryable = true)
    }

    /**
     * A fresh challenge without a credential: `GET /api/challenge` is not a route, and it answers
     * 404 with the header anyway. Read the header and ignore the status (contract §5.7).
     */
    suspend fun freshChallenge(apiBase: String): String? =
        exchange(HttpRequest("GET", "$apiBase/api/challenge", emptyMap(), null)).headers["x-richos-challenge"]

    /** `{"native_push": {...}}` or `{"native_push": null}` on a signed `POST /api/pair`. */
    suspend fun registerPush(apiBase: String, deviceId: String, challenge: String, push: NativePush?): Pair<PushAnswer, String> {
        val registration = if (push == null) "null" else buildString {
            append("{\"platform\":").append(JsonPrimitive(push.platform))
            append(",\"token\":").append(JsonPrimitive(push.token))
            append(",\"topic\":").append(JsonPrimitive(push.topic))
            if (push.previewKey != null) append(",\"preview_key\":").append(JsonPrimitive(push.previewKey))
            append(",\"previews\":").append(push.previews)
            append('}')
        }
        val signed = signed(apiBase, deviceId, challenge, "POST", "/api/pair", "{\"native_push\":$registration}".toByteArray(), "application/json")
        val answer = runCatching { lenient.decodeFromString(PushAnswer.serializer(), signed.response.text) }.getOrElse { throw TransportFailure("fault", retryable = true) }
        return answer to signed.challenge
    }

    private suspend fun exchange(request: HttpRequest): HttpResponse = try {
        http.send(request)
    } catch (e: IOException) {
        throw TransportFailure("unreachable", retryable = true)
    }

    private fun classify(response: HttpResponse, afterResign: Boolean): TransportFailure {
        val json = runCatching { lenient.parseToJsonElement(response.text) as? JsonObject }.getOrNull()
        val missing = (json?.get("missing") as? kotlinx.serialization.json.JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.content }.orEmpty()
        val fresh = response.headers["x-richos-challenge"]
        fun flag(key: String) = (json?.get(key) as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull
        return when {
            response.status == 422 && missing.isNotEmpty() -> TransportFailure("missing", retryable = true, missing = missing, challenge = fresh)
            response.status == 403 && json?.get("revoked")?.jsonPrimitive?.booleanOrNull == true -> TransportFailure("revoked", retryable = false, challenge = fresh)
            response.status == 403 -> TransportFailure("refused", retryable = false, challenge = fresh)
            response.status == 429 -> TransportFailure("rate-limited", retryable = true, challenge = fresh)
            response.status == 404 && afterResign -> TransportFailure("refused", retryable = false, challenge = fresh)
            flag("retry") == false -> TransportFailure("refused", retryable = false, aboutThisMessage = true, challenge = fresh)
            // The Mac waits for the press on the Mac (Sage F1): retryable and never final.
            response.status == 409 && flag("awaiting_mac_confirmation") == true ->
                TransportFailure("fault", retryable = true, awaitingMac = true, challenge = fresh)
            else -> TransportFailure("fault", retryable = true, challenge = fresh)
        }
    }
}
