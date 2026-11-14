#!/bin/sh
# home-standard-repos.sh — two fictional companies' Git repositories, for the home screen's picture.
#
#   home-standard-repos.sh <folder> [documents in alder] [documents in brightwater]     (default 1800 and 900)
#
# WHY. The home screen draws every connected company from its own repositories (round 73, CEO
# 2026-11-13; `richos-core/src/home_repos.rs`). `seed-records.sh` writes loro records, which that
# picture no longer reads when a company has a repository, and no committed tool made a repository
# big enough to be drawn alone (2,500 objects) with more than one company in it. This writes
# <folder>/alder and <folder>/brightwater: Markdown documents in six folders, a decision register
# with twelve numbered rulings, a source file a day, eight working days, and a document in
# brightwater that names one in alder (a thread from one company to the other).
# `home-standard-walk.py` runs it in the guest. Everything is fictional and carries no names.
set -eu
ROOT="${1:?usage: home-standard-repos.sh <folder> [documents in alder] [documents in brightwater]}"
A="${2:-1800}"
B="${3:-900}"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=QA GIT_AUTHOR_EMAIL=qa@example.invalid GIT_COMMITTER_NAME=QA GIT_COMMITTER_EMAIL=qa@example.invalid

make() { # <name> <documents> <a path the readme names>
  repo="$ROOT/$1"
  n="$2"
  mkdir -p "$repo/wiki" "$repo/src"
  cd "$repo"
  git init -q
  printf '# %s\n\nThe chandlery ledger. See %s.\n' "$1" "$3" > README.md
  {
    printf '# Decisions\n\n'
    r=0
    while [ "$r" -lt 12 ]; do
      r=$((r + 1))
      printf '## %s. Berth %s is ordered monthly\n\nDecided 2026-08-03.\n\n' "$r" "$r"
    done
  } > wiki/decisions.md
  per=$((n / 8))
  i=0
  for day in 2026-08-03 2026-08-17 2026-09-01 2026-09-15 2026-10-01 2026-10-20 2026-11-05 2026-11-12; do
    j=0
    while [ "$j" -lt "$per" ]; do
      i=$((i + 1))
      j=$((j + 1))
      case $((i % 6)) in
        0) area=docs/plans ;;
        1) area=docs/notes ;;
        2) area=wiki ;;
        3) area=research ;;
        4) area=handbook ;;
        *) area=briefs ;;
      esac
      mkdir -p "$area"
      if [ $((i % 10)) -eq 0 ]; then
        printf '# Note %s\n\nPer §%s, and see wiki/decisions.md.\n' "$i" $((i % 12 + 1)) > "$area/note-$i.md"
      else
        printf '# Note %s\n\nThe harbor master at berth %s orders chandlery.\n' "$i" "$i" > "$area/note-$i.md"
      fi
    done
    printf '// %s\n' "$day" > "src/day-$day.rs"
    git add -A
    GIT_AUTHOR_DATE="${day}T12:00:00" GIT_COMMITTER_DATE="${day}T12:00:00" git commit -q -m "work"
  done
  printf '%s: %s documents, %s commits\n' "$1" "$(git ls-files '*.md' | wc -l | tr -d ' ')" "$(git rev-list --count HEAD)"
}

make alder "$A" docs/plans/note-6.md
make brightwater "$B" alder/README.md
