#!/bin/sh
# usage-shape.sh — what SHAPE is Claude Code's usage answer, and could RichOS read it?
#
#   usage-shape.sh [--claude BIN] [--home DIR] [--config-dir DIR] [--wait SECONDS]
#   usage-shape.sh --help
#
# Asks a `claude` for its usage exactly as RichOS's quota reader does
# (crates/richos-core/src/quota/probe.rs: the same flags, an `initialize` then a
# `get_usage` control request, the same request shape) and prints only the SHAPE
# of the answer: the payload's keys, `rate_limits_available`, and for each window
# under `rate_limits` its keys and whether `utilization` is a number. No figure,
# token, account name or session field is printed or kept.
#
# Exit 0  the answer is one RichOS's reader accepts (quota.rs `normalize`:
#         rate_limits_available true, rate_limits an object, at least one window
#         with a numeric utilization).
# Exit 1  Claude Code answered, but the reader gets no figures from it; the line
#         "verdict:" says which rule applies. "NO READING" is `rate_limits: null`,
#         which the reader takes as no figures this time, not an error (quota.rs
#         `ReadError::NoReading`, since 2bb7359ad); "UNREADABLE" is any other shape
#         it refuses (`Malformed`, `Unsupported`, `Failed`).
# Exit 2  no answer to read: no such binary, or no get_usage response in time.
#
# Run it where the `claude` under test runs. In the test VM: push it with a
# steps-walk `push` step and run it with a `guest` step, for example
#   sh /Users/admin/usage-shape.sh --home "$(ls -d /Users/admin/testvm/walk-* | head -1)/home"
#
# WHY IT EXISTS: on 2026-10-05 the candidate-36 walk found the quota panel saying
# "Claude Code returned quota data this version of RichOS could not read" for a
# real signed-in account, with + Add account gone. The app does not log the
# answer it refused, so the shape had to be read from the outside, the same way
# the app asks. A check that cannot print what it was given is a guess.
set -u
CLAUDE="${RICHOS_CLAUDE_BIN:-$HOME/.local/bin/claude}"
HOME_DIR=""
CONFIG_DIR=""
WAIT=20
while [ $# -gt 0 ]; do
  case "$1" in
    --help|-h) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --claude) CLAUDE="${2:-}"; shift 2 ;;
    --home) HOME_DIR="${2:-}"; shift 2 ;;
    --config-dir) CONFIG_DIR="${2:-}"; shift 2 ;;
    --wait) WAIT="${2:-}"; shift 2 ;;
    *) echo "usage-shape.sh: unknown argument '$1' (see --help)" >&2; exit 2 ;;
  esac
done
case "$WAIT" in ''|*[!0-9]*) echo "usage-shape.sh: --wait takes whole seconds, got '$WAIT'" >&2; exit 2 ;; esac
[ "$WAIT" -ge 2 ] || { echo "usage-shape.sh: --wait must be at least 2 seconds" >&2; exit 2; }
if [ ! -x "$CLAUDE" ]; then
  echo "usage-shape.sh: no claude to ask: '$CLAUDE' is not an executable file" >&2
  exit 2
fi
if [ -n "$HOME_DIR" ]; then
  [ -d "$HOME_DIR" ] || { echo "usage-shape.sh: --home '$HOME_DIR' is not a directory" >&2; exit 2; }
  export HOME="$HOME_DIR"
  [ -n "$CONFIG_DIR" ] || CONFIG_DIR="$HOME_DIR/.claude"
fi
[ -z "$CONFIG_DIR" ] || export CLAUDE_CONFIG_DIR="$CONFIG_DIR"
unset CLAUDECODE
RAW="$(mktemp "${TMPDIR:-/tmp}/usage-shape.XXXXXX")" || { echo "usage-shape.sh: cannot make a temporary file" >&2; exit 2; }
trap 'rm -f "$RAW"' EXIT INT TERM
GAP=$((WAIT / 2))
echo "claude: $("$CLAUDE" --version 2>&1 | head -1)"
{
  printf '%s\n' '{"type":"control_request","request_id":"quota-1","request":{"subtype":"initialize","hooks":{}}}'
  sleep "$GAP"
  printf '%s\n' '{"type":"control_request","request_id":"quota-2","request":{"subtype":"get_usage","hooks":{}}}'
  sleep "$GAP"
} | (cd "${HOME:-/}" && "$CLAUDE" --print --input-format=stream-json --output-format=stream-json \
      --verbose --setting-sources "" --no-session-persistence --tools "" --strict-mcp-config \
      --mcp-config '{"mcpServers":{}}') > "$RAW" 2>/dev/null
/usr/bin/perl -MJSON::PP -e '
  my ($answered, $verdict, $label) = (0, "", "UNREADABLE");
  while (my $line = <STDIN>) {
    my $v = eval { JSON::PP->new->decode($line) } or next;
    next unless ($v->{type} // "") eq "control_response";
    my $r = $v->{response} // {};
    print "response id=", ($r->{request_id} // "?"), " subtype=", ($r->{subtype} // "?"), "\n";
    next unless ($r->{request_id} // "") eq "quota-2";
    $answered = 1;
    if (($r->{subtype} // "") ne "success") { $verdict = "get_usage did not succeed (the reader calls this Failed)"; next; }
    my $p = $r->{response};
    if (ref $p ne "HASH") { print "  payload: not an object\n"; $verdict = "the payload is not an object"; next; }
    print "  payload keys: ", join(",", sort keys %$p), "\n";
    my $a = $p->{rate_limits_available};
    my $as = !defined $a ? "absent" : (JSON::PP::is_bool($a) ? ($a ? "true" : "false") : "not a boolean");
    print "  rate_limits_available: $as\n";
    my $l = $p->{rate_limits};
    my $numeric = 0;
    if (ref $l eq "HASH") {
      for my $k (sort keys %$l) {
        my $w = $l->{$k};
        if (ref $w eq "HASH") {
          my $u = $w->{utilization};
          my $num = defined $u && !ref $u && $u =~ /^-?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?$/;
          $numeric++ if $num && ($k eq "five_hour" || $k eq "seven_day" || $k =~ /^seven_day_(opus|sonnet)$/);
          print "  rate_limits.$k: keys=", join(",", sort keys %$w), " utilization=", (!defined $u ? "absent" : ($num ? "number" : "not a number")), "\n";
        } elsif (ref $w eq "ARRAY") {
          for my $m (@$w) { $numeric++ if $k eq "model_scoped" && ref $m eq "HASH" && defined $m->{utilization} && !ref $m->{utilization} && $m->{utilization} =~ /^-?[0-9.]+$/ && defined $m->{display_name}; }
          print "  rate_limits.$k: array of ", scalar(@$w), "\n";
        } else {
          print "  rate_limits.$k: ", (defined $w ? "a bare value" : "null"), "\n";
        }
      }
    } else {
      print "  rate_limits: ", (!exists $p->{rate_limits} ? "absent" : (!defined $l ? "null" : "not an object")), "\n";
    }
    if ($as eq "false") { $verdict = "rate_limits_available is false (the reader calls this Unsupported)"; }
    elsif ($as ne "true") { $verdict = "rate_limits_available is $as (the reader calls this Malformed)"; }
    elsif (exists $p->{rate_limits} && !defined $l) { $label = "NO READING"; $verdict = "rate_limits is null (the reader calls this NoReading: no figures this time, not an error)"; }
    elsif (ref $l ne "HASH") { $verdict = "rate_limits is " . (!exists $p->{rate_limits} ? "absent" : (!defined $l ? "null" : "not an object")) . " (the reader calls this Malformed)"; }
    elsif (!$numeric) { $verdict = "no window carries a numeric utilization (the reader calls this Malformed)"; }
  }
  if (!$answered) { print STDERR "usage-shape.sh: claude gave no get_usage response within the wait\n"; exit 2; }
  if ($verdict ne "") { print "verdict: $label: $verdict\n"; exit 1; }
  print "verdict: readable by RichOS quota reader\n"; exit 0;
' < "$RAW"
