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
gs "cat $G/app.log" > "$S/app.log" 2>&1
finish
