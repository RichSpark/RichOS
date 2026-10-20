# Next release version after <TAG>

<TAG> is a published stable release. Its changelog already exists. Write only the next patch version on your own branch. Never publish anything, write a changelog, commit or push main, touch the CEO's phones or app or change the host display.

1. Verify that <TAG> is a published non-prerelease on GitHub. Its form is vX.Y.Z.
2. If the package version in `richos/app/src-tauri/Cargo.toml` has already advanced past X.Y.Z, change nothing and report that. If it is older, stop and report the inconsistency to Rich. Otherwise advance it to X.Y.(Z+1).
3. Change the matching richos-tauri package version in `richos/app/src-tauri/Cargo.lock`. Leave dependency versions alone.
4. Run `cargo metadata --manifest-path richos/app/src-tauri/Cargo.toml --locked --offline --format-version 1 > /dev/null`. Report an offline refusal accurately.
5. Inspect the two-line diff, commit on your branch, merge local main once and hand over to Rich for independent review and normal landing. Do not ask the CEO for a version bump or changelog decision.

Rich owns completion and retries. Until this lands, the next nightly's existing stable-version collision guard refuses to build under a version already published.
