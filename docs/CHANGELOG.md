# Changelog

What changed in each stable version of RichOS, newest first; nightly builds are in the [nightly changelog](CHANGELOG-NIGHTLY.md).

## 1.2.5

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5)

- Added a live square at the foot of the sidebar, opposite your name, showing how busy your Mac's processor and memory are; point at it or click it for the last minute in detail and right-click it to choose Readout, which also shows both as percentages
- Added RichCleaner (Settings, RichCleaner), which works with RichScribe: select a file on your Desktop, tap your RichScribe key and say what it is; RichOS files it in a fitting folder inside one you choose and keeps your note for you or Rich to find it by later; Recent files lets you search, change a note or undo a filing
- Added Rich asking you before installing a tool your team's work needs and your Mac lacks, such as Apple's command line developer tools, instead of macOS popping up its own request to install them: yes installs it once and the work carries on, no is remembered and telling him to install whatever is needed lets him do so from then on without asking
- Changed talking to Rich from your phone to go only through the RichConnect app: the phone web page is gone, its address now says to use the app and a copy installed on a phone removes itself the next time it reaches your Mac
- Changed the paired phone's card (Settings, Use Rich from your phone): it shows Connected or Reconnecting beside the phone's name and whether it can send you notifications, with what to switch on in RichConnect if not; a phone paired through the old web page is shown as needing to be paired again
- Changed a conversation you left more than 30 minutes ago to stop running in the background when nothing is going on in it; your next message there picks up where you left off, though Rich's first reply takes a little longer
- Changed how your team's finished work is reviewed before it is merged: RichOS now reviews it once by itself, with Codex when "Let Codex review your team's work" is on and with Claude otherwise, instead of Stu starting a separate reviewer
- Changed your team so it can no longer keep your Mac awake or change its sleep settings
- Changed switching to another Claude account, at the weekly or the five-hour limit, so it no longer stops teammates in the middle of their work: they finish on the old account and Stu moves once they are done
- Changed the pause near a Claude usage limit so a teammate's commands still run and only its next request to Claude waits
- Changed how work held at your five-hour Claude limit comes back, whether you chose Pause or Switch: it resumes one agent at a time from 20 minutes before the reset and only while the account stays under 100%, instead of all at once or only at the reset; the rest wait for the reset
- Changed Settings, Claude accounts for an account with no five-hour limit: it shows only the weekly limit and your team is no longer paused for a five-hour limit that account doesn't have
- Changed Rich so he no longer suggests switching Claude accounts just because another account's limit has reset; RichOS pauses or switches by itself when the account in use reaches its limit
- Changed the setup screen to ask for an Ultra subscription on your Anthropic account instead of Max
- Fixed your team being unable to check or save work in a folder where a file or folder name has an accent or other non-English letter, such as café.txt
- Fixed RichOS asking you to set up its engine again when it was already installed and working
- Fixed your team's browser work opening browser windows on your screen or making macOS ask you to allow Chrome's notifications; it now runs out of sight
- Fixed teammates waiting on a long command using a large share of your Mac's processor; a waiting teammate now uses almost none
- Fixed RichDoctor's window popping up over RichOS the first time RichOS starts; it now starts quietly in the menu bar
- Fixed Settings, Updates holding back an update and the question before Quit saying work was still running in the background after all of it had ended, such as when a job record was damaged or Rich had started a helper before his conversation closed; such a job now goes back to Stu at the next start
- Fixed Rich being unable to carry on when a command he had left running finished after his reply; he now continues in the conversation, where you see what he says and can press Stop
- Fixed Settings, Claude accounts showing a switch before it had happened: the account the work still runs on stays first and in use, the screen says "Switching to" the new one or "Can't move yet" while work still runs on the old one and says "Rich now uses" the new one only once the move is real
- Fixed RichOS moving your team off a Claude account with most of its week left and holding their work after one large burst of use was reported all at once or after use of a model that account had seldom used, such as Haiku
- Fixed Stu putting work off because he read another Claude account's weekly use: when he checks usage he now gets the account the work runs on, by name and only its figures
- Fixed your team staying paused for up to five minutes after a Claude usage limit reset, with Settings, Claude accounts still showing the old figures; both now catch up within seconds
- Fixed your team staying on a Claude account after its weekly limit for one model, such as Fable, ran out, with automatic reviews still starting there; RichOS now treats that like the account's own weekly limit
- Fixed Stu's work stopping when one of his steps hit a Claude usage limit: the step is tried once more, on another account with room or, when none has room, after the limit resets; the conversation says when
- Fixed Stu waiting for hours with nothing said when he started a teammate while your Claude usage was on hold; he is now told at once which account is held, why and until when
- Fixed Stu being handed a job he already had or a message he had already acted on again as if it were new, sometimes several times in a few minutes; after a restart an open job comes back to him once, marked as one he was already given
- Fixed jobs stalling until you sent a message: Stu could not start a teammate he had lined up in an earlier reply or one he had lined up when a new message or a finishing teammate woke him
- Fixed a finished job staying open because a teammate Stu had lined up for it never started; such a teammate is now cleared away and any work in its folder is kept
- Fixed Stu being told that a teammate had gone quiet when it had never started, was still working or was waiting on purpose for a usage hold or a time he had set; he is also no longer told about a waiting teammate from another conversation
- [technical] Added Codex's notes reaching Stu by themselves: each new entry Codex writes in one of your Codex channel folders comes to him in the conversation you have open and Rich and Stu can write back in that folder
- [technical] Added dedicated connections between Codex and your RichOS conversations: each Codex task gets its own, messages from Codex go to Stu and you can ask Rich to make a conversation the one new Codex tasks connect to
- [technical] Added the engine's safety checks to your team's work in the app: a file holding a password or access key is refused before it is written; private material, names on your list and your Mac's own memory figures stay out of a repository marked public; British spellings become American where your settings ask for American spelling
- [technical] Added Rich, Stu and your teammates being able to read your Mac's current processor and memory use, with the last minute of history, when it matters to their work
- [technical] Added ways for Stu to clear a teammate he lined up and will not start and to mark a teammate's earlier attempt as replaced by redone work, so the job can close instead of staying open
- [technical] Changed your teammates' and Stu's ordinary commands to run and return their result directly, as in Terminal, instead of being moved to the background; a teammate that is paused still gets the word at once
- [technical] Changed Rich's and Stu's work to go through the same checks as in Terminal before a teammate starts and at the end of each turn; a refused teammate start now says what to fix
- [technical] Improved Download update and Go back in Settings, Updates: they now download from RichOS's new GitHub address first and the old one second, so an update found before the move no longer relies on GitHub forwarding the old address
- [technical] Improved how much of your Claude usage your team spends: notes about your company and ongoing work and each teammate's standing notices now go once per conversation and again only after it is compacted or when they change; Stu's failed turns on RichOS's own reminders no longer wake Rich; Stu no longer gets a teammate's whole brief back each time he prepares or looks up a job
- [technical] Improved what Stu is told when RichOS refuses to start a teammate: the reason now names the job and its cause, such as who stopped it and when or the earlier request it repeats
- [technical] Fixed your team's commands running RichOS's own Python, Node and Git instead of yours, so your own Python packages were missing; RichOS's copies are still used where your Mac has only Apple's placeholders
- [technical] Fixed your team's commands slowly taking memory from your Mac that only a restart gave back: the Python, Git and Node that RichOS provides now start directly instead of through small scripts that set off a macOS memory leak
- [technical] Fixed commits by your team carrying a co-author line after you had turned that off in Claude Code's settings
- [technical] Fixed a teammate's work being paused or let through on the readings of the Claude account in use instead of the one it runs on during a switch and its usage still being counted to its old account after it moved
- [technical] Fixed the automatic Claude review of your team's work starting while their Claude usage was on hold; it now waits until the account it runs on has room
- [technical] Fixed Stu's ordinary messages to a teammate being refused when they mentioned a pause or holding something, such as "the pause button is misaligned"
- [technical] Fixed Stu's request to start a teammate being refused without a clear reason when RichOS's checks before the start took longer than 20 seconds
- [technical] Fixed a command Stu started with nohup not counting as running work, so an update or Quit did not wait for it and Stop did not end it
- [technical] Fixed pausing one teammate also freezing a shared build cache server, which hung other teammates' builds
- [technical] Fixed Stu's landing of finished work onto a release branch being refused when the repository's main folder was on another branch
- [technical] Fixed your team being refused in a repository protected by a land fence while it ran on an added Claude account
- [technical] Fixed finished teammates' folders and branches being left in your repositories when the conversation that started them had ended for good; the next check now cleans them up
- [technical] Fixed the cleanup of leftover files deleting a live conversation's working folder when its session had started on the 1st to the 9th of a month and reporting a folder that was already gone as a failed cleanup
- [technical] Fixed RichOS writing a second, ever-growing copy of each conversation's record on your Mac, which could reach hundreds of megabytes
- [technical] Fixed a Claude Code session in Terminal that uses RichOS's engine stopping and deleting work your team had in progress in the app on the same Mac
- [technical] Fixed the scheduled cleanup run by a RichOS engine installed from Terminal treating a live app conversation's own folder as leftover and telling every open Stu conversation about the same problem again and again; now one conversation is told and again only when the problem changes

## 1.2.4

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.4)

- Fixed the new GitHub address RichOS moves to: it is github.com/RichSpark/RichOS, not github.com/RichCore/RichOS as version 1.2.3 said, so updates, bug reports and ideas, including from RichDoctor, keep working through the move with nothing for you to do

## 1.2.3

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.3)

- Added file cards in the conversation: when Rich sends you a file from your Mac, it shows under his reply with its type, size and a PDF's page count; click it to open it in its usual app or use Show in Finder and a file since moved or deleted says so and offers Ask Rich again
- Added the same file cards on the phone page: tap one to bring the file over from your Mac and open it or save it when your phone cannot show it; files over 100 MB open only on the Mac
- Added Your instructions in Settings: write the rules Rich follows in every company or press Have Rich ask me and he asks you a few questions and writes them down; they apply from your next new conversation and rules he saves show as a Saved card under his reply
- Added Terminal setup in Settings: see the tool folders and settings RichOS read from your Terminal and press Read it again after you change them; it shows what changed and your team uses it from its next job
- Added a question the first time your team saves work in one of your folders and your Git setup has no name for you: type the name and email to put on your team's work or choose Use "RichOS" instead; only that save waits and you can change it later in Settings, Your name in Git
- Changed what Rich does with work left on hold or finished but not merged into your project: instead of asking you after 30 minutes whether to go ahead, he goes ahead, merges or drops it himself and asks you only about a decision that is truly yours
- Changed RichOS to keep finding its updates and sending your bug reports and ideas, including from RichDoctor, when it moves to its new GitHub address, github.com/RichCore/RichOS; there is nothing for you to do
- Improved Stu's reminder about a teammate's finished work that is not yet merged: it now comes as soon as he is free and every 30 minutes until the work is handled, instead of after about 12 hours
- Fixed Bust a bug and Request a feature never sending: an approved bug report now reaches RichOS on GitHub and an idea is posted under Ideas, with nothing to set up on your Mac
- Fixed messages sent close together, such as one typed and one spoken or sent from your phone, sometimes reaching Rich out of order
- Fixed a job sitting still after Stu added a new teammate for it; the new teammate now picks it up
- Fixed Rich saying he had saved or written something down when he had not: he is now told as he finishes and either saves it or tells you it was not saved
- [technical] Changed jobs that change only a project's changelog files so they get no automatic review while they run
- [technical] Changed the ready-made release notes writer on the teammate shelf to run on Opus instead of Sonnet
- [technical] Improved the check that reads each job before a teammate starts it: it now reads the whole job first and no longer turns a job back over a contradiction it misread or a setting it could not see
- [technical] Fixed RichOS leaving temporary files on your Mac after use, a Quit or a crash; it now removes them when it starts, when your team's jobs are done and when you quit
- [technical] Fixed Rich and Stu being told to run commands the app does not have when work was waiting on them; they are now told the app's own steps to resume, merge or continue it
- [technical] Fixed Stu being turned back at the end of a turn and writing extra replies after he had correctly closed a finished job or added a teammate for it
- [technical] Fixed a teammate's ordinary command being refused as a hidden shell command when a folder name in it started with a dash, as Claude's own temporary folders do

## 1.2.2

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.2)

- Added RichDoctor, a small menu bar app that RichOS installs beside itself for reporting a RichOS problem even when RichOS will not open: describe what went wrong, add screenshots and read the exact report before you choose to send it
- Added Rich's and Stu's instructions and your own standing instructions as files in your RichOS folder: edit them and both follow the changes without a restart; Rich saves a "from now on" order there for you
- Added asking you before a later RichOS version updates Rich's or Stu's instructions that you changed, with a before and after that marks every edit; a copy you never changed is updated by itself
- Added connecting one project folder to more than one company in Connected project folders: RichOS tells you before you connect it and each folder shows which other companies have it
- Added the full set of skills RichOS ships to Rich, Stu and their teammates, such as copywriting, front-end design, browser testing and iPhone and Android app development
- Added memory for Rich and Stu, so what they learn carries into later conversations
- Added Rich telling you in his reply when work has been on hold for 30 minutes with nothing set to end it; he asks whether to go ahead or keep holding it until a date or event
- Changed the commands Rich and the team run to use your Mac's own setup: the tools and settings your Terminal has, plus your own Git name and saved Git sign-in for commits and pushes
- Changed merging finished work into your project to run the project's own merge checks, as a merge in Terminal does; a failed check merges nothing and its own words say why
- Changed how Rich and Stu work for you: Rich answers exactly what you asked, checks a result before saying it is done and never asks you to run a command; Stu has a new screen mocked up for you to choose before it is built
- Improved the work teammates do in your project folders: they follow each project's own instruction file, as they would in Terminal
- Improved Talk to Rich when it misses what you said: it asks you to say it again; if Rich was talking over you, it waits until he stops and keeps even a one-word reply
- Fixed Rename in a chat's menu doing nothing: the chat's own row now turns into a text field that saves with Enter and cancels with Esc; the sidebar, the chat header and the company overview all show the new title
- Fixed the first message typed right after reopening RichOS vanishing; messages sent before the conversation opens now reach Rich in the order you sent them
- Fixed Rich, Stu and their teammates stopping for good when the internet connection dropped; they now wait for it to come back and carry on where they stopped
- Fixed messages failing over and over with "This usually fixes itself" when no Claude account was signed in on your Mac or its sign-in had expired; RichOS now sends you to Settings, Claude accounts to sign in and a failure that repeats says what to check instead of asking you to send again
- Fixed Settings, Claude accounts saying "No reading yet" for an account that was not signed in; it now says Not signed in with a Sign in button and never offers Sign in to an account that is signed in
- Fixed Settings, Updates saying "RichOS could not prepare this update" when another update task was only busy; it now waits a moment and, if it is still busy, says so
- Fixed RichOS saying "RichOS could not open" when you opened it again while it was going back to the previous version
- Fixed Quit and stop the work leaving its jobs to resume at the next launch
- Fixed stopping a job leaving the commands Rich and Stu were running for it still going
- Fixed technical rows coming back into the conversation when you switched Technical view off just as a reply finished

## 1.2.1

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.1)

- Added a "See what's new" link in Settings, Updates that opens the changelog in your default browser
- Added Haiku naming a new conversation for you
- Added the Oxford comma setting, which is off by default
- Added a Settings row and email display so you can tell which Claude account is which (Show email on Claude accounts)
- Added "+ New chat or company" and companies as sidebar section headings, with an open folder and an instant "+ New chat" tooltip
- Added the phone page listing a new conversation as soon as it starts
- Changed dictation's name to RichScribe
- Changed the account switch so it really moves Rich and Stu and the panel says where each one is
- Changed Rich's status line to say "Rich is thinking"
- Changed the "Under the hood" rows to say what is true now
- Changed the right-click menu so no browser items show anywhere and text fields offer only Cut, Copy, Paste and Select All
- Improved the phone page so it picks up where the stream left off and keeps its conversation list after a reconnect
- Improved the phone page to draw Rich's answers as formatted text, with code shown exactly as written
- Improved Rich's spoken turns so they keep every word over silence and room noise and always end
- Improved Rich's spoken times on the hour so they are said whole
- Improved the loading screen so Stop works there too
- Improved the Oxford comma rule, the Rich instructions and the overload handling so overloads are shown and workers share one overload clock
- Fixed tables and quotations not showing in every answer and a quotation now ends a table
- Fixed a phone question's answer landing in the wrong conversation, so it stays in the one it was sent from
- Fixed Stop not covering answers given on question cards and the card now shows the truth after Stop
- Fixed Stop not being remembered after a restart
- Fixed Escape not closing the open dialog, including the report over Connected folders
- Fixed Rich dropping "do it yourself" requests
- Fixed a PDF highlight appearing on right-click
- Fixed Rich's sentences being split by a queued notice
- Fixed a withdrawn question card showing an id
- Fixed Restore showing for words that were not Rich's own
- Fixed the "Voice is ready" line staying after the round button is learned
- Fixed Rich's spoken "On it!" so it is spoken right away

## 1.2.0

- Added working voice: setup downloads and checks the speech models, so Talk to Rich works on a fresh install
- Added spoken replies: Rich answers out loud in his own voice, ready as soon as the app is up
- Added Dictation: choose a function key, then speak to type in any app on your Mac (Settings, Dictation)
- Added Use Rich from your phone: pair a phone in Settings, check that six words match, then send Rich messages and voice notes from it and get notified when he answers
- Added question cards: when Rich needs an answer he asks on a card you answer by clicking, typing or speaking and other work keeps going meanwhile
- Added hiring: Rich can add teammates to your company and the home screen shows your team
- Added video watching: name a video and Rich downloads it and tells you what is in it
- Added the Output panel (⌘⇧O): every file a conversation produced, with previews, Open, Show in Finder, Save a copy and Add to chat
- Added pasting or dropping screenshots and files into a conversation
- Added Claude accounts: add more than one Claude subscription and RichOS moves the work to the next one before the current one runs out
- Added a Token spend screen that shows what Rich's work costs today, this week, this month and in total
- Added a Claude Code quota panel in Technical view
- Added a choice of model for Rich, separately for talking with you and for directing your team
- Added Connected folders in Settings: folders your team works in, each kept in Git from the start
- Added Bust a bug: Rich writes a bug report with you and posts it as a GitHub issue once you approve it
- Added Request a feature, directly under Bust a bug, posted as a GitHub discussion
- Added an optional switch that lets Codex review your team's work in progress
- Added a short first-run interview in a conversation called Onboarding, so Rich learns about your business
- Added Go back to the previous version: one press after an update returns to the version you had
- Added right-clicking a conversation in the sidebar to rename, pin or archive it
- Added ⌘⇧S to hide and show the sidebar
- Changed all settings to live in one Settings panel
- Changed the app to follow your Mac's light or dark appearance unless you pick one
- Changed the app background to a subtle speckled texture in both themes
- Changed closing the window to keep Rich's work running; Quit tells you first when work would stop
- Changed background jobs to finish and report back on their own
- Changed updates to install without an administrator password, wait until no work is running, then relaunch by themselves with your window, conversation and draft kept
- Improved Talk to Rich so you can pause naturally while speaking and every word is kept
- Improved how quickly Rich's first reply arrives after you send a message
- Improved Stop so it also ends a reply Rich is speaking
- Improved Escape so it closes popups and dialogs
- Improved work that needs the screen: it waits while your Mac is locked and carries on when you unlock it
- Fixed one damaged saved record stopping the app before its window opened
- Fixed a message typed while Rich was busy being dropped
- Fixed a sent message briefly disappearing from the conversation
- Fixed Claude Code not being found when it was installed with Homebrew or npm
- Fixed the version on screen not matching the running app
- Fixed the window opening larger than the screen it opens on
- Fixed scroll bars not following the light or dark theme
- Fixed the conversation leaving the newest message when the window is resized
- Fixed a pin, archive or rename that could not be saved still looking saved

## 1.0.2

- Fixed the setup sheet closing into an app that could do nothing and answers now read as prose instead of showing asterisks

## 1.0.1

- Fixed the first message on a fresh install; voice no longer pretends to listen and the window opens at its designed size

## 1.0.0

- First public release, for Apple Silicon Macs: you talk to Rich by typing (voice was not available yet)
