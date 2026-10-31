# Reference client modules

The phone web page that used to live here was retired on 2026-10-30; the native RichConnect apps replaced it. Do not add a screen, a service worker or anything a browser loads to this directory. See [README.md](README.md) for what remains and who loads it.

- The `lib/` modules are the reference client the protocol conformance corpus is generated from (`richos/mobile/conformance`). A change here changes what both native apps are held to: regenerate the corpus with `node richos/mobile/conformance/generate.mjs` and keep both native apps passing.
- Keep these modules headless with injectable dependencies and test them with `node --test test/*.test.js` in this directory.
