# Changelog

What changed in each stable version of RichOS, newest first; nightly builds are in the [nightly changelog](CHANGELOG-NIGHTLY.md).

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
