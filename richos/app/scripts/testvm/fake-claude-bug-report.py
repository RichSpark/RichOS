#!/usr/bin/env python3
"""fake-claude-bug-report.py - the guest's `claude` for the Bust a bug walk (bug-report-walk.sh).

Rich's write-up is ONE printed turn (`richos_core::bug_report::writer_args`: `--print
--input-format stream-json --output-format stream-json --verbose`, no tools). For that call this
reads the one user message on standard input the way Claude Code does, answers with a `result`
line whose `result` is Rich's report, and records what it was given in
<dir>/writer-prompts.log: the prompt's words, and for a picture its media type, its size and its
first bytes. The picture itself is written to <dir>/rich-saw.<ext>, so the walk can show what
Rich was shown.

The report quotes the user's own words (so a file path they typed reaches the card and must be
left out there) and names the company the walk registered and a person, so the walk can see all
of them replaced by stand-ins on the card and absent from what reaches the issues endpoint.

For Request a feature (feature-request-walk.sh) it writes up an idea the same way, naming the same
company and person, and to Rich's last pass over a report or a post (`finish_prompt`) it answers
the words as the scanner left them, so the card is what the scanner made.

Every other call (the conversation, its quota reads, sign-in status) goes to the fill-first fake
(`fake-claude-fill-first.pl`, beside this file in the guest), unchanged.
"""
import base64
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
args = sys.argv[1:]
streamed = any(a == "--input-format" and i + 1 < len(args) and args[i + 1] == "stream-json" for i, a in enumerate(args))
if not streamed:
    os.execv("/usr/bin/perl", ["/usr/bin/perl", os.path.join(HERE, "claude-fill-first"), *args])

prompt, pictures = "", []
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    for block in message.get("message", {}).get("content", []):
        if block.get("type") == "text":
            prompt += block.get("text", "")
        elif block.get("type") == "image":
            source = block.get("source", {})
            data = base64.b64decode(source.get("data", ""))
            ext = {"image/jpeg": "jpg", "image/png": "png"}.get(source.get("media_type"), "bin")
            with open(os.path.join(HERE, f"rich-saw.{ext}"), "wb") as out:
                out.write(data)
            pictures.append({"media_type": source.get("media_type"), "bytes": len(data), "head": data[:4].hex()})
with open(os.path.join(HERE, "writer-prompts.log"), "a") as log:
    log.write(json.dumps({"argv": args, "prompt": prompt, "pictures": pictures}) + "\n")


# The app's prompts are one JSON event each (`writer_prompt_for`, `change_prompt_for`,
# `finish_prompt`: "event" names which); an older build's were prose, still read below.
try:
    event = json.loads(prompt)
except ValueError:
    event = None
if not isinstance(event, dict):
    event = {}


def users_words(text):
    if event.get("user_message"):
        return event["user_message"].strip()
    start = text.find("What the user said, in their own words:\n<<<\n")
    if start < 0:
        return ""
    start += len("What the user said, in their own words:\n<<<\n")
    return text[start:text.find("\n>>>", start)].strip()


def between(text, head):
    """The words between `head` + "<<<\\n" and the next "\\n>>>"."""
    start = text.find(head + "\n<<<\n")
    if start < 0:
        return ""
    start += len(head + "\n<<<\n")
    return text[start:text.find("\n>>>", start)]


idea = event.get("kind") == "feature idea" or "pressed \"Request a feature\"" in prompt
if event.get("event") == "user_requested_draft_change" or "Answer with ONLY a JSON object, no other text: {\"section\"" in prompt:
    first = "The idea" if idea or "feature idea" in prompt else "What happened"
    report = {"section": first, "add": "It happens in the light theme too.", "private": []}
elif event.get("event") == "public_draft_privacy_check":
    # Rich's last pass (`finish_prompt`): the draft as the scanner left it, kept as it is.
    report = dict(event["draft"])
    report["private"] = []
elif ", after the automatic check:" in prompt:
    # Rich's last pass (`finish_prompt`): the report or post as the scanner left it, kept as it is.
    head = "The post, after the automatic check:" if "The post, after the automatic check:" in prompt else "The report, after the automatic check:"
    report = json.loads(between(prompt, head))
    report["private"] = []
elif idea:
    # Request a feature (round 24): the idea as Rich writes it up, naming the walk's company and a
    # person so the walk can see both replaced on the card and absent from what is posted.
    report = {
        "title": "A short weekly summary of each conversation, from Rich",
        "idea": "Once a week, Rich sends a short summary of each conversation: what changed, what was decided "
        "and what is still open. The user would like one for the Northwind Traders conversations, and "
        "Dana Whitfield would read it too.\n\nIn the user's words: " + users_words(prompt),
        "why": "Catching up today means reading back through each conversation.",
        "pictured": ["A message from Rich every Friday afternoon.", "One short part per conversation."],
        "where": "",
        "checked": "Saw the conversation on the screen you were on" if "visible words top to bottom" in prompt else "",
        "private": [{"text": "Dana Whitfield", "kind": "person"}],
    }
else:
    report = {
        "title": "Conversation names in the sidebar are cut off at larger text sizes",
        "what_happened": "With Text size raised, longer conversation names in the left sidebar are cut off, "
        "so they can no longer be read. The user saw it in the Northwind Traders conversations, "
        "and Dana Whitfield noticed it too.\n\nIn the user's words: " + users_words(prompt),
        "where": "The list of conversations in the left sidebar.",
        "steps": ["Open any conversation.", "In Settings, raise Text size to 135%.", "Look at a long conversation name in the sidebar."],
        "expected": "Every conversation name can still be read at any text size.",
        "checked": "Saw the conversation on the screen you were on" if "visible words top to bottom" in prompt else "",
        "private": [{"text": "Dana Whitfield", "kind": "person"}],
    }
print(json.dumps({"type": "system", "subtype": "init"}))
print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": json.dumps(report)}))
