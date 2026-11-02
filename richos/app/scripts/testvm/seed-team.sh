#!/bin/sh
# seed-team.sh — write a team of N hired teammates into an app data folder, in the guest.
#
#   seed-team.sh <app data folder> [N]      (N from 1 to 35, default 35)
#
# One definition per teammate in <data>/team/<name>.md, heading `# Name <separator> Role`, the
# separators cycling through " — " (what Dean's template writes), ": " and " - " (what five of the
# CEO's own teammates use), so the home screen's name-and-role reading is seen on all three. The
# first five names (finn, gavin, grant, lance, sage) are the ones whose headings used to show as
# one long name. Used by the home-screen "Working now" walk (2026-11-02); fictional names only.
set -eu
DATA="${1:?usage: seed-team.sh <app data folder> [N]}"
N="${2:-35}"
mkdir -p "$DATA/team"
i=0
for who in "finn:Finance Lead" "gavin:Growth Marketer" "grant:Grants Writer" "lance:Legal Reviewer" "sage:Software Architect" \
  "beth:Bookkeeper" "will:Marketing Director" "nora:Operations Lead" "owen:Data Analyst" "pia:Product Designer" \
  "quinn:Quality Reviewer" "rhea:Research Lead" "sam:Sales Coach" "tess:Technical Writer" "uri:Support Lead" \
  "vera:Video Producer" "wes:Web Developer" "xena:Events Planner" "yuri:Yield Analyst" "zane:Zoning Advisor" \
  "abe:Audit Assistant" "bea:Brand Strategist" "cal:Contracts Manager" "dina:Design Researcher" "eli:Email Specialist" \
  "faye:Forecast Analyst" "gus:Growth Analyst" "hana:HR Partner" "ivan:Investor Relations" "jade:Journalist" \
  "kyle:K-12 Liaison" "lena:Localization Lead" "milo:Market Researcher" "nell:Newsletter Editor" "otis:Onboarding Lead"; do
  i=$((i + 1))
  [ "$i" -le "$N" ] || break
  name="${who%%:*}"
  role="${who#*:}"
  cap="$(printf '%s' "$name" | cut -c1 | tr '[:lower:]' '[:upper:]')$(printf '%s' "$name" | cut -c2-)"
  case $((i % 3)) in
    1) heading="$cap — $role" ;;
    2) heading="$cap: $role" ;;
    *) heading="$cap - $role" ;;
  esac
  printf -- '---\nname: %s\nmodel: sonnet\n---\n\n# %s\n\nYou are %s.\n' "$name" "$heading" "$cap" > "$DATA/team/$name.md"
done
find "$DATA/team" -name '*.md' | wc -l
