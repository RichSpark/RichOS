# Changelog worker: <TAG>

**<TAG> is published. Write its changelog entry and after a stable also main's next release version, as commits on your own branch, then hand over. Nothing else.**

**Never touch the CEO's phones, his installs, his running app, the test VM or the host Mac's display, sleep, lock or input. Build, publish or release nothing, not even a dry run. Rich is the only writer to main: commit on your own branch only; never push, merge into main or deploy.**

Why a worker writes this (the CEO, 2026-10-20): "why do I think that automatically writing a changelog is a retarded idea? (as opposed to automatically triggering a worker for the job of writing)". The release command raised the escalation that started you; it writes no changelog itself.

Lead: hand this file to `spawn.sh` unchanged except its tag placeholder (the word TAG between angle brackets), replaced everywhere by the published tag the escalation names.

## 1. What went into <TAG>

Work from your own workspace, made from main. Every command below is read-only.

1. The kind. `<TAG>` of the form `vX.Y.Z-nightly.YYYYMMDD.N` is a nightly; `vX.Y.Z` is a stable. Its version is the tag without the leading `v`.
2. The published releases: `gh release list --repo WebDevBooster/richos --limit 1000 --json tagName,isDraft,isPrerelease`. Leave out drafts and any tag that is neither form above (such as `nightly`).
3. The previous release of the same kind, published before `<TAG>`:
   - for a nightly, the nightly with the highest (X, Y, Z, YYYYMMDD, N) below `<TAG>`'s;
   - for a stable, the stable with the highest (X, Y, Z) below `<TAG>`'s that is not a prerelease (a stable still marked prerelease was not published as one).
   If there is none, stop and report that.
4. Each one's source commit: `gh release view <tag> --repo WebDevBooster/richos --json body` and read its line `Source: <40-character commit>`. Do this for `<TAG>` and for the previous release.
5. `git fetch origin`, then every merge from the previous source to `<TAG>`'s, oldest first: `git log --reverse --merges --format='%x1e%s%n%n%b' <previous source>..<TAG source>`. Leave out the merges that only took main or a release branch into a work branch (a subject such as `Merge branch 'v1.2.1' into cc/echo-opus-acctswitch4`); they say nothing of their own. What is left is exactly what went into `<TAG>`. If nothing is left, stop and report that.

## 2. The entry

1. The file: a nightly goes in `docs/CHANGELOG-NIGHTLY.md`, a stable in `docs/CHANGELOG.md`. Never the other one.
2. If that file already has the heading `## <version>`, its entry exists: write nothing in it and say so.
3. Read `richos/app/scripts/changelog-instructions.md` in full and write the entry exactly as it says, from the merges of step 1 and nothing else.
4. Put it above the first entry for an older version: the top, for the version just published; its own place, for one written late. Leave everything else in the file as it is.
5. Commit only that file, with the message `Changelog: <version> in <file>` and a body naming the range: the previous release, its source commit and `<TAG>`'s source commit.

## 3. After a stable only: main's next release version

A nightly changes no version: skip this section.

After stable `vX.Y.Z`, the next nightly must not start with `X.Y.Z-nightly`, so main moves to the next patch version, `X.Y.(Z+1)`:

1. If the `[package]` version in `richos/app/src-tauri/Cargo.toml` is no longer `X.Y.Z` (main already moved past it), change nothing and say so.
2. Otherwise change exactly two lines: that `version = "X.Y.Z"` line in `richos/app/src-tauri/Cargo.toml` and the `version` line of the `[[package]]` entry named `richos-tauri` in `richos/app/src-tauri/Cargo.lock`. A dependency that happens to share the number stays as it is.
3. Check it: `cargo metadata --manifest-path richos/app/src-tauri/Cargo.toml --locked --offline --format-version 1 > /dev/null` exits 0 (the lock agrees with the manifest). If it cannot run offline here, say so with its error.
4. Commit those two files with the message `Next release version X.Y.(Z+1): vX.Y.Z is published`.

## 4. Proof, then hand over

- `git diff main...HEAD --stat` lists only the changelog file and, after a stable, the two Cargo files; `git diff main...HEAD` shows only the new entry and the two version lines.
- No test suite is run for this: nothing in it is code.

Your last step before you hand over and only if you committed anything: run `git merge main` inside your own workspace (`main` is the local branch named `main`, not `origin/main`). Never rebase. If git reports a conflict in a changelog file (another entry landed meanwhile), keep both entries in version order and commit the merge. Merge only once.

Hand over: your workspace path, branch and commit SHAs oldest first, the range you read (previous release, both source commits) and the entry as written. Second review and Rich's land follow.
