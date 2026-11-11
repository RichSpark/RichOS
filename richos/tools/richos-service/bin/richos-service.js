#!/usr/bin/env node
/**
 * RichOS local service — CLI.
 *
 *   richos-service watch                 # run the drop-zone watcher (pipeline trigger + reconcile net)
 *   richos-service run <sessionId|dir>   # run the pipeline over one session
 *   richos-service live-transcribe <id> --recorder-pid <pid> # predecode the growing native WAV
 *   richos-service live-excerpt <id> --at <epoch-ms> # return flushed speech up to the request
 *   richos-service retranscribe <id> [--model large-v3]   # re-run stages 2-6 on retained audio
 *   richos-service reconcile             # report-only sweep (never transcribes) — the anomaly audit
 *   richos-service claim ...             # coordination (§5.4): own this call or stand down (no double)
 *   richos-service failover-scan         # browser-owned calls that went dark -> promotion candidates
 *   richos-service mark-superseded ...   # record a companion's takeover of a dead browser call
 *   richos-service learn-term ...        # correction flywheel: fold an explicit "the term is X" fix into loro entities
 *   richos-service learn-from-edits <id> # correction flywheel: propose (or --apply) term fixes from a CEO-edited transcript
 *   richos-service correct-text          # correction flywheel: apply the shared vocabulary to DICTATION text
 *   richos-service dictation-review ...  # correction flywheel: what to ASK about a corrected dictation (learns nothing)
 *   richos-service dictation-answer ...  # correction flywheel: the CEO's answer — the only way a dictation pair is learned
 *   richos-service dictation-asks        # the inspectable suppression list
 *   richos-service dictation-retention   # what persisting dictation costs, and what ages out
 *   richos-service models                # the pin table: what is installed, what verifies, what it would cost
 *   richos-service verify-model <id>     # hash a model already on disk against its pinned sha256
 *   richos-service fetch-model <id>      # download it, verify it, and install it only if it verifies
 *   richos-service toolchain             # which binary, which ggml backends, which weights — and did they change
 *   richos-service workspace ...         # the CEO's own Google / Microsoft 365 source: connect / status / sync / disconnect / repair / seed-calendar
 *                                        # --account <address> picks one of several connected Google accounts
 *   richos-service doctor                # verify ffmpeg / whisper-cli / model are resolvable
 *
 * Common flags: --zone <dir> (override the drop zone), --model <id>.
 */

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { runPipeline } from '../lib/pipeline.js';
import { recentLiveExcerpt, watchLiveTranscription } from '../lib/live-transcription.js';
import { assertPrivateTree } from '../lib/private-files.js';
import { scanZone, watch } from '../lib/watcher.js';
import { decideClaimOnDisk, findPromotableOnDisk, markSuperseded } from '../lib/coordination.js';
import { dropZone, ffmpegBin, whisperBin, resolveModel, resolveModelChecked, modelSearchDirs, resolveTier, resolveTierForHost, MODEL_TIERS, DEFAULT_TIER, DEFAULT_MODEL, REPO_ROOT } from '../lib/config.js';
import { MODEL_PINS, PIN_FILE, PINS_VERIFIED_ON, TOOLCHAIN_REFERENCE, pinFor, pinnedModelIds, provenanceLine, isSingleWitness, human } from '../../../engine/voice/provisioning/model-catalog.js';
import { resolveToolchain, lockPath as toolchainLockPath } from '../lib/toolchain.js';
import { modelStatus, downloadModel } from '../../../engine/voice/provisioning/model-fetch.js';
import { inspectFile } from '../../../engine/voice/provisioning/model-integrity.js';
import { assertEvidenceOutsideProductRepo } from '../lib/workspace/privacy.js';
import {
  runWorkspace, doctorLine as workspaceDoctorLine, USAGE as WORKSPACE_USAGE,
  SUPPORTED_VENDORS as WORKSPACE_VENDORS,
} from '../lib/workspace/commands.js';
import { migrateEvidenceZone, formatMigrationReport, currentEvidenceDestination } from '../lib/workspace/migrate-evidence-zone.js';
import { ffmpegVersion } from '../lib/normalize.js';
import { entitiesFilePath, loadEntityMemory } from '../lib/entities.js';
import { learnTerm, learnFromEdits, serializeEntitiesDoc } from '../lib/capture.js';
import { correctText } from '../lib/correct.js';
import { reviewSent, answerAsk, askKey, heardSide, MATCH_WINDOW_MS } from '../lib/dictation.js';
import {
  journalRoot, loadJournal, loadLedger, saveLedger, markReconciled, withConsumed,
  surveyJournal, sweepRetention, costPerHour,
  TEXT_RETENTION_DAYS, TEXT_RETENTION_RECORDS, AUDIO_RETENTION_DAYS, AUDIO_RETENTION_BYTES,
} from '../lib/dictation-store.js';
import { expand } from '../lib/config.js';
import { log } from '../lib/log.js';

function flag(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1) return fallback;
  const v = process.argv[i + 1];
  return v && !v.startsWith('--') ? v : true;
}

/** Collect ALL values of a repeatable flag, e.g. --mangled "a" --mangled "b" -> ['a','b']. */
function flags(name) {
  const out = [];
  for (let i = 0; i < process.argv.length; i += 1) {
    if (process.argv[i] === `--${name}`) {
      const v = process.argv[i + 1];
      if (v && !v.startsWith('--')) out.push(v);
    }
  }
  return out;
}

function resolveSessionDir(arg, zone) {
  if (!arg) return null;
  if (arg.includes('/') || arg.includes(path.sep)) return path.resolve(arg);
  return path.join(zone, arg);
}

function main() {
  const cmd = process.argv[2];
  // `--zone` goes through the SAME refusal as the default: an explicit flag is not permission to
  // write the CEO's recordings into a publicly-shipping product repo.
  const zone = flag('zone')
    ? assertEvidenceOutsideProductRepo(path.resolve(String(flag('zone'))), REPO_ROOT, 'call recordings and transcripts')
    : dropZone();
  const model = flag('model') ? String(flag('model')) : undefined;
  const tier = flag('tier') ? String(flag('tier')) : undefined;

  switch (cmd) {
    case 'watch': {
      const handle = watch({ zone, model, tier });
      process.on('SIGINT', () => {
        handle.stop();
        process.exit(0);
      });
      process.on('SIGTERM', () => {
        handle.stop();
        process.exit(0);
      });
      break;
    }

    case 'run': {
      const dir = resolveSessionDir(process.argv[3], zone);
      if (!dir || !fs.existsSync(dir)) fail(`run: session directory not found: ${dir}`);
      const result = runPipeline(dir, { model, tier, zone, liveCheckpoints: flag('live-checkpoints') === true });
      report(result);
      process.exit(result.status === 'ready' ? 0 : 2);
      break;
    }

    case 'live-transcribe': {
      const dir = resolveSessionDir(process.argv[3], zone);
      const pid = Number(flag('recorder-pid', NaN));
      if (!dir || !fs.existsSync(dir) || !Number.isSafeInteger(pid) || pid <= 1) {
        fail('live-transcribe requires an existing session and a recorder PID');
      }
      const { model: selected, decodeArgs } = resolveTierForHost(tier || model);
      watchLiveTranscription(assertPrivateTree(dir), { model: selected, extraArgs: decodeArgs }, () => {
        try {
          process.kill(pid, 0);
          return true;
        } catch {
          return false;
        }
      }, (timing) => log.info('live transcription checkpoint', timing)).catch(() => {
        fail('live transcription stopped; retained audio can still be decoded');
      });
      break;
    }

    case 'live-excerpt': {
      const dir = resolveSessionDir(process.argv[3], zone);
      const at = Number(flag('at', NaN));
      if (!dir || !fs.existsSync(dir) || !Number.isSafeInteger(at) || at <= 0) {
        fail('live-excerpt requires an existing session and a request time');
      }
      const { model: selected, decodeArgs } = resolveTierForHost(tier || model);
      try {
        const result = recentLiveExcerpt(dir, { model: selected, extraArgs: decodeArgs }, at);
        process.stdout.write(JSON.stringify(result) + '\n');
      } catch {
        fail('the recent live transcript could not be read');
      }
      break;
    }

    case 'retranscribe': {
      const dir = resolveSessionDir(process.argv[3], zone);
      if (!dir || !fs.existsSync(dir)) fail(`retranscribe: session directory not found: ${dir}`);
      // --tier max re-runs the retained audio through the opt-in accuracy tier (guarded large-v3).
      const result = runPipeline(dir, { model, tier, retranscribe: true, zone });
      report(result);
      process.exit(result.status === 'ready' ? 0 : 2);
      break;
    }

    case 'tiers': {
      // List the P5 model tiers. Which one is DEFAULT is read from DEFAULT_TIER rather than
      // written here: this line said `turbo` for two weeks after the tier table was rewritten,
      // and a listing that names the wrong default is worse than one that names none.
      console.log('RichOS model tiers (select with --tier <name>, or a raw --model <id>):\n');
      for (const [name, t] of Object.entries(MODEL_TIERS)) {
        const flagName = name === DEFAULT_TIER ? `${name} (default)` : name;
        console.log(`  ${flagName}\n    model: ${t.model}${t.decodeArgs.length ? `  decode: ${t.decodeArgs.join(' ')}` : ''}`);
        console.log(`    ${t.description}\n`);
      }
      break;
    }

    case 'reconcile': {
      const r = scanZone({ zone, process: false });
      console.log(`reconcile over ${zone}`);
      console.log(`  ok/skipped: ${r.skipped.length}`);
      if (r.anomalies.length) {
        console.log('\nANOMALIES — a call may not have a transcript:');
        for (const a of r.anomalies) console.log(`  ${a.sessionId}\n    - ${a.problems.join('\n    - ')}`);
        process.exit(2);
      }
      console.log('  no anomalies');
      break;
    }

    case 'claim': {
      // Surface-agnostic ownership handshake (§5.4). A companion (macOS now, Windows later) or the
      // extension asks the SHARED authority whether to own a call or stand down (avoid double-capture).
      const req = {
        surface: flag('surface') ? String(flag('surface')) : 'desktop-companion-macos',
        captureKind: flag('kind') ? String(flag('kind')) : 'system',
        processHint: flag('process-hint') ? String(flag('process-hint')) : null,
        sessionId: flag('session-id') ? String(flag('session-id')) : null,
      };
      const decision = decideClaimOnDisk(zone, req);
      console.log(JSON.stringify({ request: req, ...decision }, null, 2));
      process.exit(decision.decision === 'own' ? 0 : 3);
      break;
    }

    case 'failover-scan': {
      // What a companion polls to learn a browser-owned call went dark (crash/hang) and can be taken
      // over. Prints the promotion candidates; a companion then captures with ownership.supersedes.
      const candidates = findPromotableOnDisk(zone);
      console.log(JSON.stringify({ zone, candidates }, null, 2));
      process.exit(0);
      break;
    }

    case 'mark-superseded': {
      // Close the failover loop: record on the dead session that a companion has taken it over.
      const dead = flag('dead') ? String(flag('dead')) : null;
      const by = flag('by') ? String(flag('by')) : null;
      if (!dead || !by) fail('mark-superseded requires --dead <sessionId> --by <sessionId>');
      const ok = markSuperseded(zone, dead, by);
      console.log(JSON.stringify({ dead, by, marked: ok }, null, 2));
      process.exit(ok ? 0 : 1);
      break;
    }

    case 'learn-term': {
      // EXPLICIT correction intake (the correction flywheel's simple, reliable path). Rich runs this
      // on a direct CEO instruction: "the term is Deepgram, it came out as Deep Graham."
      //   learn-term --canonical "Deepgram" --mangled "deep graham" [--mangled ...] [--type product]
      //              [--alias "Deep Gram Inc"] [--fuzzy false] [--case-sensitive] [--min-score 0.8]
      //              [--file <entities.json>]
      const canonical = flag('canonical') ? String(flag('canonical')) : null;
      if (!canonical) fail('learn-term requires --canonical "<term>" (optionally one or more --mangled "<observed>")');
      const file = flag('file') ? path.resolve(String(flag('file'))) : entitiesFilePath();
      const doc = readEntitiesDoc(file);
      const input = {
        canonical,
        mangled: flags('mangled'),
        aliases: flags('alias'),
      };
      if (flag('type')) input.type = String(flag('type'));
      if (String(flag('fuzzy')) === 'false') input.fuzzy = false;
      if (flag('case-sensitive') === true) input.caseSensitive = true;
      if (flag('min-score')) input.minScore = Number(flag('min-score'));

      const res = learnTerm(doc, input);
      if (res.changed) {
        fs.writeFileSync(file, serializeEntitiesDoc(res.doc));
        log.info(
          `learned "${canonical}" (${res.created ? 'new entity' : 'merged'}) — ` +
            `+${res.added.mangled.length} mangling(s), +${res.added.aliases.length} alias(es); ` +
            `version -> ${res.doc.version}`,
        );
      } else {
        log.info(`no change — "${canonical}" already carries everything supplied`);
      }
      if (res.conflicts.length) log.alarm('learn-term conflicts (skipped)', { conflicts: res.conflicts });
      console.log(JSON.stringify({
        file, changed: res.changed, created: res.created, added: res.added,
        conflicts: res.conflicts, version: res.doc.version,
      }, null, 2));
      process.exit(0);
      break;
    }

    case 'learn-from-edits': {
      // TRANSCRIPT-EDIT diff intake (the richer path). A landed transcript.md the CEO edited to fix a
      // name is a correction signal: diff the working-tree file against its committed baseline and
      // PROPOSE conservative mangling->canonical pairs. Propose by default; --apply folds them in.
      //   learn-from-edits <sessionId|path/to/transcript.md> [--apply] [--baseline <path>] [--file <entities.json>]
      const arg = process.argv[3];
      if (!arg) fail('learn-from-edits requires a session id or a path to transcript.md');
      const editedPath = resolveTranscriptPath(arg, zone);
      if (!fs.existsSync(editedPath)) fail(`learn-from-edits: transcript not found: ${editedPath}`);
      const editedText = fs.readFileSync(editedPath, 'utf8');

      const baselineOverride = flag('baseline') ? path.resolve(String(flag('baseline'))) : null;
      const baselineText = baselineOverride
        ? fs.readFileSync(baselineOverride, 'utf8')
        : gitBaseline(editedPath);
      if (baselineText == null) {
        log.info(`no committed baseline for ${editedPath} (untracked or unchanged) — nothing to learn`);
        console.log(JSON.stringify({ transcript: editedPath, proposals: [], applied: false }, null, 2));
        process.exit(0);
      }

      const apply = flag('apply') === true;
      const file = flag('file') ? path.resolve(String(flag('file'))) : entitiesFilePath();
      const doc = readEntitiesDoc(file);
      const res = learnFromEdits(doc, baselineText, editedText, { apply });

      if (res.proposals.length === 0) {
        log.info('no high-confidence term corrections found in the edits');
      } else {
        for (const p of res.proposals) {
          log.info(`  proposal: "${p.from}" -> "${p.to}"  (confidence ${p.confidence})`);
        }
      }
      if (res.applied && res.proposals.length) {
        fs.writeFileSync(file, serializeEntitiesDoc(res.doc));
        log.info(`applied ${res.results.length} proposal(s) to ${file}; version -> ${res.doc.version}`);
      } else if (res.proposals.length) {
        log.info('proposals NOT applied (re-run with --apply to fold them into entities.json)');
      }
      console.log(JSON.stringify({
        transcript: editedPath, file, applied: res.applied,
        proposals: res.proposals, rejected: res.rejected,
        version: res.applied ? res.doc.version : doc.version,
      }, null, 2));
      process.exit(0);
      break;
    }

    case 'correct-text': {
      // THE DICTATION SIDE OF THE ONE SHARED VOCABULARY — the seam that made `entities.json` a
      // single-reader database. open-wispr calls this between whisper-cli and the paste, so a term
      // the CEO taught the system once is spelled right in dictation as well as in call transcripts.
      //
      // IT CALLS `correctText()`, THE EXACT FUNCTION THE CALL PIPELINE CALLS (`lib/correct.js`, via
      // `correct()` at `lib/pipeline.js`). Not a second implementation, not a lookup table built
      // beside it: one corrector, so dictation cannot drift from what a call transcript would say.
      //
      //   richos-service correct-text --text "..."      # or pipe the text on stdin
      //
      // FAILURE POSTURE: this sits in the paste path of a dictation. It must NEVER be the reason a
      // dictation produces nothing, so every failure below echoes the input back unchanged and
      // reports on stderr. Correction is an accuracy enhancement, never a dependency.
      const text = flag('text') && flag('text') !== true ? String(flag('text')) : readStdin();
      try {
        const file = flag('file') ? path.resolve(String(flag('file'))) : entitiesFilePath();
        const mem = loadEntityMemory(file);
        const res = correctText(text, mem.entities);
        if (flag('json') === true) {
          console.log(JSON.stringify({
            text: res.text, corrections: res.corrections,
            entitiesVersion: mem.entitiesVersion, entitiesSource: mem.source,
          }, null, 2));
        } else {
          process.stdout.write(res.text);
        }
      } catch (err) {
        process.stderr.write(`correct-text: ${String(err.message || err)} — passing the text through unchanged\n`);
        process.stdout.write(text);
      }
      process.exit(0);
      break;
    }

    case 'dictation-review': {
      // ASK, NEVER INFER (ceo-decisions.md §7). Given what the CEO actually sent, find the dictation
      // it came from and report what is worth ASKING about. It learns NOTHING — there is deliberately
      // no `--apply` on this command at all. `dictation-answer` is the only way a pair is learned,
      // and it requires a human answer.
      //
      //   richos-service dictation-review --sent "..." [--journal <dir>] [--now <ms>]
      const sent = flag('sent') && flag('sent') !== true ? String(flag('sent')) : readStdin();
      if (!sent.trim()) fail('dictation-review requires --sent "<what you sent>" (or the text on stdin)');
      const root = flag('journal') ? expandPath(String(flag('journal'))) : journalRoot();
      const now = flag('now') ? Number(flag('now')) : Date.now();
      const ledger = loadLedger(root);
      const journal = withConsumed(loadJournal({ root, sinceMs: now - MATCH_WINDOW_MS }), ledger);
      const res = reviewSent(journal, sent, ledger, { now });
      console.log(JSON.stringify({
        journal: root,
        matched: res.matched,
        // What was PASTED, which is what he edited — not the recognizer's raw output. See
        // `heardSide`: reporting `text` here would name a sentence he never saw.
        entry: res.entry ? { id: res.entry.id, at: res.entry.at, heard: heardSide(res.entry) } : null,
        similarity: res.similarity ?? null,
        reason: res.reason,
        // These are QUESTIONS. Answer one with `dictation-answer --from ... --to ... --confirm`.
        prompts: res.prompts,
        suppressed: res.suppressed,
        // Returned, not discarded: silence that cannot be audited cannot be trusted.
        rejected: res.rejected,
      }, null, 2));
      process.exit(0);
      break;
    }

    case 'dictation-answer': {
      // THE HUMAN STATEMENT. The one place in the dictation flywheel where the system's beliefs
      // change, and it cannot be reached except by someone answering the question.
      //
      //   richos-service dictation-answer --from "deep graham" --to "Deepgram" --confirm
      //   richos-service dictation-answer --from "..." --to "..." --decline   # ask again next repeat
      //   richos-service dictation-answer --from "..." --to "..." --never     # permanently suppressed
      const from = flag('from') && flag('from') !== true ? String(flag('from')) : null;
      const to = flag('to') && flag('to') !== true ? String(flag('to')) : null;
      if (!from || !to) fail('dictation-answer requires --from "<what was heard>" --to "<the term>" and one of --confirm | --decline | --never');
      const answer = flag('confirm') === true ? 'confirm' : flag('never') === true ? 'never' : flag('decline') === true ? 'decline' : null;
      if (!answer) fail('dictation-answer requires exactly one of --confirm | --decline | --never');

      const root = flag('journal') ? expandPath(String(flag('journal'))) : journalRoot();
      const ask = { from, to, key: askKey(from, to) };
      let ledger = loadLedger(root);
      const res = answerAsk(ledger, ask, answer);
      ledger = { ...ledger, ...res.ledger };
      if (flag('entry') && flag('entry') !== true) ledger = markReconciled(ledger, String(flag('entry')));
      saveLedger(ledger, root);

      let learned = null;
      if (res.learn) {
        // A confirmed ask is an EXPLICIT correction, so it goes in by the explicit path — the same
        // `learnTerm` `richos-service learn-term` uses. One writer of the vocabulary, one set of
        // conflict rules, one version bump.
        const file = flag('file') ? path.resolve(String(flag('file'))) : entitiesFilePath();
        const doc = readEntitiesDoc(file);
        const r = learnTerm(doc, { canonical: res.learn.canonical, mangled: res.learn.mangled, type: flag('type') ? String(flag('type')) : undefined });
        if (r.changed) fs.writeFileSync(file, serializeEntitiesDoc(r.doc));
        learned = { file, changed: r.changed, created: r.created, added: r.added, conflicts: r.conflicts, version: r.doc.version };
        log.info(`learned "${res.learn.canonical}" from a confirmed dictation correction — version -> ${r.doc.version}`);
      } else {
        log.info(`"${to}": ${res.outcome}`);
      }
      console.log(JSON.stringify({ ask, answer, outcome: res.outcome, learned }, null, 2));
      process.exit(0);
      break;
    }

    case 'dictation-asks': {
      // THE SUPPRESSION LIST MUST BE INSPECTABLE (§7) — or a term silently refuses to learn with no
      // way to see why.
      const root = flag('journal') ? expandPath(String(flag('journal'))) : journalRoot();
      const ledger = loadLedger(root);
      console.log(JSON.stringify({
        journal: root,
        suppressed: ledger.suppressed,
        declined: ledger.declined,
        reconciledEntries: (ledger.reconciled || []).length,
        override: 'richos-service learn-term --canonical "<term>" --mangled "<observed>" overrides any suppression',
      }, null, 2));
      process.exit(0);
      break;
    }

    case 'dictation-retention': {
      // What persisting dictation actually costs, and what the policy would erase. DRY RUN BY
      // DEFAULT: the first thing anyone should be able to do with a policy that deletes the CEO's
      // speech is read it.
      const root = flag('journal') ? expandPath(String(flag('journal'))) : journalRoot();
      const apply = flag('apply') === true;
      const survey = surveyJournal(root);
      const plan = sweepRetention({ root, dryRun: !apply });
      const cost = costPerHour(loadJournal({ root }));
      console.log(JSON.stringify({
        journal: root,
        policy: {
          tierA: `text: ${TEXT_RETENTION_DAYS} days OR ${TEXT_RETENTION_RECORDS} records, whichever binds first`,
          tierB: `audio: OFF by default; when on, ${AUDIO_RETENTION_DAYS} days OR ${AUDIO_RETENTION_BYTES} bytes, whichever binds first`,
        },
        onDisk: {
          dayFiles: survey.days.length,
          records: survey.days.reduce((a, d) => a + d.records, 0),
          textBytes: survey.days.reduce((a, d) => a + d.bytes, 0),
          audioFiles: survey.audio.length,
          audioBytes: survey.audio.reduce((a, x) => a + x.bytes, 0),
        },
        costPerHourOfDictation: cost,
        dryRun: plan.dryRun,
        evict: {
          dayFiles: plan.evictDays.map((d) => ({ day: d.day, records: d.records, bytes: d.bytes, why: d.why })),
          records: plan.evictedRecords,
          audioFiles: plan.evictAudio.length,
          audioBytes: plan.evictedAudioBytes,
        },
      }, null, 2));
      process.exit(0);
      break;
    }

    // -----------------------------------------------------------------------------------------
    // Model integrity. The consent surface — the sheet that ASKS before a byte moves — is app
    // work and is deliberately not here. These are the layer beneath it: what is installed, is it
    // really the model, and fetch it in a way that cannot install anything else.
    // -----------------------------------------------------------------------------------------
    case 'models': {
      const dir = flag('dir') ? expandPath(String(flag('dir'))) : modelSearchDirs()[0];
      const deep = flag('deep') === true;
      console.log(`pin table:  ${PIN_FILE}  (verified ${PINS_VERIFIED_ON})`);
      console.log(`model dir:  ${dir}`);
      console.log(deep ? 'checking:   size, GGML magic AND sha256 over every byte' : 'checking:   size and GGML magic only — pass --deep to hash');
      console.log('');
      (async () => {
        for (const pin of MODEL_PINS) {
          // eslint-disable-next-line no-await-in-loop
          const st = await modelStatus(pin.id, dir, { deep });
          const mark = st.installed ? (st.verified ? 'VERIFIED' : 'present ') : 'absent  ';
          console.log(`${mark}  ${pin.id.padEnd(21)} ${human(pin.bytes).padStart(9)}  ${isSingleWitness(pin) ? '(single witness) ' : ''}${provenanceLine(pin)}`);
          if (!st.installed) console.log(`            ${st.message}`);
          if (st.partialBytes) console.log(`            a partial download of ${human(st.partialBytes)} is on disk; a fetch would resume from there`);
        }
      })().catch((err) => {
        console.error(String(err.message || err));
        process.exit(1);
      });
      break;
    }

    case 'verify-model': {
      const id = process.argv[3];
      if (!id) {
        console.error(`usage: richos-service verify-model <${pinnedModelIds().join('|')}> [--dir d]`);
        process.exit(1);
      }
      const dir = flag('dir') ? expandPath(String(flag('dir'))) : modelSearchDirs()[0];
      const pin = pinFor(id);
      if (!pin) {
        console.error(`"${id}" is not in the pin table, so there is no hash to check it against. Pinned: ${pinnedModelIds().join(', ')}`);
        process.exit(1);
      }
      inspectFile(path.join(dir, pin.file), pin, { deep: true })
        .then((v) => {
          console.log(v.ok ? `OK  ${pin.file} matches its pinned sha256 (${pin.sha256}).` : `FAIL  ${v.message}`);
          console.log(`      pin provenance: ${provenanceLine(pin)}`);
          if (pin.witness) console.log(`      ${pin.witness}`);
          process.exit(v.ok ? 0 : 1);
        })
        .catch((err) => {
          console.error(String(err.message || err));
          process.exit(1);
        });
      break;
    }

    case 'fetch-model': {
      const id = process.argv[3];
      if (!id) {
        console.error(`usage: richos-service fetch-model <${pinnedModelIds().join('|')}> [--dir d]`);
        process.exit(1);
      }
      const dir = flag('dir') ? expandPath(String(flag('dir'))) : modelSearchDirs()[0];
      let lastPct = -1;
      downloadModel(id, dir, {
        onAttempt: ({ attempt, of }) => {
          if (attempt > 1) console.log(`attempt ${attempt} of ${of}…`);
        },
        onProgress: ({ received, total }) => {
          const pct = Math.floor((received / total) * 100);
          if (pct !== lastPct && pct % 5 === 0) {
            lastPct = pct;
            process.stderr.write(`\r  ${String(pct).padStart(3)}%  ${human(received)} of ${human(total)}   `);
          }
        },
      })
        .then((r) => {
          process.stderr.write('\r');
          console.log(r.ok ? `OK  ${r.message}` : `FAIL  ${r.message}`);
          process.exit(r.ok ? 0 : 1);
        })
        .catch((err) => {
          console.error(String(err.message || err));
          process.exit(1);
        });
      break;
    }

    // The toolchain lock: WHAT is about to decode, and is it what decoded last time. Read-only by
    // default — `--relock` is the one way to accept a change on purpose, and it is a separate verb
    // because "accept whatever is there now" happening as a side effect of an inspection is how a
    // guard gets silently disarmed.
    case 'toolchain': {
      const relock = flag('relock') === true;
      const recheck = flag('recheck') === true;
      const resolvedTier = resolveTier(tier || model);
      let binPath;
      try {
        binPath = whisperBin();
      } catch (err) {
        console.error(`whisper-cli: MISSING — ${String(err.message || err)}`);
        process.exit(1);
      }
      let modelPath;
      try {
        modelPath = resolveModel(resolvedTier.model);
      } catch (err) {
        console.error(`model: MISSING — ${String(err.message || err)}`);
        process.exit(1);
      }
      const file = toolchainLockPath();
      if (relock) {
        try {
          fs.rmSync(file, { force: true });
          console.log(`removed the previous lock at ${file} — re-locking from what is installed now`);
        } catch (err) {
          console.error(`could not remove ${file}: ${String(err.message || err)}`);
          process.exit(1);
        }
      }
      const r = resolveToolchain({
        binPath,
        modelPath,
        modelId: resolvedTier.model,
        lockFile: file,
        // `--recheck` re-hashes the model even when the (path, size, mtime, inode, device) tuple
        // says nothing moved. That tuple is what keeps a 3.1 s hash of a 1.6 GB model off every
        // decode; this is the verb that pays it deliberately.
        force: recheck,
        memo: false,
      });
      const t = r.toolchain;
      console.log(`lock file:  ${file}${r.firstRun ? '  (did not exist — written now)' : ''}`);
      console.log(`reference:  whisper.cpp ${TOOLCHAIN_REFERENCE?.whisperCppVersion} / ggml ${TOOLCHAIN_REFERENCE?.ggmlFormulaVersion}, measured ${TOOLCHAIN_REFERENCE?.measuredOn}`);
      console.log('');
      console.log(`whisper-cli:  ${t.bin.realPath || t.bin.path}`);
      console.log(`              version ${t.bin.version || 'NOT REPORTED (this build does not answer --version)'}`);
      console.log(`              sha256 ${t.bin.sha256}  (${t.bin.bytes} bytes)`);
      for (const b of t.backends) console.log(`ggml ${b.name.padEnd(8)} sha256 ${b.sha256}  ${b.path}`);
      if (!t.backends.length) console.log('ggml backends: none reported on the version probe');
      console.log('');
      console.log(`model:        ${t.model.id}  ${t.model.path}`);
      console.log(`              sha256 ${t.model.sha256 || 'UNREADABLE'}  (${t.model.hashed === 'cached' ? 'from the lock cache — pass --recheck to re-hash' : 'hashed just now'})`);
      console.log(`              ${t.model.pinned ? 'pinned in model-pins.json' : 'NOT PINNED — no source hash to check it against'}`);
      console.log('');
      console.log(`verdict:      ${r.verdict.toUpperCase()}${r.toolchain.strict ? '  (strict mode: every warning is a refusal)' : ''}`);
      for (const m of r.messages) console.log(`  - ${m}`);
      // Exit 1 on refuse only. A warning is real and printed, and it is not a broken machine —
      // making `toolchain` exit non-zero on a Homebrew upgrade would put it in the same class as a
      // missing binary in anything that scripts it.
      process.exit(r.verdict === 'refuse' ? 1 : 0);
      break;
    }

    // -----------------------------------------------------------------------------------------
    // THE CEO'S OWN WORKSPACE SOURCE (the architecture §2/§4/§6). The library under
    // lib/workspace/ had no runtime until this case existed: the setup guide's Step 6 tells the CEO
    // to run `richos-service workspace connect google`, so the command has to BE that sentence.
    //
    // Note the zone: `--zone` above is the call-recording DROP zone. The Workspace evidence zone is
    // a different store (`workspaceZone()` / RICHOS_WORKSPACE_ZONE) and is deliberately NOT taken
    // from that flag — one flag silently pointing two stores at one directory is how evidence ends
    // up somewhere nobody meant.
    // -----------------------------------------------------------------------------------------
    case 'workspace': {
      const sub = process.argv[3];
      const vendorArg = process.argv[4] && !process.argv[4].startsWith('--') ? process.argv[4] : null;
      const sources = flags('source');
      const desktopCalendar = flag('desktop-calendar') === true && ['sync', 'connect'].includes(sub);
      if (desktopCalendar) {
        // The app holds this pipe while it owns the sync or consent window.
        process.stdin.resume();
        process.stdin.once('end', () => process.exit(1));
      }
      runWorkspace({
        sub,
        // Every `--name` actually present, so a scheduler flag is refused BY NAME rather than ignored.
        schedulerFlags: process.argv.slice(3).filter((a) => a.startsWith('--')).map((a) => a.replace(/^--/, '').split('=')[0]),
        deps: {
          ...(desktopCalendar ? { out: () => {} } : {}),
          ...(desktopCalendar && sub === 'connect' ? {
            onBrowserUnavailable: (authorizationUrl) => process.stdout.write(JSON.stringify({ authorizationUrl }) + '\n'),
          } : {}),
          ...(vendorArg ? { vendor: vendorArg } : {}),
          ...(flag('client-file') ? { clientFile: String(flag('client-file')) } : {}),
          ...(flag('client-id') ? { clientId: String(flag('client-id')) } : {}),
          // Entra's Directory (tenant) ID — the second of the two ids the Microsoft guide's Step 3
          // hands over. Google has no equivalent and never reads it.
          ...(flag('tenant') ? { tenant: String(flag('tenant')) } : {}),
          // The deliberate confidential-client path. It goes STRAIGHT into the OS keychain and into no
          // RichOS file; nothing downstream prints it, hashes it or puts it in an error.
          ...(flag('client-secret') ? { clientSecret: String(flag('client-secret')) } : {}),
          ...(flag('account') ? { accountId: String(flag('account')) } : {}),
          ...(sources.length ? { sources, only: sources } : {}),
          ...(flag('forget-cursors') === true ? { forgetCursors: true } : {}),
          // Promotion is ON by default (§4.4 step 4): a sync exists so loro can ANSWER from what was
          // pulled. `--no-promote` is the diagnostic pull, and it says so in the output rather than
          // just quietly doing less.
          ...(flag('no-promote') === true ? { promote: false } : {}),
          // `repair`'s window and mode. The window is an ISO instant taken from the run's own
          // output; the mode is dry-run unless --apply, and naming both is refused rather than
          // resolved in RichOS's favor.
          ...(flag('since') ? { since: String(flag('since')) } : {}),
          ...(flag('until') ? { until: String(flag('until')) } : {}),
          ...(flag('apply') === true ? { apply: true } : {}),
          ...(flag('dry-run') === true ? { dryRun: true } : {}),
          // `seed-calendar` (a TEST tool). `--teardown` removes everything it made and gives its
          // write grant back; `--set` names one fixture set so two of them can coexist.
          ...(flag('teardown') === true ? { teardown: true } : {}),
          ...(flag('set') ? { setId: String(flag('set')) } : {}),
        },
      })
        .then((r) => {
          if (desktopCalendar && sub === 'sync') {
            // A null result means the pass could not check account health. An empty list clears it.
            const status = {
              calendarRecovery: r.calendarRecovery ?? null,
              calendarInstances: r.calendarInstances ?? null,
            };
            process.stdout.write(JSON.stringify(status) + '\n', () => process.exit(r.exitCode));
          } else {
            process.exit(r.exitCode);
          }
        })
        .catch((err) => {
          if (!desktopCalendar) console.error(String(err.message || err));
          process.exit(1);
        });
      break;
    }

    // -----------------------------------------------------------------------------------------
    // ONE-TIME: move a corpus written before 2026-09-17 out of the compiled record tree.
    // Reports by default and changes nothing; `--apply` performs the move. See
    // lib/workspace/migrate-evidence-zone.js for what it rewrites and what it refuses to touch.
    // -----------------------------------------------------------------------------------------
    case 'migrate-evidence-zone': {
      const dest = currentEvidenceDestination();
      console.log(`writes now: ${dest.root}${dest.company ? `  (company ${dest.company} — nothing to migrate)` : ''}`);
      const result = migrateEvidenceZone({
        corpus: flag('corpus') ? expandPath(String(flag('corpus'))) : undefined,
        dryRun: flag('apply') !== true,
      });
      console.log(formatMigrationReport(result));
      process.exit(result.refusal ? 1 : 0);
      break;
    }

    case 'doctor': {
      let ok = true;
      try {
        console.log(`ffmpeg:     ${ffmpegBin()}  (${ffmpegVersion()})`);
      } catch (err) {
        ok = false;
        console.log(`ffmpeg:     MISSING — ${String(err.message || err)}`);
      }
      try {
        console.log(`whisper:    ${whisperBin()}`);
      } catch (err) {
        ok = false;
        console.log(`whisper:    MISSING — ${String(err.message || err)}`);
      }
      // THE HOST-AWARE ANSWER, because `doctor` is where somebody looks to find out what this
      // machine will actually do. `resolveTier` would report the ceiling and be silent about a
      // demotion that is going to happen on every run — which is precisely the gap this command
      // exists to close for the other resolutions it prints.
      const resolvedTier = resolveTierForHost(tier || model);
      try {
        console.log(`tier:       ${resolvedTier.name} -> model ${resolvedTier.model}${
          resolvedTier.decodeArgs.length ? ` (decode: ${resolvedTier.decodeArgs.join(' ')})` : ''
        }`);
        if (resolvedTier.hardware) {
          const h = resolvedTier.hardware;
          const mem = `${h.machine.availableBytes} B available of ${h.machine.totalBytes} B`;
          if (h.basis === 'top-rung') {
            console.log(`            resolved from this machine (${mem})${
              h.projected != null ? ` — projected ${(1 / h.projected).toFixed(1)}x faster than real time` : ''
            }`);
          } else {
            console.log(`            RESOLVED DOWN from ${h.rejected ? h.rejected.id : 'the default'} — ${h.basis} (${mem})`);
            console.log(`            ${h.notice}`);
            console.log('            docs/measurements/hardware-model-resolution-2026-09-10/');
          }
        }
        const found = resolveModelChecked(resolvedTier.model);
        console.log(`model:      ${found.path}`);
        console.log(`            ${found.pin ? `pinned sha256 ${found.pin.sha256.slice(0, 16)}… — run \`richos-service verify-model ${resolvedTier.model}\` to hash it` : 'NOT PINNED — this model has no hash to check against'}`);
        for (const r of found.rejected) console.log(`            rejected: ${r.message}`);
      } catch (err) {
        // A missing tier model is only fatal if that tier was explicitly requested; the model
        // DEFAULT_TIER names must resolve. This is what makes a fresh machine that carries only
        // ggml-large-v3-turbo-q5_0.bin pass: the 1,624,555,275-byte full-turbo file is an opt-in
        // tier now, so its absence reports "not installed (opt-in tier)" and exits 0.
        const fatal = !tier && !model ? true : resolvedTier.name === DEFAULT_TIER;
        if (fatal) ok = false;
        console.log(`model:      ${fatal ? 'MISSING' : 'not installed (opt-in tier)'} — ${String(err.message || err)}`);
      }
      console.log(`drop zone:  ${zone}`);
      // Read-only, and deliberately never fatal: the Workspace source is optional, so an unconnected
      // Google account must not turn a healthy transcription toolchain into a red `doctor`.
      // ONE LINE PER VENDOR. A single line would report whichever vendor happened to be hard-coded
      // and be silent about the other — which is the failure `doctorLine` itself was rewritten to
      // avoid for accounts, applied one level up. The CEO may have connected either, or both.
      for (const wsVendor of WORKSPACE_VENDORS) {
        console.log(`workspace:  ${wsVendor} — ${workspaceDoctorLine({ vendor: wsVendor })}`);
      }
      process.exit(ok ? 0 : 1);
      break;
    }

    default:
      console.log(
        [
          'usage:',
          `  richos-service watch [--zone dir] [--tier ${Object.keys(MODEL_TIERS).map((n) => (n === DEFAULT_TIER ? `${n}*` : n)).join('|')}]   (* = default)`,
          '  richos-service run <sessionId|dir> [--zone dir] [--tier name | --model id] [--live-checkpoints]',
          '  richos-service live-transcribe <sessionId|dir> --recorder-pid <pid>',
          '  richos-service live-excerpt <sessionId|dir> --at <epoch-ms>',
          '  richos-service retranscribe <sessionId|dir> [--tier max | --model id]',
          '  richos-service tiers',
          '  richos-service reconcile [--zone dir]',
          '  richos-service claim --surface <s> --kind <browser-tab|system|process> [--process-hint h] [--session-id id]',
          '  richos-service failover-scan [--zone dir]',
          '  richos-service mark-superseded --dead <sessionId> --by <sessionId>',
          '  richos-service learn-term --canonical "<term>" [--mangled "<observed>" ...] [--type t] [--alias a ...] [--fuzzy false] [--case-sensitive] [--min-score n] [--file f]',
          '  richos-service learn-from-edits <sessionId|path/to/transcript.md> [--apply] [--baseline path] [--file f]',
          '  richos-service correct-text [--text "..."] [--json] [--file f]      # dictation reads the SAME vocabulary as calls',
          '  richos-service dictation-review --sent "<what you sent>" [--journal dir]   # ASKS; learns nothing',
          '  richos-service dictation-answer --from "<heard>" --to "<term>" (--confirm|--decline|--never) [--entry id]',
          '  richos-service dictation-asks [--journal dir]                      # the inspectable suppression list',
          '  richos-service dictation-retention [--apply] [--journal dir]       # what it costs, and what ages out',
          '  richos-service models [--dir d] [--deep]           # the pin table + what is installed',
          '  richos-service verify-model <id> [--dir d]         # hash it against its pinned sha256',
          '  richos-service fetch-model <id> [--dir d]          # download, verify, install only if it verifies',
          '  richos-service toolchain [--tier t] [--recheck] [--relock]   # which binary, which backends, which weights',
          WORKSPACE_USAGE,
          '  richos-service migrate-evidence-zone [--corpus dir] [--apply]   # ceo/unfiled/evidence -> ceo/evidence/unfiled (reports by default)',
          '  richos-service doctor',
        ].join('\n'),
      );
      process.exit(cmd ? 1 : 0);
  }
}

/** `~`-expanding path resolve, so `--journal ~/...` behaves the way every other path flag does. */
function expandPath(p) {
  return path.resolve(expand(p));
}

/**
 * Read all of stdin, synchronously. `correct-text` sits in the paste path of a dictation, so a
 * pipe is the natural interface and blocking is correct: there is nothing else to do until the
 * sentence has arrived. A closed/absent stdin yields '' rather than throwing.
 */
function readStdin() {
  try {
    return fs.readFileSync(0, 'utf8');
  } catch {
    return '';
  }
}

/** Read the raw entities.json doc (preserving its structure for round-trip write). Missing => a fresh doc. */
function readEntitiesDoc(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return { schemaVersion: 1, version: '', entities: [] };
  }
}

/** Resolve a transcript.md from a session id, a session dir, or a direct file path. */
function resolveTranscriptPath(arg, zone) {
  const p = arg.includes('/') || arg.includes(path.sep) ? path.resolve(arg) : path.join(zone, arg);
  if (p.endsWith('.md')) return p;
  return path.join(p, 'transcript.md');
}

/**
 * The committed (HEAD) version of a git-tracked transcript IS the emitted baseline — the CEO's
 * uncommitted working-tree edits are exactly the correction signal we want. Using git avoids any
 * extra retained-copy machinery and captures precisely those edits. Returns null when the file is
 * untracked or unchanged (nothing to learn).
 */
function gitBaseline(absPath) {
  const rel = path.relative(REPO_ROOT, absPath);
  try {
    // `-C REPO_ROOT` so this works from any cwd; the working tree may live in a git worktree.
    const committed = execFileSync('git', ['-C', REPO_ROOT, 'show', `HEAD:${rel}`], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const current = fs.readFileSync(absPath, 'utf8');
    return committed === current ? null : committed;
  } catch {
    return null; // not tracked at HEAD
  }
}

function report(result) {
  if (result.status === 'ready') {
    log.info(`READY ${result.sessionId} — ${result.words} words -> ${result.transcript}`);
  } else {
    log.alarm(`${result.status.toUpperCase()} ${result.sessionId}`, { problems: result.problems });
  }
}

function fail(msg) {
  log.error(msg);
  process.exit(1);
}

main();
