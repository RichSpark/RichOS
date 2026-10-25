# The American spelling table

`dialect-en-US.generated.dict` is a lookup table of British spellings and their
American forms, one pair per line. It is the data every part of the American
spelling auto-fix reads:

- the engine's write-time hook, in python3, at run time, from the installed engine;
- the RichOS app, compiled in with `include_str!` at build time (the same way
  `richos-voice` embeds `engine/voice/models/model-pins.json`), so no customer
  Mac needs Python or any other runtime for it.

It lives in the engine because the engine asset ships every tracked engine file
and the hook reads it from `$ENGINE_ROOT/scripts/lib/dialect/`; the app reaches
it by a relative path inside the same repository. One committed file, two
readers, no second copy.

## What is here

| File | What it is | Kept by |
|---|---|---|
| `dialect-en-US.generated.dict` | the table (generated; do not edit) | `dialect-table.py` |
| `dialect-table.py` | the generator, `--check`, `--report`, `--deny-lists` | hand |
| `dialect-table.test.py`, `dialect-table.test.sh` | its suite | hand |
| `dialect-en-US.overrides.dict` | additions VarCon's verified data lacks | hand |
| `dialect-en-US.leave.tsv` | forms VarCon would change that stay as written | hand |
| `../dialect-en-US.dict` | the dictionary `dialect-guard.sh` read until stream B, still the app lint's list until stream C; every line is carried into the table | hand |
| `dialect-fix.py` | the fixer: what is fixed, what is refused, Edit context, the change report; run by `scripts/hooks/dialect-guard.sh` and imported by `richos/app/scripts/lint/dialect.py` | hand |
| `third_party/varcon/` | VarCon 2020.12.07, pinned by sha256, with its README and notices | vendored, unmodified |
| `fixtures/` | British test inputs for the table and for the streams that use it | hand |

## Commands

```sh
python3 richos/engine/scripts/lib/dialect/dialect-table.py            # regenerate
python3 richos/engine/scripts/lib/dialect/dialect-table.py --check    # fail if stale
python3 richos/engine/scripts/lib/dialect/dialect-table.py --report   # every count
python3 richos/engine/scripts/lib/dialect/dialect-table.py --deny-lists
bash richos/engine/scripts/lib/dialect/dialect-table.test.sh          # the suite
```

Edit a hand-kept list, regenerate, commit both. The suite fails on a stale
table. `--deny-lists` is the one-time check that no pair in the table trips the
named-person or publication guards; it needs this Mac's private deny-list and
the private record, so it is run by hand, not by the suite.

## How the pairs are chosen

The generator's docstring states the rules; the measurement behind them is
`richos-hq docs/research/2026-09-27-american-spelling-autofix.md` in the private
record. In short: British forms from VarCon's verified clusters at SCOWL level
70 or below, excluding any form that is also a preferred American spelling in
some sense, keeping forms American dictionaries only tolerate under "always the
preferred American form", then the leave list removed and the hand-kept
additions merged. Every disagreement between a hand-kept list and VarCon stops
the generator instead of being resolved silently.

## Exemptions from the dialect guard

The three vocabulary files (the table, the overrides, the leave list) are named
by exact basename in `dialect-fix.py`'s own-file list. `third_party/` and
`fixtures/` are exempt by path segment and `third_party/varcon` and the table
are registered in `.richos/vendored-material` as third-party material. The
generator, its suite and this page carry no word the table would change and
the suite proves it, so they need no exemption.

## License

VarCon is under its own notice-only terms; the table is a modified version of
it and says so in its header. Full notices: `third_party/varcon/README` and
`docs/legal/THIRD-PARTY-NOTICES.md`.
