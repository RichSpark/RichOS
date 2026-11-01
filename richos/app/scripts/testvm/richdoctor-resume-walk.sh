#!/bin/bash
# richdoctor-resume-walk.sh — RichDoctor's "Continue your unfinished report?" card on the real app,
# in one guest run, over fixture drafts (never the operator's own RichDoctor folder).
#
#   run-walk.py --no-app --home <empty dir> --engine "" --report <json> -- \
#     richdoctor-resume-walk.sh <RichDoctor.app> <out-dir>
#
# The fixture folder holds two drafts:
#   * SENT   — stage "review", delivery "submitted", left "draft, saved to finish later", and the
#              NEWER saved time: what an earlier version left behind after a report was sent.
#   * LATER  — a report really saved to finish later, with its own, older saved time.
#
#   R1  the card offers the LATER report, with its own saved time, and never the SENT one.
#   R2  after Continue (which opens the draft) and Quit, both draft.json files are byte-identical:
#       opening RichDoctor rewrites nothing and moves no saved time.
#   R3  after Quit no RichDoctor process is left in the guest.
#   R4  (CEO 2026-11-01: "WHY THE FUCK IS IT NOT BEING SENT AUTOMATICALLY WHEN THE USER GETS ONLINE")
#       a third draft, QUEUED, was approved and sent while offline: the card never offers it, and with
#       the relay down it stays queued. Then the loopback relay stand-in comes UP, with nobody
#       touching RichDoctor: the report is filed by itself, exactly one issue, its draft says
#       "submitted", and no later request re-sends it.
#
#   R5  (rv-20261101T212128Z-4d4b000f-544e #1) a reviewed report is sent and the relay refuses it
#       (no retry time); the person chooses Edit report, then Save for later: the draft is a
#       review-stage draft with its approval withdrawn, and after a relaunch the card offers it.
#   R6  (rv-20261101T221231Z-569ad7fe-8335 #1) a report is sent with the relay down (queued), the
#       person chooses Edit report and changes the screenshot set (Remove screenshot 1; the file
#       picker that adds one cannot be driven by the accessibility tree, and add and remove go
#       through the same engine call, covered by the engine tests): with the relay back up nothing
#       is sent by itself, and Send report registers exactly what is shown (no screenshot).
#
# Delivery points at a loopback address nothing listens on, so nothing can reach GitHub. run-walk.py
# passes the VM name first and stops the guest and deletes the clone when this returns (CEO §54).
# EXIT STATUS: 0 only when every check passed.
set -u
VM="$1"
APP="${2:?usage: richdoctor-resume-walk.sh <vm> <RichDoctor.app> <out-dir>}"
S="${3:?usage: richdoctor-resume-walk.sh <vm> <RichDoctor.app> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
G=/Users/admin/richdoctor-resume-walk
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
pass() { note "ok: $1"; }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
gs() { "$T/guest.sh" "$VM" "$@"; }

PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) setup_failed "the guest payload ($PAYLOAD)" ;; esac
GHOME="$PAYLOAD/home"
AS="sudo launchctl asuser \$(id -u admin) sudo -u admin env HOME='$GHOME'"
DATA="$GHOME/Library/Application Support/com.richos.richdoctor"
QUEUED_ID=d-0e0e0e0e0e0e
QUEUED_WORDS="FIXTURE QUEUED report sent while offline"
QUEUED_RID=RD-3333-3333
PORT=8766
SENT_ID=d-5e17a1a1a1a1
LATER_ID=d-1a7e41a7e411
SENT_MS=1793254656525
LATER_MS=1793000000000
SENT_WORDS="FIXTURE SENT report that was already filed"
LATER_WORDS="FIXTURE LATER report saved to finish later"

finish() {
  # The stand-in this walk started, by the PID it recorded at its start, never by its name.
  gs "test -s $G/stand-in.pid && kill \$(cat $G/stand-in.pid) 2>/dev/null; true" >/dev/null 2>&1
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done: every check passed"
  exit 0
}
has() {
  "$T/ax.sh" "$VM" find --value "$1" --contains --app RichDoctor --first >/dev/null 2>&1 && return 0
  "$T/ax.sh" "$VM" find --title "$1" --contains --app RichDoctor --first >/dev/null 2>&1
}
wait_text() { for _ in $(seq 1 "${2:-30}"); do has "$1" && return 0; sleep 3; done; return 1; }

# ---- the fixture drafts --------------------------------------------------------------------------
python3 -I - "$S" "$SENT_ID" "$LATER_ID" "$SENT_MS" "$LATER_MS" "$SENT_WORDS" "$LATER_WORDS" "$QUEUED_ID" "$QUEUED_WORDS" "$QUEUED_RID" <<'PY' || setup_failed "writing the fixture drafts"
import json, sys
out, sent_id, later_id, sent_ms, later_ms, sent_words, later_words = sys.argv[1:8]
sent_ms, later_ms = int(sent_ms), int(later_ms)
def draft(i, words, ms):
    return {"id": i, "created_at": ms - 60000, "updated_at": ms, "stage": "review", "description": words,
            "conversation": [{"kind": "person", "text": words}], "left": "draft, saved to finish later"}
sent = draft(sent_id, sent_words, sent_ms)
sent["delivery"] = {
    "approved": {"report_id": "RD-2222-2222", "title": sent_words, "body": "fixture", "manifest": [],
                 "approved_at": sent_ms - 1000, "installation_id": "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b"},
    "state": {"state": "submitted", "issue_url": "https://example.invalid/fixture/issues/1", "issue_number": 1, "at": sent_ms - 500},
}
queued_id, queued_words, queued_rid = sys.argv[8:11]
queued = draft(queued_id, queued_words, later_ms + 1000)
queued["stage"] = "approved"
queued["left"] = "approved, not sent yet"
queued["delivery"] = {
    "approved": {"report_id": queued_rid, "title": queued_words, "approved_at": later_ms,
                 "body": "What happened: fixture.\n\nSent with RichDoctor 1.2.3 \u00b7 Report ID " + queued_rid,
                 "manifest": [], "installation_id": "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b",
                 "receipt_secret": "Zm9yLXRoZS13YWxrLW9ubHktZml4dHVyZS1yZWNlaXB0LTE"},
    "state": {"state": "not-sent", "why": {"cause": "offline"}, "at": later_ms + 2000},
}
for d in (sent, queued, draft(later_id, later_words, later_ms)):
    json.dump(d, open(f"{out}/{d['id']}.json", "w"), indent=2)
PY
gs "mkdir -p $G '$DATA/drafts/$SENT_ID' '$DATA/drafts/$LATER_ID' '$GHOME/Applications'" || setup_failed "fixture folders"
gs --push "$S/$SENT_ID.json" "$DATA/drafts/$SENT_ID/draft.json" || setup_failed "the sent fixture draft"
gs "mkdir -p '$DATA/drafts/$QUEUED_ID'" || setup_failed "the queued fixture folder"
gs --push "$S/$QUEUED_ID.json" "$DATA/drafts/$QUEUED_ID/draft.json" || setup_failed "the queued fixture draft"
gs --push "$T/report-relay-stand-in.py" "$G/relay-stand-in.py" || setup_failed "pushing the relay stand-in"
gs --push "$S/$LATER_ID.json" "$DATA/drafts/$LATER_ID/draft.json" || setup_failed "the saved-for-later fixture draft"
gs "chown -R admin '$GHOME/Library'" >/dev/null 2>&1 || true
BEFORE=$(gs "shasum -a 256 '$DATA/drafts/$SENT_ID/draft.json' '$DATA/drafts/$LATER_ID/draft.json'")
LATER_CLOCK=$(gs "LC_ALL=C date -r $((LATER_MS / 1000)) '+%-I:%M %p'" | tr -d '\r' | head -1)

# ---- RichDoctor, installed as the separate download installs it -----------------------------------
ditto -c -k --keepParent "$APP" "$S/RichDoctor.zip" || setup_failed "zipping RichDoctor"
gs --push "$S/RichDoctor.zip" "$G/RichDoctor.zip" || setup_failed "pushing RichDoctor"
gs "rm -rf '$GHOME/Applications/RichDoctor.app' && ditto -x -k $G/RichDoctor.zip '$GHOME/Applications/' && chown -R admin '$GHOME/Applications/RichDoctor.app'" || setup_failed "installing RichDoctor"
gs "$AS RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:$PORT RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local nohup '$GHOME/Applications/RichDoctor.app/Contents/MacOS/RichDoctor' > $G/app.log 2>&1 & echo \$! > $G/app.pid" \
  || setup_failed "launching RichDoctor"
note "RichDoctor launched in the guest, pid $(gs "cat $G/app.pid" | tr -d '\r')"

# ---- R1: the card ---------------------------------------------------------------------------------
if wait_text "Continue your unfinished report?" 20; then pass "R1 the unfinished-report card is offered"; else fail "R1 no card for the saved-for-later report"; fi
"$T/shot.sh" "$VM" "$S/1-card.png" || fail "capture 1-card"
if has "$LATER_WORDS"; then pass "R1 the card offers the report saved to finish later"; else fail "R1 the saved-for-later report is not on the card"; fi
if has "$SENT_WORDS"; then fail "R1 the card offers the report that was already sent"; else pass "R1 the sent report is not offered"; fi
if has "Saved on this Mac at $LATER_CLOCK"; then pass "R1 the card shows the time it was saved ($LATER_CLOCK)"; else fail "R1 the card does not show its own saved time ($LATER_CLOCK)"; fi

if has "$QUEUED_WORDS"; then fail "R4 the card offers the queued report that is sent by itself"; else pass "R4 the queued report is not offered"; fi
if gs "grep -c '\"state\": \"not-sent\"' '$DATA/drafts/$QUEUED_ID/draft.json'" | grep -q 1; then pass "R4 with the relay down it is still queued"; else fail "R4 the queued report left the queue while the relay was down"; fi

# ---- R4: the relay comes up; nobody touches RichDoctor ---------------------------------------------
gs "nohup python3 $G/relay-stand-in.py $PORT $G/relay.json $G/stand-in.log 0 412 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" || fail "R4 starting the relay stand-in"
note "the loopback relay is up; waiting for RichDoctor to send the queued report by itself"
filed=no
for _ in $(seq 1 40); do
  if gs "grep -q 'RD-3333-3333/complete' $G/stand-in.log 2>/dev/null && grep -q submitted '$DATA/drafts/$QUEUED_ID/draft.json'" >/dev/null 2>&1; then filed=yes; break; fi
  sleep 3
done
if [ "$filed" = yes ]; then pass "R4 the queued report was filed by itself once the relay was up"; else fail "R4 not filed within 120 s of the relay coming up"; fi
gs "cat $G/stand-in.log" > "$S/stand-in.log" 2>&1 || true
gs "curl -s http://127.0.0.1:$PORT/_issues" | tr -d '\r' > "$S/issues.json"
N=$(grep -o '"report_id"' "$S/issues.json" | wc -l | tr -d ' ')
if [ "$N" = 1 ]; then pass "R4 exactly one issue exists"; else fail "R4 issues on the relay: $N (want 1)"; fi
REG=$(grep -c '"path": "/v1/reports"' "$S/stand-in.log" || true)
sleep 40
gs "cat $G/stand-in.log" > "$S/stand-in-after.log" 2>&1 || true
REG2=$(grep -c '"path": "/v1/reports"' "$S/stand-in-after.log" || true)
if [ "$REG" = "$REG2" ]; then pass "R4 nothing re-sent it ($REG registration(s) before and after 40 s)"; else fail "R4 re-sent after it was filed: $REG then $REG2"; fi

# ---- R2: open the draft, then quit; nothing is rewritten ------------------------------------------
"$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "pressing Continue"
sleep 4
"$T/shot.sh" "$VM" "$S/2-opened.png" || fail "capture 2-opened"
gs "$AS osascript -e 'tell application id \"com.richos.richdoctor\" to quit'" >/dev/null 2>&1 || true
sleep 5
AFTER=$(gs "shasum -a 256 '$DATA/drafts/$SENT_ID/draft.json' '$DATA/drafts/$LATER_ID/draft.json'")
if [ "$BEFORE" = "$AFTER" ]; then pass "R2 opening a draft and quitting left both draft files byte-identical"; else fail "R2 a draft file changed: $BEFORE / $AFTER"; fi

# ---- R3: Quit leaves nothing running --------------------------------------------------------------
LEFT=$(gs "pgrep -fl 'RichDoctor.app/Contents/MacOS/RichDoctor' || true" | tr -d '\r')
if [ -z "$LEFT" ]; then pass "R3 no RichDoctor process is left"; else fail "R3 left running: $LEFT"; fi
# ---- R5 / R6: a failed send, Edit report, Save for later; a changed screenshot set ----------------
# A reviewed fixture report with one screenshot, offered alone: the earlier drafts are moved away.
R5_ID=d-5555aaaa5555
R5_WORDS="FIXTURE REVIEW report for the failed-send paths"
python3 -I - "$S" "$R5_ID" "$R5_WORDS" "$PORT" <<'PY' || setup_failed "writing the review fixture"
import hashlib, json, struct, sys, zlib
out, rid, words, port = sys.argv[1:5]
def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
w = h = 24
raw = b"".join(b"\x00" + bytes((x * 9 + y * 5 + 40) % 256 for x in range(w) for _ in range(3)) for y in range(h))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
open(f"{out}/r5-shot.png", "wb").write(png)
ms = 1793300000000
d = {"id": rid, "created_at": ms - 60000, "updated_at": ms, "stage": "review", "description": words, "without_ai": True,
     "conversation": [{"kind": "person", "text": words}], "left": "draft, saved to finish later",
     "screenshots": [{"id": "shot-r5r5r5r5r5r5", "sha256": hashlib.sha256(png).hexdigest(), "covers": 0, "approved_at": ms, "media_type": "image/png"}],
     "report": {"report_id": "RD-5555-5555", "title": "FIXTURE failed-send report", "what_happened": words, "expected": "It is filed.",
                "when": "Just now", "steps": [], "versions": "RichDoctor fixture", "excerpts": [], "screenshots": ["shot-r5r5r5r5r5r5"],
                "relay": f"http://127.0.0.1:{port}", "causes": "", "missing": [], "provenance": "without-ai", "removals": [], "shortened": []}}
json.dump(d, open(f"{out}/{rid}.json", "w"), indent=2)
PY
launch_app() {
  gs "$AS RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:$PORT RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local nohup '$GHOME/Applications/RichDoctor.app/Contents/MacOS/RichDoctor' > $G/app.log 2>&1 & echo \$! > $G/app.pid" || fail "relaunching RichDoctor"
}
quit_app() {
  gs "$AS osascript -e 'tell application id \"com.richos.richdoctor\" to quit'" >/dev/null 2>&1 || true
  sleep 5
}
stand_in_stop() { gs "test -s $G/stand-in.pid && kill \$(cat $G/stand-in.pid) 2>/dev/null; rm -f $G/stand-in.pid; true" >/dev/null 2>&1; }
stand_in_start() { # <state> <log>: the loopback relay stand-in, its PID recorded for finish()
  gs "nohup python3 $G/relay-stand-in.py $PORT $G/$1 $G/$2 0 500 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" >/dev/null || fail "starting the relay stand-in ($1)"
  sleep 2
}
# No log file yet means the stand-in has had no request at all: zero registrations.
registrations() { gs "if [ -f $G/$1 ]; then grep -c '\"path\": \"/v1/reports\"' $G/$1; else echo 0; fi; true" | tr -d '\r' | tail -1; }
stand_in_stop
gs "mkdir -p $G/old-drafts && mv '$DATA'/drafts/* $G/old-drafts/ && mkdir -p '$DATA/drafts/$R5_ID/shots'" || setup_failed "clearing the earlier drafts"
gs --push "$S/$R5_ID.json" "$DATA/drafts/$R5_ID/draft.json" || setup_failed "the review fixture draft"
gs --push "$S/r5-shot.png" "$DATA/drafts/$R5_ID/shots/shot-r5r5r5r5r5r5" || setup_failed "the review fixture screenshot"
gs "chown -R admin '$DATA/drafts'" >/dev/null 2>&1 || true
launch_app
wait_text "Continue your unfinished report?" 20 || fail "R5 no card for the review fixture"
# R5: the relay refuses (the stand-in's "reject" fault) with no retry time.
stand_in_start relay5.json stand-in5.log
gs "curl -s -X POST -d '{\"next\":\"reject\"}' http://127.0.0.1:$PORT/_fault" >/dev/null || fail "R5 arming the refusal"
"$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "R5 pressing Continue"
wait_text "Send report" 15 || fail "R5 the review screen did not open"
"$T/ax.sh" "$VM" click --title "Send report" --app RichDoctor --first >/dev/null 2>&1 || fail "R5 pressing Send report"
if wait_text "Not sent yet" 20; then pass "R5 the relay's refusal shows Not sent yet"; else fail "R5 no Not sent yet after the refusal"; fi
"$T/shot.sh" "$VM" "$S/5-refused.png" || fail "capture 5-refused"
"$T/ax.sh" "$VM" click --title "Edit report" --app RichDoctor --first >/dev/null 2>&1 || fail "R5 pressing Edit report"
wait_text "Save for later" 15 || fail "R5 the review screen did not come back"
"$T/ax.sh" "$VM" click --title "Save for later" --app RichDoctor --first >/dev/null 2>&1 || fail "R5 pressing Save for later"
sleep 3
if gs "grep -c '\"cause\": \"withdrawn\"' '$DATA/drafts/$R5_ID/draft.json'" | grep -q 1 && gs "grep -c '\"stage\": \"review\"' '$DATA/drafts/$R5_ID/draft.json'" | grep -q 1; then pass "R5 the draft is review-stage with its approval withdrawn"; else fail "R5 the draft still carries the approval: $(gs "grep -E '\"(stage|state|cause)\"' '$DATA/drafts/$R5_ID/draft.json'" | tr -d '\r' | tr '\n' ' ')"; fi
quit_app
launch_app
if wait_text "Continue your unfinished report?" 20 && has "$R5_WORDS"; then pass "R5 after the relaunch the card offers the report saved for later"; else fail "R5 the failed, edited and saved report is not offered after the relaunch"; fi
"$T/shot.sh" "$VM" "$S/5-offered.png" || fail "capture 5-offered"
# R6: the relay goes away, the report is sent (queued), the screenshot set changes, the relay returns.
stand_in_stop
"$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "R6 pressing Continue"
# A report that was sent and saved for later reopens on its Not sent yet screen (open_saved).
wait_text "Send again" 15 || fail "R6 the reopened report did not show Send again"
"$T/ax.sh" "$VM" click --title "Send again" --app RichDoctor --first >/dev/null 2>&1 || fail "R6 pressing Send again"
sleep 10
if gs "grep -c '\"cause\": \"unreachable\"' '$DATA/drafts/$R5_ID/draft.json'" | grep -q 1; then pass "R6 with the relay down the report is Not sent yet (queued)"; else fail "R6 not queued with the relay down: $(gs "grep -E '\"(stage|state|cause)\"' '$DATA/drafts/$R5_ID/draft.json'" | tr -d '\r' | tr '\n' ' ')"; fi
"$T/ax.sh" "$VM" click --title "Edit report" --app RichDoctor --first >/dev/null 2>&1 || fail "R6 pressing Edit report"
wait_text "Remove screenshot 1" 15 || fail "R6 the screenshot's Remove is not on the review screen"
"$T/ax.sh" "$VM" click --title "Remove screenshot 1" --app RichDoctor --first >/dev/null 2>&1 || fail "R6 removing the screenshot"
sleep 3
stand_in_start relay6.json stand-in6.log
note "R6: the relay is back; waiting 45 s (the outbox looks every 5 s, a retry is due after 30 s)"
sleep 45
N6=$(registrations stand-in6.log)
if [ "$N6" = 0 ]; then pass "R6 the old approval was not sent by itself after the screenshot set changed"; else fail "R6 the old approval was sent by itself ($N6 registration(s))"; fi
"$T/ax.sh" "$VM" click --title "Send report" --app RichDoctor --first >/dev/null 2>&1 || fail "R6 pressing Send report"
sent6=no
for _ in $(seq 1 20); do
  if gs "grep -q '/complete' $G/stand-in6.log 2>/dev/null"; then sent6=yes; break; fi
  sleep 3
done
gs "cat $G/stand-in6.log" > "$S/stand-in6.log" 2>&1 || true
if [ "$sent6" = yes ]; then pass "R6 Send report filed the report"; else fail "R6 Send report did not reach the relay"; fi
MANIFEST=$(python3 -I - "$S/stand-in6.log" <<'PY'
import json, sys
n = None
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r["path"] == "/v1/reports" and r["method"] == "POST":
        n = len(json.loads(r["body"]).get("manifest", []))
print("none" if n is None else n)
PY
)
if [ "$MANIFEST" = 0 ]; then pass "R6 the report sent is the report as shown: no screenshot in its manifest"; else fail "R6 the approved manifest has $MANIFEST screenshot(s); the screen showed none"; fi
"$T/shot.sh" "$VM" "$S/6-sent.png" || fail "capture 6-sent"
quit_app
LEFT=$(gs "pgrep -fl 'RichDoctor.app/Contents/MacOS/RichDoctor' || true" | tr -d '\r')
if [ -z "$LEFT" ]; then pass "R5/R6 no RichDoctor process is left"; else fail "R5/R6 left running: $LEFT"; fi
gs "cat $G/app.log" > "$S/app.log" 2>&1
finish
