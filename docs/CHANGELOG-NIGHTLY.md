# Nightly changelog

What changed in each RichOS nightly build from 1.2.1-nightly.20261019.67 on, newest first; stable versions are in the [changelog](CHANGELOG.md).

## 1.2.3-nightly.20261024.74

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.3-nightly.20261024.74)

- Added file cards in the conversation: when Rich sends you a file from your Mac, it shows under his reply with its type, size and a PDF's page count; click it to open it in its usual app or use Show in Finder and a file since moved or deleted says so and offers Ask Rich again
- Added the same file cards on the phone page: tap one to bring the file over from your Mac and open it or save it when your phone cannot show it; files over 100 MB open only on the Mac
- Changed what Rich does with work left on hold or finished but not merged into your project: instead of asking you after 30 minutes whether to go ahead, he goes ahead, merges or drops it himself and asks you only about a decision that is truly yours
- Improved Stu's reminder about a teammate's finished work that is not yet merged: it now comes as soon as he is free and every 30 minutes until the work is handled, instead of after about 12 hours
- Fixed a job sitting still after Stu added a new teammate for it; the new teammate now picks it up
- Fixed Rich saying he had saved or written something down when he had not: he is now told as he finishes and either saves it or tells you it was not saved
- [technical] Changed jobs that change only a project's changelog files so they get no automatic review while they run
- [technical] Changed the ready-made release notes writer on the teammate shelf to run on Opus instead of Sonnet
- [technical] Improved the check that reads each job before a teammate starts it: it now reads the whole job first and no longer turns a job back over a contradiction it misread or a setting it could not see
- [technical] Fixed a teammate's ordinary command being refused as a hidden shell command when a folder name in it started with a dash, as Claude's own temporary folders do

## 1.2.2-nightly.20261023.71

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.2-nightly.20261023.71)

- Added RichDoctor, a small menu bar app that RichOS installs beside itself for reporting a RichOS problem even when RichOS will not open: describe what went wrong, add screenshots and read the exact report before you choose to send it
- Added Rich's and Stu's instructions and your own standing instructions as files in your RichOS folder: edit them and both follow the changes without a restart; Rich saves a "from now on" order there for you
- Added asking you before a later RichOS version updates Rich's or Stu's instructions that you changed, with a before and after that marks every edit; a copy you never changed is updated by itself
- Added connecting one project folder to more than one company in Connected project folders: RichOS tells you before you connect it and each folder shows which other companies have it
- Added the full set of skills RichOS ships to Rich, Stu and their teammates, such as copywriting, front-end design, browser testing and iPhone and Android app development
- Added memory for Rich and Stu, so what they learn carries into later conversations
- Added Rich telling you when work has been on hold for 30 minutes with nothing set to end it; he asks whether to go ahead or keep holding it until a date or event
- Changed the commands Rich and the team run to use your Mac's own setup: the tools and settings your Terminal has, plus your own Git name and saved Git sign-in for commits and pushes
- Changed merging finished work into your project to run the project's own merge checks, as a merge in Terminal does; a failed check merges nothing and its own words say why
- Changed how Rich and Stu work for you: Rich answers exactly what you asked, checks a result before saying it is done and never asks you to run a command; Stu has a new screen mocked up for you to choose before it is built
- Improved the work teammates do in your project folders: they follow each project's own instruction file, as they would in Terminal
- Improved Talk to Rich when it misses what you said: it asks you to say it again; if Rich was talking over you, it waits until he stops and keeps even a one-word reply
- Fixed Rich, Stu and their teammates stopping for good when the internet connection dropped; they now wait for it to come back and carry on where they stopped
- Fixed messages failing over and over with "This usually fixes itself" when no Claude account was signed in on your Mac or its sign-in had expired; RichOS now sends you to Settings, Claude accounts to sign in and a failure that repeats says what to check instead of asking you to send again
- Fixed Settings, Claude accounts saying "No reading yet" for an account that was not signed in; it now says Not signed in with a Sign in button and never offers Sign in to an account that is signed in
- Fixed Settings, Updates saying "RichOS could not prepare this update" when nothing was being prepared; it now waits a moment and says when another update task is still busy
- Fixed Quit and stop the work leaving its jobs to resume at the next launch
- Fixed stopping a job leaving the commands Rich and Stu were running for it still going
- Fixed technical rows coming back into the conversation when you switched Technical view off just as a reply finished

## 1.2.2-nightly.20261021.70

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.2-nightly.20261021.70)

- Fixed Rename in a chat's menu doing nothing; it now turns the chat's own row into a text field, saves with Enter and cancels with Esc and the sidebar, the chat header and the company overview all show the new title
- Fixed the first message typed right after reopening RichOS vanishing and messages sent before the conversation opens now reach Rich in the order you sent them

## 1.2.1-nightly.20261020.68

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.1-nightly.20261020.68)

- Added See what's new in Settings, Updates, which opens the changelog in your default browser
- Added a Show email option so you can tell your Claude accounts apart
- Added Haiku naming each new conversation
- Added a setting for the Oxford comma, which is off by default
- Added + New chat or company in the sidebar, with companies as section headings, an open folder and an instant + New chat tooltip
- Changed dictation's name to RichScribe
- Improved the phone page: it picks up where the stream left off, keeps the conversation list after a reconnect and shows Rich's answers as formatted text, with code exactly as written
- Improved your spoken turn so it keeps every word over silence and room noise and always ends
- Improved Rich's spoken times on the hour so they are said whole
- Fixed Rich dropping "do it yourself" requests
- Fixed Escape not closing the report over Connected folders
- Fixed right-clicking in a PDF highlighting text
- Fixed Rich's sentences being split by a queued notice
- Fixed a withdrawn card showing an id
- Fixed Restore appearing on words that were not his own
- Fixed a quotation not ending a table
- Fixed a card showing the wrong state after Stop and Stop not being remembered across a restart
- Fixed the "Voice is ready" line staying after you have learned the round button
- Fixed the wording under Under the hood so it is accurate

## 1.2.1-nightly.20261019.67

- Changed the status band to say "Rich is thinking" until something comes back from Rich
- Improved Rich's answers so tables, quotations, dividers and code blocks render, in finished and stopped answers alike
- Improved what happens when Claude is overloaded: the work waits and retries on its own, from 1 to 80 minutes and the conversation shows the retry
- Fixed a switch to another Claude account being reported before it happened; the whole team now moves, including work that runs on its own
- Fixed Claude accounts showing an account as in use before anyone was on it; the panel now says when a move is under way
- Fixed RichOS switching Claude accounts early after a small jump in weekly use and too late after a sudden burst
- Fixed Rich's notes to himself showing up in the conversation
- Fixed Rich answering a cut-off message on his own after a restart; restoring it is now your choice
- Fixed Under the hood saying a finished job still needed a review and showing a replaced review or attempt as still current
- Fixed an answer clicked on a question card just before Stop not being stopped with the rest
- Fixed Stop pressed on the loading screen not reaching the conversation it names
- Fixed right-click showing the browser's own menu items: text fields now offer only Cut, Copy, Paste and Select All and the PDF preview shows no menu
- Fixed Escape not closing the Bust a bug and Request a feature panels
- Fixed tapping an answer on a question card on the phone page doing nothing; keeping the same answer now finishes editing
- Fixed a question card answered on the phone page showing up in another conversation after you switched
- Fixed the phone page not listing a conversation started on the Mac and showing its messages in the conversation on screen
