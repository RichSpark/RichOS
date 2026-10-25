#!/usr/bin/env bash
#
# testdevices.mutation.sh — PROVES the test-device suite CAN FAIL, one property
# at a time. Invoked by testdevices.test.sh; the loop is mutation-harness.sh.
#
# The two directions of the one asymmetry matter most: a device whose owner is
# gone must go (owner-alive-ignored proves the opposite direction is held too),
# and a device whose owner cannot be proven must NEVER be deleted
# (undecided-deleted), because a collector that deletes on a guess deletes a
# live test's simulator once and is switched off for ever.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=mutation-harness.sh
. "$SCRIPT_DIR/mutation-harness.sh"
mutation_begin "testdevices (test simulators and emulators)" "scripts/lib/testdevices.test.sh"

D="scripts/lib/testdevices.py"

mutant owner-alive-ignored "test_T04" "$D" \
    '        return LEAVE, "a registered owner still runs"' \
    '        return COLLECT, "a registered owner still runs"' \
    "a running test's own simulator would be deleted under it."

mutant creator-pid-reuse-ignored "test_T03" "$D" \
    '        if started > born + 1:' \
    '        if False:' \
    "a simulator whose creator's pid was reused by another process would be kept for ever."

mutant undecided-deleted "test_T05" "$D" \
    '        elif verdict == UNDECIDED:{NL}            res["undecided"].append(row)' \
    '        elif False:{NL}            res["undecided"].append(row)' \
    "a device whose owner nothing proves would be deleted on a guess."

mutant emulator-args-not-verified "test_T10b" "$D" \
    '        if not _names_avd(rec["pid"], rec["avd"]):{NL}            continue' \
    '        if False:{NL}            continue' \
    "a recorded pid that another process now holds would be treated as the emulator."

mutant sandbox-reads-the-machine "test_T11" "$D" \
    '    try:{NL}        import pwd{NL}        real = os.path.realpath(pwd.getpwuid(os.getuid()).pw_dir)' \
    '    return True{NL}    try:{NL}        import pwd{NL}        real = os.path.realpath(pwd.getpwuid(os.getuid()).pw_dir)' \
    "a test suite with a sandboxed HOME would read, and could delete, the machine's own simulators."

mutant shutdown-unproven-alerted "test_T06b" "$D" \
    '            if verdict == "owner cannot be proven" and d.get("state") == "Shutdown":' \
    '            if False:' \
    "every shut-down device from an unrecorded checkout would sit in the MASSIVE ALERT for ever."

mutant failure-row-never-resolves "test_T08" "$D" \
    '            continue                    # nothing was read, so nothing is resolved{NL}        del rows[key]' \
    '            continue                    # nothing was read, so nothing is resolved{NL}        pass' \
    "the alert would outlive the device Rich deleted by hand, and an alert that never clears is not read."

mutant stale-generation-authorizes-deletion "test_T20" "$D" \
    '            if r.get("generation") != expected:' \
    '            if False:' \
    "a replacement Android process would inherit a dead owner's deletion permission."

mutant deadline-not-propagated "test_T17" "$D" \
    '    remaining = maximum if deadline is None else min(maximum, deadline - time.time())' \
    '    remaining = maximum' \
    "simulator inventory would exceed the whole hook deadline."

mutant shared-owner-discarded "test_T19" "$D" \
    '                if old != owner and owner_state(old)[0] != "gone":' \
    '                if False:' \
    "a second user's completion would delete a simulator still owned by the first."

mutant interrupted-collector-silent "test_T32" "$D" \
    '            record_collector_failure("cleanup started but has not completed")' \
    '            pass' \
    "an interrupted cleanup pass would leave no alert."

mutant killed-collector-silent "test_T32b" "$D" \
    '                writer.record_running(_collect_command())' \
    '                pass' \
    "a killed cleanup pass would leave no alert."

mutant running-collection-alerted "test_T32b" "scripts/lib/testdevice_alerts.py" \
    '            if row.get("pid_start") and current == row["pid_start"]:' \
    '            if False:' \
    "a collection that is simply still running would be published as a cleanup failure (2026-10-24)."

mutant abandoned-collection-hidden "test_T32c" "scripts/lib/testdevice_alerts.py" \
    '            if row.get("pid_start") and current == row["pid_start"]:' \
    '            if True:' \
    "a killed collector's mark would never alert again."

mutant overdue-collection-hidden "test_T32c" "scripts/lib/testdevice_alerts.py" \
    '        if not isinstance(due, (int, float)) or now >= due + GRACE_SECONDS:' \
    '        if not isinstance(due, (int, float)):' \
    "a collection hung for hours would read as running for ever."

mutant progress-not-recorded "test_T32f" "scripts/lib/testdevices.py" \
    '            writer.note_operation(remaining)' \
    '            pass' \
    "a long collection whose every operation succeeds would read as timed out (rv-20261025T010507Z-164da3c3-7552)."

mutant new-run-masks-standing-failure "test_T32d" "scripts/lib/testdevice_alerts.py" \
    '    judged = standing({KEY: previous}).get(KEY) if previous is not None else None' \
    '    judged = None' \
    "a standing collector failure would vanish while the next pass runs."

mutant new-run-masks-abandoned-mark "test_T32e" "scripts/lib/testdevice_alerts.py" \
    '    judged = standing({KEY: previous}).get(KEY) if previous is not None else None' \
    '    judged = standing({KEY: previous}).get(KEY) if isinstance(previous, dict) and previous.get("verdict") != RUNNING else None' \
    "a killed or hung collector's mark would read as recovered the moment the next pass started (2026-10-25)."

mutant owned-run-not-renewed "test_T44" "$D" \
    '        lease["last_use"] = time.time(){NL}        lease["activity"] = {"pid": child.pid, "renewer": os.getpid(), "renewed": lease["last_use"]}' \
    '        lease["activity"] = {"pid": child.pid, "renewer": os.getpid(), "renewed": time.time()}' \
    "a live, owned xcodebuild run would lose its simulator five minutes in (esc-20260924T220236Z-52fae3ec)."

mutant dead-owner-still-renewed "test_T45" "$D" \
    '    if owner_state(owner)[0] != "alive":{NL}        return False, "the run'"'"'s owner is not proven alive"' \
    '    if False:{NL}        return False, "the run'"'"'s owner is not proven alive"' \
    "an orphaned run would keep renewing a lease whose owner is gone."

mutant expired-lease-renewed "test_T46b" "$D" \
    '        if lease_expired(rec):{NL}            return False, "the lease reached its lifetime or inactivity limit"' \
    '        if False:{NL}            return False, "the lease reached its lifetime or inactivity limit"' \
    "renewal would keep writing to a lease past its lifetime."

mutant ended-run-still-renewed "test_T51" "$D" \
    '    if child.poll() is not None:{NL}        return False, "the owned run ended"' \
    '    if False:{NL}        return False, "the owned run ended"' \
    "a finished run would keep its device from counting as idle."

mutant reregister-keeps-long-lifetime "test_T50" "$D" \
    '"max_seconds": LEASE_MAX_SECONDS,' \
    '"max_seconds": old_lease.get("max_seconds", LEASE_MAX_SECONDS),' \
    "registering again would carry a declared lifetime to a caller that never declared it."

mutant busy-registry-crashes-collector "test_T53" "$D" \
    '        except RegistryBusy as exc:' \
    '        except ZeroDivisionError as exc:' \
    "a registry lock held for five seconds would kill the collector and close native admission (2026-09-24)."

mutant wedged-registry-tolerated "test_T54" "$D" \
    '            if now - since >= COLLECTOR_BUSY_ALERT_SECONDS:' \
    '            if False:' \
    "a registry lock that never frees would be skipped silently for ever, with no lease ever expired."

mutant orphan-booted-prepared-not-shut-down "test_T55" "$D" \
    '                if found and found.get("state") not in ("", "Shutdown"):' \
    '                if False:' \
    "a prepared simulator an old run left booted would fail the next lease's boot (2026-09-24, iPhone SE)."

mutant lost-lease-run-keeps-going "test_T57" "$D" \
    '            if not ours and child.poll() is None:' \
    '            if False:' \
    "a run whose lease was handed to another run would go on driving that run's device (esc-20260925T014934Z-0a4bf206)."

mutant run-not-in-its-own-group "test_T58" "$D" \
    '        child = subprocess.Popen(command, start_new_session=True)' \
    '        child = subprocess.Popen(command)' \
    "ending a run would end only its first process, leaving xcodebuild's children on the device."

mutant cli-gives-up-at-the-collectors-five-seconds "test_T59" "$D" \
    '        REGISTRY_LOCK_SECONDS = CLI_REGISTRY_LOCK_SECONDS' \
    '        pass' \
    "a UI suite's acquire or release would fail whenever simctl held the registry for five seconds (2026-09-25)."

mutant device-admission-back-to-sixty-seconds "test_T60" "$D" \
    '        held.append(worker_tokens.Budget(worker_tokens.machine_directory(), runner=True).acquire(timeout=left()))' \
    '        held.append(worker_tokens.Budget(worker_tokens.machine_directory(), runner=True).acquire(timeout=60))' \
    "a busy host would fail a test's device launch after 60 s instead of waiting its bounded turn (2026-09-25)."

mutant boot-wait-lets-the-lease-idle-out "test_T61" "$D" \
    '                touch_lease(kind, ident)' \
    '                pass' \
    "a boot waiting its bounded turn for admission would lose its device to the inactivity limit."

mutant pool-count-ignored "test_T63" "$D" \
    '            if len(busy) < limit and not any(r["prepared"] == key for r in busy):' \
    '            if not busy:' \
    "a nightly that chose two simulated phones would still run its iPhone suites one after another (2026-09-25)."

mutant raised-pool-shares-one-device "test_T63" "$D" \
    '            if len(busy) < limit and not any(r["prepared"] == key for r in busy):' \
    '            if len(busy) < limit:' \
    "with room in the pool, a second run would be handed a device type another run is driving."

mutant pool-default-not-one "test_T62" "$D" \
    'POOL_LEASES = 1{NL}' \
    'POOL_LEASES = 2{NL}' \
    "every land and every engineer's suite would take a second simulator nobody chose."

mutation_end
