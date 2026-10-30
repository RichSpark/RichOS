# Nightly changelog

What changed in each RichOS nightly build from 1.2.1-nightly.20261019.67 on, newest first; stable versions are in the [changelog](CHANGELOG.md).

## 1.2.5-nightly.20261030.82

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261030.82)

- Changed a conversation you left more than 30 minutes ago to stop running in the background when nothing is going on in it; your next message there picks up where you left off, though Rich's first reply takes a little longer
- Changed how your team's finished work is reviewed before it is merged: RichOS now reviews it once by itself, with Codex when "Let Codex review your team's work" is on and with Claude otherwise, instead of Stu starting a separate reviewer
- Changed your team so it can no longer keep your Mac awake or change its sleep settings
- Fixed RichOS asking you to set up its engine again when it was already installed and working
- Fixed your team's browser work opening browser windows on your screen or making macOS ask you to allow Chrome's notifications; it now runs out of sight
- Fixed your team's commands making macOS ask you to install the command line developer tools on a Mac without them; the step now stops and names the missing tool instead
- Fixed teammates waiting on a long command using a large share of your Mac's processor; a waiting teammate now uses almost none
- Fixed Stu putting work off because he read another Claude account's weekly use: when he checks usage he now gets the account the work runs on, by name and only its figures
- Fixed RichOS moving your team off a Claude account with most of its week left when some of their work ran on a model that account had seldom used, such as Haiku
- [technical] Added Rich, Stu and your teammates being able to read your Mac's current processor and memory use, with the last minute of history, when it matters to their work
- [technical] Added your Mac's own memory figures, such as its total memory, to what your team is kept from committing to a repository marked public
- [technical] Fixed the automatic Claude review of your team's work starting while their Claude usage was on hold; it now waits until the account it runs on has room
- [technical] Fixed Stu being told about a waiting teammate from another conversation or told a teammate that was still working had gone quiet, such as after a switch between Claude accounts
- [technical] Fixed your team being refused in a repository protected by a land fence while it ran on an added Claude account
- [technical] Fixed the scheduled cleanup of a RichOS engine installed from Terminal treating a live app conversation's own folder as leftover and telling Stu about the same cleanup problem again each time its count changed

## 1.2.5-nightly.20261029.81

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261029.81)

- Changed how agents held near your five-hour Claude limit come back: instead of all resuming at once 20 minutes before the reset, they now resume one at a time from then and only while the account stays under 100%; the rest wait for the reset
- Fixed Settings, Updates and the question before Quit saying an assignment was still running in the background after all its work had ended, which could keep an update from installing; such a job now goes back to Stu at the next start
- Fixed Stu being handed a job he already had again, sometimes several times in a few minutes, as if it were a new order; after a restart he gets it back once, marked as one he was already given
- Fixed jobs stalling until you sent a message: when a teammate finishing woke Stu or a new message arrived while he was working, he could not start the teammate he had lined up next
- Fixed a teammate Stu had just lined up for a job being cleared away before it started when he closed that job in the same reply; the job now stays open until that teammate runs or Stu clears it
- Fixed RichOS moving your team off a Claude account that had most of its week left and holding their work, when one large burst of use was reported all at once
- Fixed Stu's work stopping when one of his steps hit a Claude usage limit: the step is tried once more, on another account with room or, when none has room, after the limit resets; the conversation says when
- Fixed a message Stu had already acted on being given to him again after RichOS restarted, when the reply it came in had been cut short
- Fixed Settings, Claude accounts showing a switch before it had happened: the account the work still runs on stays first and in use, the screen says "Switching to" the new one or "Can't move yet" while work still runs on the old one and says "Rich now uses" the new one only once the move is real
- [technical] Improved what Stu is told when RichOS refuses to start a teammate: the reason now names the job and its cause, such as who stopped it and when or the earlier request it repeats
- [technical] Fixed your team's commands slowly taking memory from your Mac that only a restart gave back: the Python, Git and Node that RichOS provides now start directly instead of through small scripts that set off a macOS memory leak
- [technical] Fixed Stu's ordinary messages to a teammate being refused when they mentioned a pause or holding something, such as "the pause button is misaligned"
- [technical] Fixed a teammate's work being paused or let through on the readings of the Claude account in use instead of the account it actually runs on, during a switch between accounts

## 1.2.5-nightly.20261029.80

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261029.80)

- Fixed a finished job staying open because a teammate Stu had lined up for it never started; such a teammate is now cleared away and any work in its folder is kept
- Fixed Stu being unable to start a teammate he had lined up before his last reply
- Fixed Stu being told a teammate had gone quiet when that teammate had never started
- [technical] Added a way for Stu to clear a teammate he lined up and will not start and to set aside a stray record left by a test run, which had stopped the cleanup of leftover teammate folders in that conversation
- [technical] Fixed every open Stu conversation being told about the same problem with the scheduled cleanup run by a RichOS engine installed from Terminal and told again after Stu had acknowledged it; now one conversation is told and again only when the problem changes

## 1.2.5-nightly.20261028.79

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261028.79)

- Fixed your team being unable to check or save work in a folder where a file or folder name has an accent or other non-English letter, such as café.txt
- [technical] Fixed a Claude Code session in Terminal that uses RichOS's engine stopping and deleting work your team had in progress in the app on the same Mac

## 1.2.5-nightly.20261028.78

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261028.78)

- [technical] Added Codex's notes reaching Rich by themselves: each new entry Codex writes to him in one of your Codex channel folders comes to him in the conversation you have open and Rich and Stu can write back in that folder
- [technical] Added the engine's safety checks to your team's work in the app: a file holding a password or access key is refused before it is written, private material and names on your list stay out of a repository marked public and British spellings become American where your settings ask for American spelling
- [technical] Improved Download update and Go back in Settings, Updates: they now download from RichOS's new GitHub address first and the old one second, so an update found before the move no longer relies on GitHub forwarding the old address

## 1.2.4-nightly.20261027.77

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.4-nightly.20261027.77)

- Fixed the new GitHub address RichOS moves to: it is github.com/RichSpark/RichOS, not github.com/RichCore/RichOS as the previous nightly said, so updates, bug reports and ideas, including from RichDoctor, keep working through the move with nothing for you to do

## 1.2.3-nightly.20261027.76

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.3-nightly.20261027.76)

- Changed RichOS to keep finding its updates and sending your bug reports and ideas, including from RichDoctor, when it moves to its new GitHub address, github.com/RichCore/RichOS; there is nothing for you to do

## 1.2.3-nightly.20261025.75

[GitHub release](https://github.com/WebDevBooster/richos/releases/tag/v1.2.3-nightly.20261025.75)

- Added Your instructions in Settings: write the rules Rich follows in every company or press Have Rich ask me and he asks you a few questions and writes them down; they apply from your next new conversation and rules he saves show as a Saved card under his reply
- Added Terminal setup in Settings: see the tool folders and settings RichOS read from your Terminal and press Read it again after you change them; it shows what changed and your team uses it from its next job
- Added a question the first time your team saves work in one of your folders and your Git setup has no name for you: type the name and email to put on your team's work or choose Use "RichOS" instead; only that save waits and you can change it later in Settings, Your name in Git
- Fixed Bust a bug and Request a feature never sending: an approved bug report now reaches RichOS on GitHub and an idea is posted under Ideas, with nothing to set up on your Mac
- Fixed messages sent close together, such as one typed and one spoken or sent from your phone, sometimes reaching Rich out of order
- [technical] Fixed RichOS leaving temporary files on your Mac after use, a Quit or a crash; it now removes them when it starts, when your team's jobs are done and when you quit
- [technical] Fixed Rich and Stu being told to run commands the app does not have when work was waiting on them; they are now told the app's own steps to resume, merge or continue it
- [technical] Fixed Stu being turned back at the end of a turn and writing extra replies after he had correctly closed a finished job, added a teammate or filed a job report

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
