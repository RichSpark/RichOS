//! COMPANY MEMORY AT BOOT, AND AGAIN THE MOMENT THE CEO SETS IT UP.
//!
//! Everything in [`wire_company_memory`] lived inside `setup` until 2026-09-01, ran exactly
//! once, and printed the boot line. It moved here **unchanged** because first-run
//! provisioning has to run it a second time: the CEO answers "set my memory up", a corpus
//! appears on disk, and the app must start using it without being relaunched. Two copies of
//! this logic would drift, and the copy that drifted would be the one he actually hits.
//!
//! What this file adds on top of that move is [`MemoryStatus`] — the four outcomes the boot
//! log already distinguished, as a value the window can read, so the first-run question is
//! asked from the same fact the log states rather than from a second guess about it.

use richos_core::correction::CliLoroWriter;
use richos_core::entity::EntityRegistry;
use richos_core::loro::{CliContextCompiler, CorpusLanes, CorpusPaths, LoroInstall, SharedSliceProvenance};
use richos_core::provision;
use richos_core::spine::Spine;
use serde::Serialize;

/// WHAT THIS INSTALL'S MEMORY IS DOING, in the four states it can be in.
///
/// Four and not two, for the reason `LoroError::CompilerNotInstalled` exists: *"there is no
/// record here"* and *"there is a record here and I cannot read it"* are different sentences,
/// and a surface that collapsed them would offer to create a corpus he already has.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MemoryStatus {
    /// `ready` — a corpus resolved and the compiler is wired to the spine.
    /// `none` — nothing resolved. The ordinary state of a fresh install, and the one the
    ///   first-run question answers.
    /// `no-compiler` — a corpus resolved and the program that reads it is not installed.
    /// `unusable` — something else refused; `detail` carries it verbatim.
    pub state: String,
    /// The corpus root, when one resolved.
    pub root: Option<String>,
    /// Which candidate answered — `CorpusSource::as_str`.
    pub source: Option<String>,
    /// The compiler directory in use, when one resolved.
    pub compiler: Option<String>,
    /// Every candidate considered and what became of it. Carried to the window so a state
    /// that needs an operator can say what was looked for rather than only that it failed.
    pub tried: Vec<String>,
    /// The machine-facing detail of a `no-compiler` or `unusable` state. Never shown to the
    /// CEO as it stands — the surface writes his sentence and keeps this for whoever set
    /// RichOS up, the same split `NO COMPUTE LEASE` already uses.
    pub detail: Option<String>,
    /// `~/RichOS/corpus`, as a string to SHOW him. Pre-filling it into the question is what
    /// makes his part a choice instead of a path he types. **It is not a fallback**: nothing
    /// but the surface reads it, and `provision` refuses a target nobody named.
    pub offered_location: Option<String>,
    /// Set only by the provisioning command, so the window can say "that is done" instead of
    /// re-rendering the question it just answered.
    pub provisioned_now: bool,
}

/// Fill in the location the CEO is offered. Separate from the states above because it is a
/// fact about the MACHINE and not about the memory: it is the same string whether or not a
/// corpus exists, and it is absent only when there is no `$HOME` to hang it off.
pub fn with_offered_location(mut status: MemoryStatus, home: Option<&std::path::Path>) -> MemoryStatus {
    if let Some(home) = home {
        status.offered_location = Some(provision::offered_corpus_dir(home).display().to_string());
    }
    status
}

/// BOTH HALVES OF LORO, FROM ONE RESOLUTION.
///
/// The writer is returned rather than resolved separately by the caller, and that is the
/// whole point of the type: `main.rs` used to build it from `CliLoroWriter::from_env()`,
/// twelve hundred lines away from this function and answering a different question, which is
/// how the read path came to resolve the CEO's corpus while the write path resolved nothing
/// at all. There is now one [`LoroInstall`] per wiring, and the reader and the writer are
/// built from it.
pub struct WiredMemory {
    pub status: MemoryStatus,
    /// The WRITE half. `None` exactly when there is no corpus or no compiler — never
    /// because a second resolution disagreed with the first.
    pub writer: Option<CliLoroWriter>,
    /// THE RESOLUTION ITSELF, carried out for the same reason `writer` is.
    ///
    /// The home screen has to compile the corpus into a picture
    /// (`richos_core::home_field`), and it must do so WITHOUT the spine's mutex —
    /// `send_message` holds that lock for the whole of a turn, and a home screen that froze
    /// until Rich finished talking is a home screen nobody goes back to. That leaves two
    /// ways to reach the corpus from a command: resolve it again, or carry the one
    /// resolution this function already made. A second `LoroInstall::locate` is exactly the
    /// second-answer-to-an-answered-question this file's own header calls out, so it is this
    /// one. `None` in every state but `ready`.
    pub install: Option<LoroInstall>,
}

/// TIER C — COMPANY MEMORY (continuity §2.1 #8 / §4, open-items 3.5).
///
/// Prints exactly what it printed when it lived in `setup`: the boot log is an operator's
/// only window into this seam and its wording is load-bearing. What is new is the return
/// value, and one extra line in the `no-compiler` case — which used to be indistinguishable
/// from any other misconfiguration and is now the ordinary state of a provisioned corpus on
/// a machine where the compiler ships from nowhere (`BLOCKED.md`).
pub fn wire_company_memory(
    spine: &mut Spine,
    loro_provenance: &SharedSliceProvenance,
    registry: &EntityRegistry,
    engine: &std::path::Path,
) -> WiredMemory {
    // ONE RESOLUTION. Everything below is built from this value and nothing below looks the
    // corpus up again.
    let mut paths = CorpusPaths::from_process();
    paths.engine_dir = Some(engine.to_path_buf());
    let (install, tried) = match LoroInstall::locate(&paths) {
        Ok(v) => v,
        // A CORPUS WITH NO COMPILER IS ITS OWN LINE. It is not a misconfiguration — it is a
        // provisioned corpus on a machine where the compiler has not been installed, which
        // is the honest state of a fresh install until `BLOCKED.md`'s question is answered.
        Err(richos_core::loro::LoroError::CompilerNotInstalled { root, tried }) => {
            eprintln!(
                "[richos] memory: saved at {root}, but its reading tools are missing. \
                 Conversation memory is unavailable. Looked in: {tried}"
            );
            // NO WRITER EITHER, and for the same reason: `loro-write.mjs` is one of the two
            // entry points `LoroTools::locate` requires, so a corpus with no compiler has no
            // writer to reach. The desk refuses rather than accepting a confirmation it
            // cannot carry out.
            eprintln!(
                "[richos] memory: corrections are unavailable because the writing tools are missing."
            );
            return WiredMemory {
                status: MemoryStatus {
                    state: "no-compiler".into(),
                    root: Some(root),
                    detail: Some(tried.clone()),
                    tried: vec![tried],
                    ..Default::default()
                },
                writer: None,
                install: None,
            };
        }
        Err(e) => {
            eprintln!("[richos] memory: could not read saved information; continuing without it: {e}");
            return WiredMemory {
                status: MemoryStatus { state: "unusable".into(), detail: Some(e.to_string()), ..Default::default() },
                writer: None,
                install: None,
            };
        }
    };

    let Some(install) = install else {
        eprintln!("[richos] memory: no saved memory folder is configured.");
        // WHAT WAS LOOKED FOR, not merely that it failed. This is the line that
        // would have turned "no corpus configured" from a shrug into an
        // instruction the first time anyone read it on a double-click.
        for t in &tried {
            eprintln!("[richos] memory: looked in {t}");
        }
        // AND WHAT WILL HAPPEN ABOUT IT, which is the line this branch was missing on
        // 2026-09-01: until this pass, nothing anywhere created any of the candidates
        // above, so a reader who got this far had a list and no next step.
        eprintln!(
            "[richos] memory: RichOS will offer to set up a memory folder in the window."
        );
        return WiredMemory {
            status: MemoryStatus { state: "none".into(), tried, ..Default::default() },
            writer: None,
            install: None,
        };
    };

    // THE WRITE HALF, from the install that was just resolved — not from a second walk, and
    // not from the environment. This single line is the whole of the fix: whatever corpus the
    // reader below is about to compile from is the corpus a confirmed correction writes to,
    // because it is the same value.
    let writer = Some(CliLoroWriter::from_install(&install));

    // THE READ HALF. A lane map this build cannot use is a READ-SCOPING failure and is
    // reported as one — it does not un-resolve the corpus, so the writer above survives it.
    // A desk that can still show him what loro believes, and still write a correction he
    // confirms, is strictly better than one switched off by a setting about slice narrowing.
    let mut compiler = match CliContextCompiler::from_install(&install, registry) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[richos] memory: could not read saved information; continuing without it: {e}");
            eprintln!(
                "[richos] memory: corrections can still be saved to {}.",
                install.root().path().display()
            );
            return WiredMemory {
                status: MemoryStatus {
                    state: "unusable".into(),
                    root: Some(install.root().path().display().to_string()),
                    detail: Some(e.to_string()),
                    ..Default::default()
                },
                writer,
                // A compiler that refused its lane map cannot compile the home field either.
                install: None,
            };
        }
    };

    eprintln!(
        "[richos] memory: reading from {} (via {}), runtime {}",
        compiler.root().path().display(),
        install.source().as_str(),
        compiler.tools().node()
    );
    // AND THE WRITER IS THE SAME PLACE, said out loud. An operator must never have to infer
    // that the two halves agree — the boot line asserts it, and a build in which it were
    // false would print two different paths here rather than looking identical and being
    // wrong.
    eprintln!(
        "[richos] memory: saving corrections to {} via {}",
        install.root().path().display(),
        install.tools().write_bin().display()
    );
    let status = MemoryStatus {
        state: "ready".into(),
        root: Some(compiler.root().path().display().to_string()),
        source: Some(install.source().as_str().to_string()),
        compiler: Some(compiler.tools().dir().display().to_string()),
        ..Default::default()
    };
    // THE LANE MAP, RECONCILED AGAINST THE CORPUS — open item 3.5, and the
    // reason it is safe to stop being empty.
    //
    // The map defaults to the CEO's six companies now that he has ratified
    // the layout (`wiki/ceo-decisions.md` §5). A mapping is a CLAIM that a
    // partition exists, and loro refuses one it does not have — `exit 2: no
    // such company partition "femcboost" in this corpus. Known: (none).`
    // His corpus has zero partitions today, so a map that were merely
    // filled in would make every re-prime `Unavailable`. Asking the corpus
    // costs 0.06 s (measured, three runs) and turns the map from a claim
    // into a fact.
    //
    // A FAILED PROBE IS NOT "NO PARTITIONS". It leaves the map exactly as
    // configured and says so — inventing an empty corpus from a failure to
    // read one is the same class of lie the whole seam is built against.
    match CorpusLanes::probe(compiler.tools(), compiler.root()) {
        Ok(corpus) => {
            for note in compiler.reconcile_lanes(&corpus) {
                eprintln!("[richos] loro lane: {note}");
            }
            let unmapped = compiler.entities_with_no_lane(registry, &corpus);
            if !unmapped.is_empty() {
                // Loud, because the CEO's side of this is a re-prime that
                // says "loro could not be consulted" every turn: with no
                // lane the compile widens, loro returns another company's
                // items, and the cross-entity re-assertion refuses the
                // slice whole. Fail-closed and correct, and useless to him.
                eprintln!(
                    "[richos] loro lane: this corpus has partitions ({}) but {} \
                     {} bound to none of them — every slice for {} will be \
                     REFUSED by the cross-entity guard. Give the company a partition of \
                     its own name, or set RICHOS_LORO_LANES.",
                    corpus.companies().join(", "),
                    unmapped.join(", "),
                    if unmapped.len() == 1 { "is" } else { "are" },
                    if unmapped.len() == 1 { "it" } else { "them" },
                );
            }
            if compiler.lanes().is_empty() {
                eprintln!(
                    "[richos] memory: this folder has no company partitions; all companies read the same saved information."
                );
            }
            // WHOSE RECORD IS THIS, when the corpus is one repository's own
            // AND HAS NO PARTITIONS.
            //
            // An UNPARTITIONED in-repo corpus has no company field on any
            // item, so the lane map has nothing to narrow and the
            // cross-entity guard has nothing to refuse — both work, and
            // neither can see that a FemcBoost thread is being handed
            // RichOS's record under a heading reading COMPANY MEMORY.
            // Measured on 2026-09-01 against the CEO's only corpus.
            //
            // Once that corpus IS partitioned, `repo_layout_root()` returns
            // None and this block does not run: the caveat asserts "holds no
            // <entity> partition", which would then be false, and a false
            // caveat tells a fresh Rich to discount memory correctly his.
            //
            // The REGISTRY answers whose it is, which it could not do
            // before today: `richos-hq` became a registered root of the
            // `richos` entity in this same pass. An unowned path leaves the
            // owner unstated rather than guessed.
            if let Some(repo) = corpus.repo_layout_root() {
                match registry.resolve_root(repo) {
                    Ok(owner) => {
                        eprintln!(
                            "[richos] loro corpus: in-repo layout at {} — this is {}'s own \
                             record, and every other company's slice will say so.",
                            repo.display(),
                            owner.id
                        );
                        compiler.set_repo_corpus_owner(Some(owner.id.to_string()));
                    }
                    Err(e) => eprintln!(
                        "[richos] loro corpus: in-repo layout at {} but no registered \
                         company owns that path ({e}) — the owner is left unstated \
                         rather than guessed.",
                        repo.display()
                    ),
                }
            }
        }
        Err(e) => eprintln!(
            "[richos] loro lane: could not read which partitions this corpus has ({e}) \
             — the lane map is left exactly as configured rather than assumed empty."
        ),
    }
    compiler.set_provenance_sink(std::sync::Arc::clone(loro_provenance));
    // TIER E — THE EVIDENCE LOOKUP (the 2026-09-17 evidence-retrieval ruling §1/§4).
    //
    // Wired HERE and from the SAME `install`, because the lookup's evidence zone and the
    // compiler's corpus are two halves of one answer: the zone is `<corpus>/ceo/evidence/…`,
    // and resolving the corpus a second time is the second-answer-to-an-answered-question
    // this file's own header calls out. It is attached AFTER the compiler for the same reason
    // it is gated on it — without a coverage label nothing is ever looked at.
    //
    // Three states, and all three are ordinary:
    //   - no `RICHOS_EVIDENCE_BIN` / no `richos-evidence.mjs` beside the service entry point
    //     -> `Ok(None)`, and the payload keeps `EvidenceTier::NotWired`, which the priming
    //     prompt renders as NOTHING rather than as a claim that the CEO has no such files.
    //   - an in-repo dogfood root -> `Ok(None)` too: that layout has no evidence zone, and
    //     pointing the lookup at one would ask it to read the product checkout.
    //   - a pointer that was given and is wrong -> the reason is printed, once, at boot.
    match richos_core::evidence::CliEvidenceLookup::from_env(compiler.root()) {
        Ok(Some(lookup)) => {
            eprintln!(
                "[richos] file lookup: ready via {} in {}; used when saved memory cannot answer a question.",
                lookup.tools().bin().display(),
                lookup.zone().path().display()
            );
            spine.set_evidence_lookup(Box::new(lookup));
        }
        Ok(None) => eprintln!(
            "[richos] file lookup: not configured; questions that saved memory cannot answer will say so."
        ),
        Err(e) => eprintln!(
            "[richos] file lookup: unavailable ({e}); continuing without searching files."
        ),
    }
    spine.set_loro_context_compiler(Box::new(compiler));
    spine.set_loro_provenance(std::sync::Arc::clone(loro_provenance));
    WiredMemory { status, writer, install: Some(install) }
}
