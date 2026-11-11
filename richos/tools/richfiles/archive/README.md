# RichFiles ZIP helper

This macOS command uses the same pinned ZIP and Deflate dependencies as RichOS.

```text
richfiles-archive list ZIP
richfiles-archive create DEST NAME.zip -- SOURCE...
richfiles-archive extract ZIP DEST FOLDER [-- INDEX...]
```

Pass source and destination paths as absolute paths and names as single path components. `list` emits a JSON array of entries with `index`, `path`, `isDirectory` and `size`. Directory paths omit the final slash. `create` and `extract` emit a JSON object with the absolute `path` of the published result. Extraction selects all entries when no indices are supplied. Selecting a directory also selects its descendants. Failures return a nonzero exit status and write a reason to stderr.

Creation and extraction write to a private stage on the destination volume. Publication uses macOS exclusive rename, so an occupied name stops the operation and its existing item stays in place. Both operations retain their inputs. Termination signals request cancellation between I/O steps and allow ordinary staging cleanup.

The helper supports ordinary files and directories in single-volume ZIPs using Store or Deflate. It checks entry paths and types before extraction and verifies each stream against the declared length. The limits are 20,000 entries, 32 MiB of catalog data and 16 GiB of uncompressed data. Extracted files use safe local permissions. ZIP encryption, links and special files are unsupported.
