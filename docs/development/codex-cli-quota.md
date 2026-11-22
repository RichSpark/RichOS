# Codex CLI quota contract

RichOS uses the installed Codex CLI and its existing sign-in. The [App Server
protocol](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt)
documents JSON lines over stdio, the `initialize` handshake and
`account/rateLimits/read`. The installed CLI can emit its own schema with
`codex app-server generate-json-schema --out DIR`.

| What | What Codex reports |
| --- | --- |
| Included allowance | `rateLimitsByLimitId` and the older single `rateLimits` view. Each bucket can have primary and secondary windows. The window includes `usedPercent`, `windowDurationMins` and `resetsAt` in Unix seconds. The number of windows depends on the account. |
| Limit and access state | Optional `rateLimitReachedType` and, on the CLI checked here, `ordinaryUsageAllowed` and `spendControlReached`. The installed schema describes `ordinaryUsageAllowed` as backend permission for ordinary included usage. A missing field leaves that permission unknown. |
| Banked resets | `rateLimitResetCredits.availableCount` is authoritative when present. Detail rows can be absent or capped. Their optional metadata includes expiry. |
| Usage credits | The optional bucket `credits` reports `hasCredits`, `unlimited` and a nullable decimal string `balance`. A missing balance is unknown. This interface gives no estimate of how much work a balance will cover. |

[Earned reset redemption](https://learn.chatgpt.com/docs/app-server#8-earned-rate-limit-resets-chatgpt)
is `account/rateLimitResetCredit/consume`. Use one UUID per logical redemption
and reuse it after an uncertain response. Codex chooses the next available
reset if `creditId` is omitted. The documented outcomes are `reset`,
`alreadyRedeemed`, `nothingToReset` and `noCredit`. Read usage again after
redemption. [Banked resets](https://help.openai.com/en/articles/20001498-how-resets-work-in-codex)
cover the main included allowance. Scheduled reset times describe usage
windows. Codex also provides interactive redemption under CLI `/usage` and
desktop Settings → Usage & billing.

OpenAI applies included allowance first. Eligible work then draws from the
existing credit balance automatically. Redemptions, purchases and credit
reload are separate operations. The standard CLI run already follows the
account's applicable billing and spending controls.
[OpenAI credit documentation](https://help.openai.com/en/articles/12642688-using-credits-for-flexible-usage-in-chatgpt-personal-plans)

On macOS, the bundled CLI version `0.171.0-alpha.13` returned an account ID,
ordinary-use permission, a weekly window, reset inventory and paid balance
from a single read on November 22, 2026. The response had a null secondary
window. The read started no Codex turn and redeemed no reset.

## RichOS dispatch

Run `codex-channel.sh quota` or ask Rich in the app to check Codex quota.
Both use the active CLI sign-in and return a fresh reading. Missing fields are
shown as unknown. The response omits account identifiers and reset IDs.

Build dispatch and resume check the main included allowance before starting a
turn. When a current main window is at its limit and Codex reports a banked
reset, RichOS verifies the active ChatGPT account, rechecks inside a shared
lock and redeems one. A persistent attempt keys retries across concurrent
terminal and app dispatches. Provider overrides, a missing account identity
and non-main limits can leave automatic redemption unavailable.

When included usage is exhausted, RichOS still sends the ordinary CLI turn so
Codex can draw from eligible credits. If a known usage-limit failure occurs
during a turn, RichOS rechecks the account and can retry once after a
confirmed banked reset in the same Codex chat. A failed dispatch preserves the
request and reports the outcome and resume command on its original connection.
Codex second reviews continue to use their existing Claude fallback.

RichDoctor's Configuration collection includes the last completed dispatch
summary in the Codex connection watcher item. Its fixed fields record the
quota read and reset outcomes, preflight and elapsed milliseconds and the
process exit. Usage amounts and account identifiers are omitted from that
diagnostic summary.
