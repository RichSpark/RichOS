# Pierce evidence access

The desktop host supplies the selected company's connected folders through
`RICHOS_APP_READ_ROOTS`, plus the current session's evidence directory. Terminal
configuration and paths in the brief cannot expand that desktop boundary.

A terminal project can declare its private repositories and evidence folders in
its own `orchestration.config`, independently of citations in individual briefs:

```sh
PIERCE_READ_ROOTS='["/absolute/company-folder","/absolute/evidence-folder"]'
```

Use existing directories. Spaces and `#` inside a quoted path are supported.
Paths are canonicalized, including symlinks. Relative paths, the filesystem root,
the whole home directory and unavailable directories are refused. This is literal
JSON in one shell-quoted value; no commands or variable substitutions execute.
An absent setting grants no additional directory. Cited existing paths retain
their terminal read access. The reviewer still has only Read, Glob and Grep.

Opus and its optional terminal Haiku shadow receive the exact same request and
read roots. Native records and independent collection manifests pin the canonical
configured grants as `access_sha256`, alongside source, profile and rules hashes.
Changing those grants requires a new `PIERCE_SHADOW_PERIOD`. A mismatched manifest
is left intact and cannot collect changed-input results. Queued twins recheck the
access identity before their provider call. Revisions cannot cross this boundary.

Keep production captures separate from controlled provider fixtures. Preserve
unavailable outcomes, refusals and exclusions. Agreement and finding matches do
not establish accuracy; private source-based grading is still required.
