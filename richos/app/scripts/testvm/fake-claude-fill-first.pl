#!/usr/bin/perl
# A fake `claude` for the fill-first VM check. It answers what RichOS asks of the binary:
# `auth status/login/logout`, `--version`, the control-only quota probe (`get_usage`) and
# plain turns, reading each account's figures from a file the walk writes:
#   Account 1 (no CLAUDE_CONFIG_DIR of RichOS's own): /Users/admin/fill-first/usage-1.json # machine-path-exempt: isolated guest fixture documentation; no customer default
#   an added account: <its CLAUDE_CONFIG_DIR>/usage.json
# A usage file is {"five": N, "weekly": N}, optionally with "five_at" and "week_at" (epoch
# seconds) for when each window resets; without them a window resets 3 h / 4 d after this
# process started. A usage file {"null": true} answers get_usage as Claude Code 2.1.289 does
# some of the time: rate_limits_available true and rate_limits null (walk of nightly 36, D1,
# qa/usage-shape.sh), and a turn then streams no rate_limit_event. While /Users/admin/fill-first/slow exists, a turn is held open before it # machine-path-exempt: isolated guest fixture documentation; no customer default
# answers (at most 10 minutes), so a walk can photograph a turn in progress. Only the user's
# own turns are held: the app's internal turns (the handoff summary and the re-prime of a new
# lease, both "[INTERNAL ...") answer at once, because a lease is installed only after its
# priming answers (spine.rs rotate_lease), and holding the priming kept the conversation on
# the old lease's session while the walk photographed it (2026-10-05 run, shot 4).
#
# A LEASE'S EVIDENCE. Claude Code's own SessionStart hook (engine app-evidence.py) creates
# <data>/engine-state/evidence/<session>/ with .lock and callbacks.jsonl; the app reads it
# for the working agents and refuses a turn end it cannot read (native.rs worker settlement:
# "Stopped ... I don't recognize what came back"). This fake runs no hooks, so it writes that
# journal itself, with the same row shape, when it starts as a lease (--session-id), and, while
# /Users/admin/fill-first/agents exists, the provider's rows for one background agent per line # machine-path-exempt: isolated guest fixture documentation; no customer default
# of that file on the first user turn (SubagentStart and PostToolUse[Agent] "async_launched",
# the rows app_workers.rs reads). /Users/admin/fill-first/reply.txt, when present, is the answer. # machine-path-exempt: isolated guest fixture documentation; no customer default
#
# HELPER STEPS (weekly-switch handoff plan 2026-10-07, slice 6). While the agents file lists
# helpers, each user turn (and each second of a held turn) is one step of every helper that has
# not ended: the fake runs the gate command from the lease's <plugin-dir>/hooks/hooks.json
# (PreToolUse), with a helper payload (agent_id walk-agent-N) on stdin and the lease's own
# environment, as Claude Code would. When the gate refuses with the order (exit 2, "Stop the task
# now" on stderr), the fake writes that helper's SubagentStop row; an admitted step writes
# nothing. Every step is a line in calls.log ("gate <agent> exit <n>"). Without helpers, or
# without a hooks.json, no step runs and nothing here changes.
#
# THE BACK END'S JOB (round 1 of the weekly-switch handoff test, 2026-10-07). The handoff
# continuation lives in the work host (work_host.rs continuation_after_a_helper_ended), which
# runs only for a job the front desk wrote down; helpers on the front desk's own lease never
# reach it. Two files make a job with a helper on the back end, with no model:
#   register      while it exists, the front desk's user turn whose text carries the file's
#                 text (a lease whose --mcp-config has richos_assignments; never the app's own
#                 priming turn) calls that server's `record` tool with the file's text as the
#                 assignment, as Claude Code would, and says the receipt's words. The
#                 file is taken (renamed) first, so the job is written down once. calls.log:
#                 "register ok <text>" or "register error <text>". A file of several lines
#                 writes one job per line, in order, in that one turn (the turn's text carries
#                 the first line); a one-line file is exactly the single job above.
#
# NOBODY SIGNED IN (the held queue, richos-hq plan 2026-10-09 §4 row 5). While
# /Users/admin/fill-first/signed-out exists, `auth status --json` answers loggedIn false with exit # machine-path-exempt: isolated guest fixture documentation; no customer default
# 1, and every BACK-END user turn is refused the way Claude Code 2.1.295 refuses one with no login
# (measured with a scratch HOME): system/init, a <synthetic> assistant text "Not logged in ·
# Please run /login", then a result with is_error true and no errors key. The front desk's turns
# are answered as usual: the walk writes its jobs down in a turn that ran just before the sign-in
# ran out. While /Users/admin/fill-first/log-work-turns exists, each back-end user turn is a line # machine-path-exempt: isolated guest fixture documentation; no customer default
# in calls.log: "work-turn refused <text>" or "work-turn answered <text>", and each front-desk user
# turn "desk-turn <text>" (first 400 chars), so a walk can tell which lease took which turn.
#   work-agents   one helper per line, launched on the FIRST back-end lease's first user turn (a
#                 lease whose --mcp-config has richos_work), as work-agent-N; once per walk
#                 (work-agents-started). A background helper keeps running after the back end's
#                 turn ends, so after that turn a stepper child steps every open work-agent
#                 every 2 s (the same gate step as above) until each has ended, for at most 10
#                 minutes, and ends with this process.
# Nothing here reaches a network.
use strict; use warnings; use JSON::PP; use Fcntl qw(:flock O_CREAT O_EXCL O_WRONLY); use File::Path qw(make_path);
use IPC::Open2 qw(open2); use POSIX qw(WNOHANG);
$| = 1;
# The walk's folder in the guest; RICHOS_FILL_FIRST_DIR moves it for a check off the guest.
my $dir = $ENV{RICHOS_FILL_FIRST_DIR} // ((getpwuid($<))[7] . '/fill-first');
my $json = JSON::PP->new->canonical;
my $folder = $ENV{CLAUDE_CONFIG_DIR} // '';
my $file = $folder =~ /claude-accounts/ ? "$folder/usage.json" : "$dir/usage-1.json";
# Defaults when the walk has not written a file yet: Account 1 40/30, an added account 10/20,
# so a newly added account never reads as a jump from zero.
my $default = $folder =~ /claude-accounts/ ? { five => 10, weekly => 20 } : { five => 40, weekly => 30 };
sub usage {
  open(my $fh, '<', $file) or return $default;
  local $/; my $text = <$fh>; close $fh;
  return eval { $json->decode($text) } || $default;
}
open(my $log, '>>', "$dir/calls.log");
print $log join(' ', time(), ($folder || 'account-1'), @ARGV), "\n"; close $log;
if (grep { $_ eq '--version' } @ARGV) { print "2.1.288 (Claude Code)\n"; exit 0; }
# WHO EACH FOLDER IS SIGNED IN AS (round 18's "That is the account you already use"): `auth
# status` names an email, as Claude Code's does. Account 1 is account-1@fixture.invalid; an
# added folder is added-<n>@fixture.invalid unless its sign-in wrote another. While
# /Users/admin/fill-first/login-as exists, `auth login` under a folder signs that folder in as # machine-path-exempt: isolated guest fixture documentation; no customer default
# the address it holds, so a walk can sign in with the account already in use. No network.
if (@ARGV && $ARGV[0] eq 'auth') {
  my ($id) = $folder =~ /claude-accounts\/(\d+)/;
  if (($ARGV[1] // '') eq 'login' && $folder && open(my $as, '<', "$dir/login-as")) {
    my $who = <$as> // ''; close $as; $who =~ s/\s+\z//;
    if (length $who && open(my $out, '>', "$folder/email")) { print $out $who; close $out; }
  }
  if (($ARGV[1] // '') eq 'status') {
    if (-e "$dir/signed-out") {
      print $json->encode({ loggedIn => JSON::PP::false, authMethod => 'none', configDirectory => $folder }), "\n";
      exit 1;
    }
    # Account 1 is any folder that is not an added account's: the app names it explicitly
    # ($HOME/.claude), so "a folder was named" does not mean "an added account".
    my $email = defined $id ? "added-$id\@fixture.invalid" : 'account-1@fixture.invalid';
    if (defined $id && open(my $in, '<', "$folder/email")) { my $e = <$in> // ''; close $in; $e =~ s/\s+\z//; $email = $e if length $e; }
    print $json->encode({ loggedIn => JSON::PP::true, configDirectory => $folder, email => $email, orgId => 'fixture-org' }), "\n";
  }
  exit 0;
}
my $five_reset = time() + 3 * 3600; my $week_reset = time() + 4 * 86400;
sub arg_after { my ($flag, $match) = @_; for my $i (0 .. $#ARGV - 1) { return $ARGV[$i + 1] if $ARGV[$i] eq $flag && (!$match || $ARGV[$i + 1] =~ $match); } return undef }
my $session = arg_after('--session-id');
my $profile = arg_after('--plugin-dir', qr{/engine-profiles/});
my $evidence;
if ($session && $profile && $session =~ /\A[A-Za-z0-9_-]{1,128}\z/) {
  (my $data = $profile) =~ s{/engine-profiles/[^/]+/?\z}{};
  $evidence = "$data/engine-state/evidence/$session";
  make_path($evidence, { mode => 0700 });
}
sub journal {  # append callback rows under the journal's lock, as app-evidence.py does
  return unless $evidence;
  open(my $lock, '>>', "$evidence/.lock") or return; flock($lock, LOCK_EX);
  open(my $out, '>>', "$evidence/callbacks.jsonl") or return;
  print $out $json->encode({ schema => 1, callback => { session_id => $session, %$_ } }), "\n" for @_;
  close $out; close $lock;
}
journal({ hook_event_name => 'SessionStart' });
my %ended;
sub gate_command {  # the PreToolUse command of the lease's hooks.json, or undef
  return undef unless $profile && open(my $fh, '<', "$profile/hooks/hooks.json");
  local $/; my $text = <$fh>; close $fh;
  my $m = eval { $json->decode($text) } or return undef;
  return $m->{hooks}{PreToolUse}[0]{hooks}[0]{command};
}
sub step_one {  # one gate step of one helper; true when the gate ordered it to stop and it ended
  my ($command, $id, $type) = @_;
  my $payload = $json->encode({ hook_event_name => 'PreToolUse', session_id => $session, agent_id => $id,
    agent_type => $type, tool_name => 'Bash', tool_input => { command => 'true' } });
  my $in = "$dir/gate-in-$$-$id.json"; my $err = "$dir/gate-err-$$-$id.txt";
  open(my $w, '>', $in) or return 0; print $w $payload; close $w;
  system("($command) < '$in' > /dev/null 2> '$err'");
  my $code = $? >> 8;
  my $said = ''; if (open(my $e, '<', $err)) { local $/; $said = <$e> // ''; close $e; }
  unlink $in, $err;
  # The order is named as such; any other refusal carries the gate's first words, because an
  # exit 2 alone cannot tell the order from the hook refusing for another reason (walk
  # walk-1db6a7b8afee: every step exited 2, from before the cut-off on).
  my $ordered = $code == 2 && $said =~ /Stop the task now/;
  my ($first) = split /\n/, $said; $first //= '';
  if (open(my $l, '>>', "$dir/calls.log")) { print $l join(' ', time(), ($folder || 'account-1'), 'gate', $id, 'exit', $code,
    $ordered ? 'order' : length $first ? ('said:', substr($first, 0, 200)) : ()), "\n"; close $l; }
  return 0 unless $ordered;
  journal({ hook_event_name => 'SubagentStop', agent_id => $id, agent_type => $type });
  return 1;
}
sub open_work_helpers {  # [id, type] of every work-agent this lease launched that has not ended
  return () unless $evidence && open(my $in, '<', "$evidence/callbacks.jsonl");
  my (%type, %stopped, @order);
  while (my $row = <$in>) {
    my $c = (eval { $json->decode($row) } || {})->{callback} or next;
    my $id = $c->{agent_id} // ''; next unless $id =~ /\Awork-agent-\d+\z/;
    if (($c->{hook_event_name} // '') eq 'SubagentStart') { push @order, $id unless exists $type{$id}; $type{$id} = $c->{agent_type} // 'helper'; }
    $stopped{$id} = 1 if ($c->{hook_event_name} // '') eq 'SubagentStop';
  }
  close $in;
  return map { [$_, $type{$_}] } grep { !$stopped{$_} } @order;
}
sub helper_steps {  # one step of every listed helper that has not ended
  my $command = $evidence ? gate_command() : undef;
  return unless $command;
  if (open(my $names, '<', "$dir/agents")) {
    my @who = grep { length } map { s/\s+\z//r } <$names>; close $names;
    my $n = 0;
    for my $type (@who) {
      $n++; my $id = "walk-agent-$n";
      next if $ended{$id};
      $ended{$id} = 1 if step_one($command, $id, $type);
    }
  }
  step_one($command, @$_) for open_work_helpers();
}
# The lease's MCP servers, from --mcp-config as the app passes it: the front desk carries the
# register (richos_assignments), the back end carries the work tools (richos_work).
my $servers = eval { $json->decode(arg_after('--mcp-config') // '{}')->{mcpServers} } || {};
my $backend = exists $servers->{richos_work};
sub calls_line { if (open(my $l, '>>', "$dir/calls.log")) { print $l join(' ', time(), ($folder || 'account-1'), @_), "\n"; close $l; } }
sub register_job {  # the front desk writes the job down through the app's own register; its words, or undef
  my ($sent) = @_;
  my $server = $servers->{richos_assignments} or return undef;
  # Only on the turn that carries his words: the app primes the front desk with a turn of its
  # own (<executive-continuity>), and the register refuses that one ("This conversation is not
  # open for new assignments right now"; walk walk-4ec19e8a365e).
  open(my $fh, '<', "$dir/register") or return undef;
  my @jobs = grep { length } map { my $t = $_; $t =~ s/\s+/ /g; $t =~ s/\A //; $t =~ s/ \z//; $t } <$fh>; close $fh;
  return undef unless @jobs && index($sent, $jobs[0]) >= 0;
  my $taken = "$dir/register.taken.$$";
  rename("$dir/register", $taken) or return undef;
  my ($from, $to);
  my $pid = eval { open2($from, $to, $server->{command}, @{ $server->{args} || [] }) };
  unless ($pid) { calls_line('register', 'error', 'the register could not be started'); return undef; }
  my $n = 1;
  print $to $json->encode($_), "\n" for
    { jsonrpc => '2.0', id => 1, method => 'initialize',
      params => { protocolVersion => '2025-06-18', capabilities => {}, clientInfo => { name => 'fake-claude', version => '1' } } },
    { jsonrpc => '2.0', method => 'notifications/initialized' },
    map { $n++; { jsonrpc => '2.0', id => $n, method => 'tools/call', params => { name => 'record', arguments => { assignment => $_ } } } } @jobs;
  close $to;
  my ($answered, @words) = (0);
  while (my $line = <$from>) {
    my $reply = eval { $json->decode($line) } or next;
    my $rid = $reply->{id} // ''; next unless $rid =~ /\A\d+\z/ && $rid >= 2;
    my ($ok, $result) = (0, $reply->{result} || {});
    my $said = $result->{content} && $result->{content}[0] ? $result->{content}[0]{text} // '' : ($reply->{error}{message} // '');
    if ($result->{content} && !$result->{isError}) {
      my $receipt = eval { $json->decode($said) } || {};
      $ok = $receipt->{recorded} ? 1 : 0; push @words, $receipt->{say} if defined $receipt->{say};
    }
    calls_line('register', $ok ? 'ok' : 'error', ($said =~ s/\s+/ /gr));
    last if ++$answered == @jobs;
  }
  close $from; waitpid($pid, 0);
  return @words ? join(' ', @words) : undef;
}
my $stepper;  # the pid of this lease's background stepper, while it runs
sub step_in_background {  # the work-agents keep running after the back end's turn ends
  return unless $backend && open_work_helpers();
  return if $stepper && waitpid($stepper, WNOHANG) == 0;
  my $parent = $$;
  my $pid = fork();
  return unless defined $pid;
  if ($pid) { $stepper = $pid; return; }
  open(STDIN, '<', '/dev/null'); open(STDOUT, '>', '/dev/null');
  for (1 .. 300) {
    sleep 2;
    POSIX::_exit(0) if getppid() != $parent;
    helper_steps();
    POSIX::_exit(0) unless open_work_helpers();
  }
  POSIX::_exit(0);
}
sub iso { my @t = gmtime(shift); sprintf('%04d-%02d-%02dT%02d:%02d:%02dZ', $t[5]+1900, $t[4]+1, $t[3], $t[2], $t[1], $t[0]) }
while (my $line = <STDIN>) {
  my $msg = eval { $json->decode($line) } or next;
  my $type = $msg->{type} // '';
  if ($type eq 'control_request') {
    my $kind = $msg->{request}{subtype} // '';
    my $answer = {};
    if ($kind eq 'get_usage') {
      my $u = usage();
      $answer = $u->{null} ? { rate_limits_available => JSON::PP::true, rate_limits => undef }
        : { rate_limits_available => JSON::PP::true, rate_limits => {
        five_hour => { utilization => $u->{five} + 0, resets_at => iso($u->{five_at} // $five_reset) },
        seven_day => { utilization => $u->{weekly} + 0, resets_at => iso($u->{week_at} // $week_reset) } } };
    }
    print $json->encode({ type => 'control_response', response => { subtype => 'success', request_id => $msg->{request_id}, response => $answer } }), "\n";
  } elsif ($type eq 'user') {
    # Claude Code's startup inventory, with every turn, in the shape testvm/permission-provider.py
    # proved on the real app (the S7 walk). Without it a company conversation's turn is refused
    # before it starts: "The connection did not report whether company interview tools loaded."
    # (native.rs ensure_onboarding_tools_loaded; the 2026-10-05 round-16 walk, relaunch log).
    # These are declarations only; this fake executes none of these tools. A back-end lease
    # also declares the five work tools: without them the app refuses its job before and after
    # its turn ("The desktop work tools did not load", native.rs work_readiness_facts).
    my @work = $backend ? map { "mcp__richos_work__$_" } qw(repositories prepare inspect integrate complete) : ();
    print $json->encode({ type => 'system', subtype => 'init', model => 'fill-first-fixture',
      plugins => [ { name => 'rich-skills' }, { name => 'richos-app-engine' } ], permissionMode => 'bypassPermissions',
      tools => [ 'Bash', 'mcp__richos_onboarding__save_company_notes', 'mcp__richos_onboarding__decline_onboarding',
        'mcp__richos_continuity__checkpoint', 'mcp__richos_continuity__inspect',
        'mcp__richos_assignments__record', 'mcp__richos_status__background_work', @work ] }), "\n";
    my $internal =index($json->encode($msg->{message} // {}), '[INTERNAL') >= 0;
    # While /Users/admin/fill-first/log-turns exists, each user turn's text (first 400 chars, one # machine-path-exempt: isolated guest fixture documentation
    # line) goes to calls.log as "turn <text>", so a walk can read what the app sent to the back end.
    my $c = $msg->{message}{content};
    my $sent = ref $c eq 'ARRAY' ? join(' ', map { ref $_ eq 'HASH' ? ($_->{text} // '') : '' } @$c) : ($c // '');
    $sent =~ s/\s+/ /g;
    if (!$internal && -e "$dir/log-turns" && open(my $tl, '>>', "$dir/calls.log")) {
      print $tl join(' ', time(), ($folder || 'account-1'), 'turn', substr($sent, 0, 400)), "\n"; close $tl;
    }
    if (!$internal && $evidence && open(my $names, '<', "$dir/agents")) {
      my @who = grep { length } map { s/\s+\z//r } <$names>; close $names;
      my $seen = '';
      if (open(my $in, '<', "$evidence/callbacks.jsonl")) { local $/; $seen = <$in> // ''; close $in; }
      if ($seen !~ /walk-agent-/) {
        my $n = 0;
        journal(map { $n++; ({ hook_event_name => 'SubagentStart', agent_id => "walk-agent-$n", agent_type => $_ },
          { hook_event_name => 'PostToolUse', tool_name => 'Agent', tool_response => { status => 'async_launched', agentId => "walk-agent-$n" } }) } @who);
      }
    }
    # The back end launches the work-agents in the background, once per walk (THE BACK END'S JOB).
    if (!$internal && $backend && $evidence && open(my $names, '<', "$dir/work-agents")) {
      my @who = grep { length } map { s/\s+\z//r } <$names>; close $names;
      if (@who && sysopen(my $once, "$dir/work-agents-started", O_CREAT | O_EXCL | O_WRONLY)) {
        close $once;
        my $n = 0;
        journal(map { $n++; ({ hook_event_name => 'SubagentStart', agent_id => "work-agent-$n", agent_type => $_ },
          { hook_event_name => 'PostToolUse', tool_name => 'Agent', tool_response => { status => 'async_launched', agentId => "work-agent-$n" } }) } @who);
      }
    }
    # NOBODY SIGNED IN: a back-end turn is refused as Claude Code refuses it (header).
    if (!$internal && $backend && -e "$dir/signed-out") {
      calls_line('work-turn', 'refused', substr($sent, 0, 400)) if -e "$dir/log-work-turns";
      # Escaped to ASCII: the middle dot is not a byte this fake's other frames ever print.
      my $ascii = JSON::PP->new->canonical->ascii;
      my $refused = "Not logged in \x{b7} Please run /login";
      print $ascii->encode({ type => 'assistant', message => { model => '<synthetic>', role => 'assistant',
        content => [ { type => 'text', text => $refused } ] } }), "\n";
      print $ascii->encode({ type => 'result', subtype => 'success', is_error => JSON::PP::true, result => $refused,
        stop_reason => 'stop_sequence', terminal_reason => 'api_error' }), "\n";
      next;
    }
    calls_line($backend ? ('work-turn', 'answered') : ('desk-turn'), substr($sent, 0, 400)) if !$internal && -e "$dir/log-work-turns";
    my $receipt = $internal ? undef : register_job($sent);
    unless ($internal) {
      helper_steps();
      for (1 .. 600) { last unless -e "$dir/slow"; sleep 1; helper_steps(); }
    }
    my $u = usage();
    print $json->encode({ type => 'rate_limit_event', rate_limit_info => { status => 'allowed', unifiedWindows => {
      five_hour => { utilization => $u->{five} / 100, resetsAt => $u->{five_at} // $five_reset },
      seven_day => { utilization => $u->{weekly} / 100, resetsAt => $u->{week_at} // $week_reset } } } }), "\n" unless $u->{null};
    my $who = $folder =~ /claude-accounts\/(\d+)/ ? "account $1" : 'Account 1';
    my $text = "Answered on $who.";
    if (!$internal && open(my $r, '<', "$dir/reply.txt")) { local $/; my $t = <$r>; close $r; $text = $t =~ s/\s+\z//r if defined $t && length $t; }
    $text = $receipt if defined $receipt && length $receipt;
    $text = 'ready' if $internal;
    print $json->encode({ type => 'assistant', message => { role => 'assistant', content => [ { type => 'text', text => $text } ] } }), "\n";
    print $json->encode({ type => 'result', subtype => 'success', stop_reason => 'end_turn', is_error => JSON::PP::false, result => $text }), "\n";
    step_in_background() unless $internal;
  }
}
