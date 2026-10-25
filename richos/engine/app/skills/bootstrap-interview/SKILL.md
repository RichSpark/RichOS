---
name: bootstrap-interview
description: Use when the CEO agrees to tell you about his business for the first time or asks to pick that conversation back up or asks you to ask him how he wants you to work. It carries the questions, their order and where the answers go. Do not use it to re-ask about a company you already have notes on unless he asks you to.
---

# Getting to know his business

He has agreed to spend about twenty minutes telling you what his company is. Your job is to
have that conversation, write down what he actually said and be honest about the parts you
could not do.

**This is a conversation, not a form.** The stages below are a checklist in your own head, not
a script to read out. Ask naturally, follow what he says and move on the moment a stage has
enough to work with. Short is fine.

## Before the first question

**Check what you already have.** Use the saved company material in your instructions. The COMPANY NOTES DESTINATION
names the bound company and exact absolute file under `companies/<company>/company.md`.
If you need to inspect that file, use that exact path. Never guess a central folder or use
the engine working directory as one. Three cases:

1. **It is not there or it is empty.** Start at Stage 1.
2. **It is there and thin — some stages covered, some not.** Tell him plainly what you already
   have, confirm it is still right and pick up at the first stage that is missing. Never
   re-ask something the file already answers and never quietly assume a note from last week
   is still true.
3. **It is there and full.** You have nothing to do about the company. Say so and offer to
   change or add to it instead. If he has not yet told you how he wants you to work, ask that
   (Stage 7).

**When he asks only how he wants you to work** (for example he pressed "Have Rich ask me" on his
instructions), go straight to Stage 7 and skip the company stages.

**Explain these choices briefly in your own words.**

- Stopping partway is fine and so is "not sure yet." Both are real answers. You will write
  down what he told you and leave the rest visibly open.
- Explain that requested expertise can be added to the team after he approves the proposal.
  A proposal is distinct from a teammate actually saved and available.

## The stages

### 1 — What the business is
What is it, in a sentence or two? Who is it for? What does it promise them? Does it have a
name yet or is that still open?

### 2 — Who uses what
Is there more than one thing people use — an app, a website, an admin view, more than one
product sharing a back end? If so, which people use which. If there is only one, say so
plainly rather than leaving it blank; "one product, one audience" is an answer.

### 3 — How the work gets made
What it is built with, where it runs, who builds it today. Ask at the level he can answer
without reading code. If he does not know a detail, that is a detail for someone else and not
a gap in the interview.

### 4 — Who he wants around him
Who does he wish he had? An architect, engineers, someone on quality, someone on marketing, an
advisor who knows the field. Do not read him a catalog — infer the likely shape from stages 1
to 3 and check it with him: "it sounds like you would want someone on the back end, someone on
the front and someone testing — anything else or does that cover it?"

Write down who he described. When the interview ends you propose the team (see "When you are
done"): the people he described plus anyone else the business clearly calls for from what he
told you. Choosing a useful proposal is your job; the unresolved decisions remain his.

### 5 — What must never happen
The rules that are not negotiable — how customer information is handled, what the product must
never do, anything about accessibility, brand or compliance that he already knows he cares
about. If nothing comes to mind, that is a valid answer; leave it open.

### 6 — What he wants to decide himself
What is genuinely his call and what is safe for you to decide on your own? Money above some
amount, anything that cannot be undone, anything involving someone outside the company. This
does not have to be exhaustive — you are capturing his examples, not writing a policy.

### 7 — How he wants you to work
After the company questions, ask about him rather than the company: how does he want you to
work, wherever you are? Anything you should always do or never do: spending, emails, how he
likes reports, when not to disturb him. Ask it in your own words, once and let him answer
freely. If he has nothing to add, that is an answer; save nothing.

These are not company notes. Save them with `mcp__richos_onboarding__save_your_instructions`:
`rules`, one short rule per item, each in his own words. They are added at the end of his
instructions file, after everything already in it and he sees them as a card under your reply.
Never use Write, Edit or a shell for that file. After it returns `status: saved`, tell him in a
sentence that you wrote them down as his instructions and that you follow them in every company,
starting with his next new conversation.

## Writing it down

**One destination** for stages 1 to 6: the exact company file in COMPANY NOTES DESTINATION.
Stage 7 goes to his instructions, as above.
**One way to save:** call `mcp__richos_onboarding__save_company_notes` with `notes` and
`progress`. The app chooses the company and path. You never supply either as a tool argument.
The app creates the destination when needed, checks the byte limit, writes atomically and
reads the file back before returning `status: saved`. Do not use Write, Edit or a shell to
save interview answers. If the tool is unavailable or refuses, say the notes have not been
saved and help resolve that; never claim persistence based on a proposed write.

Nothing else. You do not edit a `CLAUDE.md`, you do not edit any configuration file and you do
not run any setup or verification script. Those belong to a different
copy of this system, run by somebody sitting at a terminal and doing them here would change
files nothing in this app ever reads. If you find yourself about to write outside that one
file, stop and ask him instead. Team changes are a separate request, interpreted in context (see "When you are done").

**Keep answers safe as you go.** His acceptance of the interview includes writing down
his answers. Tell him in the first minute that you will keep notes as you go. After each
substantive answer, save a partial checkpoint before asking the next question. Use his actual
answer; ask him to confirm a summary only when its meaning is uncertain or disputed.
Include all previously saved answers in every replacement. Never discard an earlier answer
because you have moved on to a later stage.

**How to write it.**

- His words where you have them. Your paraphrase is a summary of what he said, never an
  inference about what he meant and never a fact he did not give you.
- One short section per stage that has an answer, headed so he can find it again.
- **Every stage that has no answer gets a line saying so** — "not discussed yet" or "he said he
  is not sure yet." An empty heading reads as a question nobody asked. Deferral is honest and
  silence is not.
- **Date it and say it came from a conversation with him on that date.** The file has no other
  source. Something in it will go out of date and nothing will announce that, so it has to
  carry when it was true.
- **Keep it short — comfortably under eight thousand UTF-8 bytes, not characters.** The app
  enforces 8192 bytes including its progress header. Accents and other non-ASCII characters
  can occupy multiple bytes. A size refusal leaves the previous saved answers intact: shorten
  the notes without inventing or dropping a decision, then retry the same save tool.
- Use `progress: "partial"` while any of stages 1 to 6 are still unasked. Use `progress: "complete"`
  only once each of them has an answer or an explicit deferral from him. The app records this
  progress with the notes, so he can resume after closing the application.

## When you are done

First save with the correct progress and wait for the tool's verified success. If it returns
a warning about the reminder, say what was saved and what still needs fixing separately.
Then tell him three things, in plain words, in this order:

1. **What you wrote down.** A sentence or two, not a recital of the file.
2. **What is still open.** Every stage he deferred, named. Not buried, not softened.
3. **The team.** Explain the expertise the business needs and your proposed additions in your
   own words. Preserve names and roles he explicitly requested. Establish who is already
   available from the current team evidence; `richos_work.team` lists and shows the shelf
   and saved definitions. Use the actual record, not a duplicate catalog or a stock roster.
   Distinguish existing teammates from proposed additions. Save the proposal in the notes so
   it survives the conversation, then ask for any genuinely unresolved decision.

Interpret his approval or correction in context. No particular word is required. Apply the
ordinary latency-based handoff rule: Rich decides what to handle and what to pass to Stu.
A handoff carries the entire approved request, every proposed name and role, relevant business
facts and established constraints. The handoff is preserved in full; the display title is
not the execution contract. Do not split a request to fit that title. Acknowledge in your own
words. Stu chooses the work sequence and reports actual outcomes to Rich.

If he changes the proposal, preserve that change. If he defers it, keep the proposal in the
notes without starting the deferred work.

**Name someone as hired only after the hiring has reported them saved.** Until he says go the
list is a proposal and a proposal is never described as a team that exists.

## If he stops partway

Save what you have so far with `progress: "partial"`, with the covered stages filled in and
the rest marked as not discussed. Tell him where you stopped and that picking it up later costs him
nothing. If Stage 4 is behind you, end with the team, as in point 3 above. Do not hold the
answers in the conversation hoping to finish later — a conversation ends and a file does not.

## If he declines the interview or its resumption

If he explicitly says "not now" to this interview offer, call
`mcp__richos_onboarding__decline_onboarding` with no arguments. Do this only for an answer to
the interview offer, never for an unrelated "not now" in ordinary work. Wait for confirmation
before saying the reminder is off. His existing notes remain saved and usable. He can ask to
resume any time; the next successful checkpoint clears the old decline for this company.
