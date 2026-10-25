# British test inputs for the American spelling auto-fix

Test inputs written in British spelling on purpose. The `fixtures/` path
segment is exempt from `dialect-guard.sh`, and any fixer built on the generated
table must keep it exempt, or these inputs would be rewritten and every test
reading them would pass vacuously (plan check catch C8).

Streams A (the app's `richos-core` module, Rust) and B (the engine hook,
python3) put their British inputs HERE, or in a `fixtures/` directory of their
own. Rust tests can embed a file with `include_str!`; from a source file in
`richos/app/crates/richos-core/src/` the path climbs four levels to `richos/`
and continues into `engine/scripts/lib/dialect/fixtures/`, the same shape
`richos-voice` uses for its model pins.

| File | What it is |
|---|---|
| `british-keyed.txt` | 455 words of reply-style prose with an inline answer key: `{british|american}` must be fixed, `{word|=}` must be left. 106 must-fix items, 30 must-leave traps (names, code spans, a path, an identifier, a customer quotation, words that are already American, vocabulary rather than spelling). Written by clark-opus-amspell1 for the 2026-09-27 research, copied byte for byte from the private research kit (sha256 `07b0a0f987c95ff0e900fd90097b0c04144491921f20780e5649118c62859b95`). |
| `british-keyed.context-traps.txt` | the 7 must-leave words of that sample that the table DOES carry, so only context rules can keep them |
| `harper-review-list.tsv` | captured result of the one-time Harper cross-check: 25 verified forms, 18 fix and 7 leave |
| `stream-a-app.txt` | stream A's inputs for the app's fixer (`richos-core` `american_spelling.rs`), in named `=== section` blocks: protected contexts that must stay as written, prose around them with its expected American form, capitals and sentence starts, a word split across two deltas, a document and an edit fragment's file, and the one declared miss on the keyed sample |

To strip the key and get the plain British text, replace every
`{british|american}` with `british` and every `{word|=}` with `word`
(the regular expression `\{([^|{}]+)\|([^{}]*)\}`, group 1).

The generated table covers all 106 must-fix items of `british-keyed.txt` as
data (`dialect-table.test.py`, class `KeyedSample`). Whether a fixer actually
fixes them, and leaves all 30 traps, depends on its context rules, which is the
fixer's own test to prove.
