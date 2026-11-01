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
# HELPER STEPS. While the agents file lists helpers, each user turn (and each second of a held
# turn) is one step of every helper that has not ended, as Claude Code runs one tool call: the
# fake runs the PreToolUse gate command from the lease's <plugin-dir>/hooks/hooks.json, with a
# helper payload (agent_id walk-agent-N) on stdin and the lease's own environment; then the tool
# itself unless that was refused (a stand-in Read of this script, which the fake performs:
# calls.log "ran <agent> Read <bytes>B"; a Read, because the engine's sealed-worktree guard lets
# a read-only tool through for a helper with no worker registration and refuses any other one,
# "Lock-out: REFUSED (no registration)". A helper this fake launched has no worker receipt either,
# so the engine's hook refuses its Read too, "worker identity has not joined its app receipt":
# the engine's answer, never the quota gate's, given only once the gate let the call through);
# then, refused or not, the PostToolBatch command when hooks.json has one ("batch-start
# <agent>", then "batch <agent> exit <n>" once it returns), which Claude Code runs after every
# call of a batch has resolved and before its next model request. A held step waits as long as either command holds
# it; a refused step ends nothing. Each gate call is a line in calls.log ("gate <agent> exit
# <n>", with the gate's first words when it said any; the batch's with its additionalContext).
# While /Users/admin/fill-first/thinking exists no helper takes a step: each one's model call is # machine-path-exempt: isolated guest fixture documentation; no customer default
# in flight, so a walk can set a hold up before a helper's next tool call comes.
# Without helpers, or without a hooks.json, no step runs and nothing here changes.
#
# THE BACK END'S JOB. A helper of Stu's runs only for a job the front desk wrote down; helpers
# on the front desk's own lease are not his. Two files make a job with a helper on the back end,
# with no model:
#   register      while it exists, the front desk's user turn whose text carries the file's
#                 text (a lease whose --mcp-config has richos_assignments; never the app's own
#                 priming turn) calls that server's `record` tool with the file's text as the
#                 assignment, as Claude Code would, and says the receipt's words. The
#                 file is taken (renamed) first, so the job is written down once. calls.log:
#                 "register ok <text>" or "register error <text>". A file of several lines
#                 writes one job per line, in order, in that one turn (the turn's text carries
#                 the first line); a one-line file is exactly the single job above. Each job
#                 written down is then handed to Stu with the `richos_status` server's
#                 `delegate` tool, as Rich hands work over (a job only written down waits:
#                 "Preparation has not started."). calls.log: "delegate ok|error <text>".
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
#                 lease whose --mcp-config has richos_work and not richos_assignments), as
#                 work-agent-N; once per walk
#                 (work-agents-started). A background helper keeps running after the back end's
#                 turn ends, so after that turn a stepper child steps every open work-agent
#                 every 2 s (the same gate step as above) until each has ended, for at most 10
#                 minutes, and ends with this process.
#   work-agents-finish  while it exists, a work-agent's next step the app's QUOTA gate admits is
#                 its last: it finishes on its own, its SubagentStop row is written and calls.log
#                 says "gate work-agent-N exit <n> finished" (after the step's batch line).
#                 Admitted is a PreToolUse exit 0, or a refusal by the engine's hook behind the
#                 gate (`--claude-quota-gate` runs it only once it has admitted the call); and a
#                 batch that came back with no gate refusal in its context. A step the quota
#                 gate holds never returns, and one it refuses says so first ("RichOS desktop
#                 quota: ..." or "RichOS desktop: This app work has ended"): neither finishes it
#                 (weekly switch, the CEO's §153: a helper on the account being left carries on
#                 to its own end).
#
# THE TASK FRAMES. The app counts Stu's running helpers from his stream (agent_tasks.rs TaskBook),
# not from the journal: Claude Code prints the Agent tool call, its async_launched answer and a
# system task_started (task_type local_agent) when a background agent starts, and a system
# task_notification (status completed) when it ends, followed between turns by the automatic
# turn it runs for it. So does this fake for each work-agent; without them the app saw nothing of
# Stu's running and moved him under a running helper (accounts-walk 5d).
#
# THE INPUT HOOK. Claude Code calls the app's UserPromptSubmit hook (`richos-input-context`, a
# control_request hook_callback carrying the input's exact text) when it takes an input in, and
# the app writes that turn's tool scopes then, the register's among them (native.rs
# prepare_work_turn, run at that hook). So does this fake, for every user turn, and it waits for
# the app's control_response before the turn goes on; lines that arrive meanwhile are kept and
# read after. Without it the register refused every job ("RichOS has not opened this
# conversation for assignments.", accounts-walk 5d's first run).
# Nothing here reaches a network.
use strict; use warnings; use JSON::PP; use Fcntl qw(:flock O_CREAT O_EXCL O_WRONLY); use File::Path qw(make_path);
use IPC::Open2 qw(open2); use POSIX qw(WNOHANG); use Cwd qw(getcwd);
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
# A resumed lease is the same session as the one it resumes. The app resumes with the path of the
# session's native transcript (`--resume <...>/<session>.jsonl`, native.rs native_resume), on
# another account too; a bare session id is taken as it is.
my $session = arg_after('--session-id') // arg_after('--resume');
if (defined $session) { $session =~ s{\A.*/}{}; $session =~ s{\.jsonl\z}{}; }
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
# THE NATIVE TRANSCRIPT. Claude Code keeps each session's record at
# <config folder>/projects/<project>/<session>.jsonl and names it in SessionStart's
# transcript_path; the app finds it there (session_tools.rs native_path) and carries it to the next
# account's folder before it resumes the session there (native.rs native_resume). Without it the
# app could not move a back end at all ("The provider has not recorded this session's native
# transcript path", accounts-walk 5d). So this fake keeps an empty one, created if missing.
my $transcript;
if ($evidence) {
  # Account 1 without a folder named is $HOME/.claude, as Claude Code's default.
  my $config = length $folder ? $folder : (($ENV{HOME} // (getpwuid($<))[7]) . '/.claude');
  my $project = "$config/projects/-fill-first";
  make_path($project);
  $transcript = "$project/$session.jsonl";
  if (open(my $t, '>>', $transcript)) { close $t; }
}
journal({ hook_event_name => 'SessionStart', ($transcript ? (transcript_path => $transcript) : ()) });
my %ended;
sub gate_command {  # the first command hooks.json registers for an event (PreToolUse when none is named), or undef
  my ($event) = @_; $event //= 'PreToolUse';
  return undef unless $profile && open(my $fh, '<', "$profile/hooks/hooks.json");
  local $/; my $text = <$fh>; close $fh;
  my $m = eval { $json->decode($text) } or return undef;
  return $m->{hooks}{$event}[0]{hooks}[0]{command};
}
sub gate_run {  # one gate command run with a payload on stdin: (exit code, first stderr line, stdout)
  my ($command, $id, $payload) = @_;
  my $in = "$dir/gate-in-$$-$id.json"; my $err = "$dir/gate-err-$$-$id.txt"; my $out = "$dir/gate-out-$$-$id.txt";
  open(my $w, '>', $in) or return (1, '', ''); print $w $json->encode($payload); close $w;
  system("($command) < '$in' > '$out' 2> '$err'");
  my $code = $? >> 8;
  my ($said, $answer) = ('', '');
  if (open(my $e, '<', $err)) { local $/; $said = <$e> // ''; close $e; }
  if (open(my $o, '<', $out)) { local $/; $answer = <$o> // ''; close $o; }
  unlink $in, $err, $out;
  my ($first) = split /\n/, $said; $first //= '';
  return ($code, $first, $answer);
}
my $steps = 0;
my $between_turns = 0;  # set in the background stepper: its frames arrive between the back end's turns
sub step_one {  # one tool call of one helper (HELPER STEPS); true when that step was its last (it finished)
  my ($command, $id, $type) = @_;
  # A tool_use_id, a prompt_id and the cwd, as every Claude Code tool call's hook payload has: the
  # engine's evidence hook refuses a call without them ("Tool callback has no tool identity",
  # "Callback has no prompt identity").
  my %call = (tool_name => 'Read', tool_input => { file_path => $0 }, tool_use_id => 'toolu_fake_' . $id . '_' . (++$steps));
  my ($code, $first) = gate_run($command, $id, { hook_event_name => 'PreToolUse', session_id => $session, agent_id => $id,
    agent_type => $type, %call, prompt_id => "fake-prompt-$session", cwd => getcwd() });
  # A refusal carries the gate's first words: an exit 2 alone does not say why.
  my $quota_refused = $code != 0 && $first =~ /\ARichOS desktop(?: quota)?: /;
  calls_line('gate', $id, 'exit', $code, length $first ? ('said:', substr($first, 0, 200)) : ());
  if (!$code) {  # the tool itself: the Read, done
    my $bytes = -1; if (open(my $r, '<', $0)) { local $/; $bytes = length(<$r> // ''); close $r; }
    calls_line('ran', $id, 'Read', "${bytes}B");
  }
  # Its batch, refused or not: after the call resolved and before the helper's next model request.
  my $batch_ok = 1;
  if (my $batch = gate_command('PostToolBatch')) {
    calls_line('batch-start', $id);
    my ($bcode, $bfirst, $answer) = gate_run($batch, $id, { hook_event_name => 'PostToolBatch', session_id => $session,
      agent_id => $id, agent_type => $type, prompt_id => "fake-prompt-$session", cwd => getcwd(),
      tool_calls => [ { %call, tool_response => ($code ? $first : '') } ] });
    my $context = ((eval { $json->decode($answer) } || {})->{hookSpecificOutput} || {})->{additionalContext} // '';
    $batch_ok = !$bcode && $context !~ /\ARichOS desktop(?: quota)?: /;
    calls_line('batch', $id, 'exit', $bcode, length $context ? ('said:', substr($context, 0, 200)) : (),
      length $bfirst ? ('err:', substr($bfirst, 0, 200)) : ());
  }
  my $finished = !$quota_refused && $batch_ok && $id =~ /\Awork-agent-\d+\z/ && -e "$dir/work-agents-finish";
  calls_line('gate', $id, 'exit', $code, 'finished') if $finished;
  return 0 unless $finished;
  journal({ hook_event_name => 'SubagentStop', agent_id => $id, agent_type => $type });
  # Its end on the lease's stream (THE TASK FRAMES): task_notification, and between the back
  # end's turns the automatic turn Claude Code runs for it, which ends with a result.
  if ($id =~ /\Awork-agent-(\d+)\z/) {
    print $json->encode({ type => 'system', subtype => 'task_notification', task_id => "task-work-agent-$1", status => 'completed',
      session_id => $session }), "\n";
    if ($between_turns) {
      print $json->encode({ type => 'assistant', message => { role => 'assistant', content => [ { type => 'text', text => "$type finished." } ] } }), "\n";
      print $json->encode({ type => 'result', subtype => 'success', stop_reason => 'end_turn', is_error => JSON::PP::false, result => "$type finished." }), "\n";
    }
  }
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
  return if -e "$dir/thinking";  # HELPER STEPS: every helper's model call is in flight
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
# register (richos_assignments), and Stu's back end the work tools (richos_work) without the
# register. Rich's conversation lease carries richos_work too now (native.rs mcp_config), so the
# work tools alone do not make a lease the back end; they are still declared wherever they are.
my $servers = eval { $json->decode(arg_after('--mcp-config') // '{}')->{mcpServers} } || {};
my $work_tools = exists $servers->{richos_work};
my $backend = $work_tools && !exists $servers->{richos_assignments};
sub calls_line { if (open(my $l, '>>', "$dir/calls.log")) { print $l join(' ', time(), ($folder || 'account-1'), @_), "\n"; close $l; } }
# One line naming a lease and its kind, so a walk reads which account each one opened on without
# parsing the argument line above (it runs over several lines): "lease back-end <session>",
# "lease front-desk <session>" or "lease other <session>".
calls_line('lease', $backend ? 'back-end' : exists $servers->{richos_assignments} ? 'front-desk' : 'other', $session) if $evidence;
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
  my ($answered, $recorded, @words) = (0, 0);
  while (my $line = <$from>) {
    my $reply = eval { $json->decode($line) } or next;
    my $rid = $reply->{id} // ''; next unless $rid =~ /\A\d+\z/ && $rid >= 2;
    my ($ok, $result) = (0, $reply->{result} || {});
    my $said = $result->{content} && $result->{content}[0] ? $result->{content}[0]{text} // '' : ($reply->{error}{message} // '');
    if ($result->{content} && !$result->{isError}) {
      my $receipt = eval { $json->decode($said) } || {};
      $ok = $receipt->{recorded} ? 1 : 0; $recorded += $ok; push @words, $receipt->{say} if defined $receipt->{say};
    }
    calls_line('register', $ok ? 'ok' : 'error', ($said =~ s/\s+/ /gr));
    last if ++$answered == @jobs;
  }
  close $from; waitpid($pid, 0);
  delegate_jobs(@jobs) if $recorded;
  return @words ? join(' ', @words) : undef;
}
sub delegate_jobs {  # each job handed to Stu through richos_status `delegate`, as Rich does
  my @jobs = @_;
  my $server = $servers->{richos_status} or do { calls_line('delegate', 'error', 'no richos_status server'); return };
  my ($from, $to);
  my $pid = eval { open2($from, $to, $server->{command}, @{ $server->{args} || [] }) };
  unless ($pid) { calls_line('delegate', 'error', 'the status server could not be started'); return; }
  my $n = 1;
  print $to $json->encode($_), "\n" for
    { jsonrpc => '2.0', id => 1, method => 'initialize',
      params => { protocolVersion => '2025-06-18', capabilities => {}, clientInfo => { name => 'fake-claude', version => '1' } } },
    { jsonrpc => '2.0', method => 'notifications/initialized' },
    map { $n++; { jsonrpc => '2.0', id => $n, method => 'tools/call', params => { name => 'delegate', arguments => { text => $_ } } } } @jobs;
  close $to;
  my $answered = 0;
  while (my $line = <$from>) {
    my $reply = eval { $json->decode($line) } or next;
    my $rid = $reply->{id} // ''; next unless $rid =~ /\A\d+\z/ && $rid >= 2;
    my $result = $reply->{result} || {};
    my $said = $result->{content} && $result->{content}[0] ? $result->{content}[0]{text} // '' : ($reply->{error}{message} // '');
    calls_line('delegate', ($result->{content} && !$result->{isError}) ? 'ok' : 'error', ($said =~ s/\s+/ /gr));
    last if ++$answered == @jobs;
  }
  close $from; waitpid($pid, 0);
}
my $stepper;  # the pid of this lease's background stepper, while it runs
sub step_in_background {  # the work-agents keep running after the back end's turn ends
  return unless $backend && open_work_helpers();
  return if $stepper && waitpid($stepper, WNOHANG) == 0;
  my $parent = $$;
  my $pid = fork();
  return unless defined $pid;
  if ($pid) { $stepper = $pid; return; }
  # STDOUT stays the lease's: a helper's end is a frame on it (THE TASK FRAMES). Each frame is
  # one line written whole, so it never interleaves with the lease's own.
  open(STDIN, '<', '/dev/null');
  $between_turns = 1;
  for (1 .. 300) {
    sleep 2;
    POSIX::_exit(0) if getppid() != $parent;
    helper_steps();
    POSIX::_exit(0) unless open_work_helpers();
  }
  POSIX::_exit(0);
}
my @queued;  # lines read while waiting for the app's answer to the input hook, read next
my $hooks = 0;
sub next_line { return @queued ? shift @queued : scalar <STDIN>; }
sub input_hook {  # THE INPUT HOOK (header): the app's answer is awaited, other lines kept
  my ($text) = @_;
  my $id = 'fake-input-' . ++$hooks . "-$$";
  print $json->encode({ type => 'control_request', request_id => $id, request => { subtype => 'hook_callback',
    callback_id => 'richos-input-context', input => { hook_event_name => 'UserPromptSubmit', prompt => $text } } }), "\n";
  while (defined(my $l = <STDIN>)) {
    my $m = eval { $json->decode($l) };
    return if $m && ($m->{type} // '') eq 'control_response' && (($m->{response} // {})->{request_id} // '') eq $id;
    push @queued, $l;
  }
}
sub iso { my @t = gmtime(shift); sprintf('%04d-%02d-%02dT%02d:%02d:%02dZ', $t[5]+1900, $t[4]+1, $t[3], $t[2], $t[1], $t[0]) }
while (defined(my $line = next_line())) {
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
    my @work = $work_tools ? map { "mcp__richos_work__$_" } qw(repositories prepare inspect integrate complete) : ();
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
    input_hook($sent);
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
        # THE TASK FRAMES (header): the Agent call, its immediate answer and task_started, on
        # this turn's own stream, as Claude Code prints them for a background agent.
        $n = 0;
        for my $type (@who) {
          $n++; my $tool = "toolu_agent_work-agent-$n";
          print $json->encode({ type => 'assistant', message => { role => 'assistant', content => [ { type => 'tool_use', id => $tool,
            name => 'Agent', input => { name => $type, description => "work-agent-$n", run_in_background => JSON::PP::true } } ] } }), "\n";
          print $json->encode({ type => 'user', message => { role => 'user', content => [ { type => 'tool_result', tool_use_id => $tool,
            content => "async_launched work-agent-$n" } ] } }), "\n";
          print $json->encode({ type => 'system', subtype => 'task_started', task_id => "task-work-agent-$n", tool_use_id => $tool,
            task_type => 'local_agent', session_id => $session }), "\n";
        }
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
