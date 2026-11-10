# Nightly changelog

What changed in each RichOS nightly build from 1.2.1-nightly.20261019.67 on, newest first; stable versions are in the [changelog](CHANGELOG.md).

## 1.3.0-nightly.20261110.93

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.0-nightly.20261110.93)

- Added RichPass on the navigation rail: a free password maker that makes the same strong password for each website again from your master passphrase every time and never saves it; Download RichPass saves it as one file you open in Safari, Chrome or Firefox, even offline
- Added protection for your copy of RichPass on its page and in Settings: while RichOS is open it puts back the exact original if anything changes, renames or removes the file and tells you to reopen it
- Changed your Mac's request for access to your Desktop, Documents, Downloads or a removable drive to say what RichOS uses it for
- Fixed the first click in the RichOS window doing nothing when RichOS was not the app in front, such as right after you closed RichCleaner's window
- [technical] Added RichPass's protection checks, without file names or contents, to RichDoctor's reports

## 1.3.0-nightly.20261107.91

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.0-nightly.20261107.91)

- Added a navigation rail along the left edge of the window with Home, Chats, RichFinder (the search ⌘K opens), RichCleaner, RichScribe and Bust a bug; your Mac's resources square and your initials, which open Settings, moved onto it from the bottom of the sidebar and stay in view when you hide the sidebar
- Added Conversations and Company memory to RichFinder: it finds your messages and Rich's replies in all your conversations by what they are about and choosing a message takes you to that exact message; where your team keeps company memory, View record shows the matching record and where it came from
- Added pictures, PDFs, Office documents, audio and video in your search folder to RichFinder, found by the notes and labels RichCleaner gave them
- Added Try again to RichBrief's message when it could not prepare your meeting notes: it makes them again from the saved recording without recording anything new and the message no longer shows technical error text
- Changed the meeting notes prompt to show the RichBrief name and icon; the RichScribe and RichCleaner settings now show their icons beside their names and RichFinder's icon sits beside Super Finder
- Changed RichCleaner's window to say when RichCleaner is off and offer Turn on RichCleaner instead of showing filing steps that cannot work yet
- Changed the Bust a bug panel to mention that RichDoctor in your menu bar can file the same report if RichOS ever won't open
- Improved RichFinder with its own search model built into RichOS in place of Apple's: it puts the right file first more often, shows a better passage from it and stays quick while it reads new files
- Improved RichFinder to use a new or removed search folder in RichOS's settings file right away, for you and for Rich, where before it took a restart
- Fixed meeting notes failing when a meeting ended while RichOS was still downloading what it needs to transcribe; RichBrief now says "Recording saved. Finishing setup for your notes…" and waits up to five minutes
- Fixed meeting notes of a call in Google Chrome stopping moments after you joined
- Fixed meeting notes you started from a calendar reminder recording until you pressed Stop when you joined that Zoom, Microsoft Teams or Webex meeting in your browser; they now stop when you leave the call
- Fixed RichBrief's recording indicator disappearing without a word when its recorder stopped unexpectedly; it now says nothing or only part of the meeting was recorded and offers Try again
- Fixed meeting notes warning that microphone access might be off for RichOS even after your microphone had started picking up sound
- Fixed the tooltip on the message box's work status staying on screen after you moved the pointer away when that status updated
- [technical] Added RichFinder's timings, without your search words, to Bust a bug reports and RichDoctor's reports
- [technical] Fixed a ready-to-land handover from Codex being skipped without a review when it listed its repositories one per line; one naming no repository RichOS can review now gets a reply saying so

## 1.3.0-nightly.20261105.90

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.0-nightly.20261105.90)

- Added meeting notes: when another app such as Zoom or Microsoft Teams starts using your microphone or a meeting on your connected Google or Microsoft calendar is a minute away, RichOS offers Take Notes at the top right of your screen; nothing is recorded until you press it, the recording of both sides stops by itself when the call ends and Rich then posts the summary, decisions, your actions, other people's commitments and open questions in the conversation you had open, with the transcript in the Output panel
- Added Start meeting notes and Pause meeting prompts to Settings: record a conversation in person until you press Stop or stop RichOS offering notes until you press Resume
- Added asking Rich about past meetings: in a company's conversations he can read that company's meeting notes and transcripts
- Added documents to Search: name one folder of Markdown notes in RichOS's settings file and Search finds them on your Mac by what they are about with Super Finder or word for word with Require quoted text; Open original opens the file and Rich can search the same folder for you
- Fixed the SwearMeter count in Settings being partly hidden under the bottom of the menu when you turned on Show SwearMeter

## 1.2.9-nightly.20261105.89

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.9-nightly.20261105.89)

- Added writing help to RichScribe's menu in the menu bar: Clean up last dictation tidies what you just said and Rewrite selected text changes text you selected the way you tell it out loud; Claude shows the original beside its revision and nothing changes until you press Replace or Copy; RichOS asks before your text first goes to Claude
- Added Vocabulary to Settings, RichScribe: choose a company and save how its names are spelled and RichScribe types them that way; you can switch the company from RichScribe's menu too
- Added Recover last draft to RichScribe's menu, which keeps what you last dictated for 30 minutes while RichOS is open so you can copy it again
- Added Live preview to Settings, RichScribe, More options: turn it on to see your most recent words while you speak
- Added SwearMeter: turn on Show SwearMeter in Settings to see in the home screen's left column how many swear words are in what you have typed or said to Rich and your question card answers, counted on your Mac
- Changed RichScribe to type only into the app and window where you started: if you move somewhere else first, your words are copied and kept in Recover last draft instead; it never types into a password field
- Changed the RichScribe bar while you speak to show which app your words go to and which vocabulary is in use, with Finish and Cancel buttons and a warning when another app keeps your key from reaching it; afterward it says where your words went
- Fixed changing your RichScribe key stopping work Rich still had running in the background in other conversations of the same company
- Fixed a Bust a bug report or Request a feature idea being able to reach GitHub twice when it went out but RichOS never heard back
- Fixed a Claude account added right after removing another being able to take on the removed account's usage, which could pause your team's work
- Fixed a conversation's Output panel failing to list its files or take new ones after RichOS was interrupted while noting a file there
- Fixed the Token spend screen leaving out some requests when RichOS stopped while saving its totals
- Fixed a rule saved to Your instructions being lost when another change to them was saved at the same moment
- Fixed Ask Rich again on a file that is no longer on your Mac showing a message in your name that you never typed
- Fixed RichDoctor not hiding a name in a report when it was capitalized differently and the text nearby had letters such as the Turkish İ
- Fixed RichDoctor waiting forever while looking into your problem when Claude stopped taking in what it sent
- [technical] Added timing details for each RichScribe recording to its log without your words; RichDoctor's reports now include them along with the previous log file
- [technical] Changed the checks on your team's commands and writes to catch Apple's developer-tools placeholder even when it is reached through a link, to refuse a write when the check for listed names cannot run and to stop correcting spelling inside text in code
- [technical] Fixed rare cases where your team's work could be lost or skipped: a teammate's work removed during cleanup when its record was incomplete or it saved new changes while stopping, a worker treated as finished before its last result arrived, a prepared teammate set aside before it started or a review of an earlier job counted for a later one
- [technical] Fixed Rich's check that his push is not failing on GitHub judging a later commit than the one he pushed or dropping a push that reported an error after it went through
- [technical] Fixed a teammate's ordinary command being refused when it only printed, searched for or quoted a shell's -c option or a visible-browser option, for example in a commit message

## 1.2.8-nightly.20261102.88

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.8-nightly.20261102.88)

- Changed the home screen once you have a team: your own memories now light up in gold in front of a dimmed example marked "Example, not yours", which fades as yours grow and is gone at 2,500, where before there was no picture until you had about 2,000 memories
- Changed the home screen's left column once you have a team to also show how many teammates are working now, sources understood, decisions and lessons remembered and how many memories Rich has from you
- Improved how soon your memories appear on the home screen: they are drawn in as soon as RichOS has read them and show right away on later launches while they are unchanged
- Fixed RichDoctor not being brought up to date when it was open as RichOS started; RichOS now updates it within about a minute of you closing its window and no longer closes a minimized or hidden RichDoctor window, which could lose what you had typed

## 1.2.7-nightly.20261102.87

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.7-nightly.20261102.87)

- Changed the home screen's right-hand column from your whole team to Working now: it lists only the teammates working at this moment, up to six, each with its role and is hidden when nobody is working
- Fixed RichDoctor never being brought up to date after it was first installed; each newer RichOS now replaces it and RichDoctor's report shows the RichOS version it came with
- Fixed RichDoctor's window not moving when you dragged it by its top strip, including while a panel was open in it
- [technical] Added your team taking up the teammates defined in your companies' repositories that use the RichOS engine each time work starts, a company's own repository first; a teammate made only in RichOS or edited there since is left as it is

## 1.2.6-nightly.20261102.86

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.6-nightly.20261102.86)

- Added a + button to the message box that opens your Mac's file picker for files and folders; a folder, picked or dragged in, is handed to Rich to read where it is instead of being copied
- Changed the message box to a new look: when empty it reads "Type here… OR…" with Talk to Rich on the line below; the +, the status line, Output and Send sit in a row under your words
- Changed RichDoctor to send a report by itself once it can when your Send did not get through because you were offline, the reporting service was busy or unreachable or RichDoctor quit while sending; it no longer comes back as unfinished
- Changed closing RichDoctor's window while it is looking into your problem so it no longer cancels: reopening it carries on with Claude and RichDoctor retries Claude's passing hiccups by itself
- Changed Prepare without AI in RichDoctor to ask when the problem happened, what you expected and what happened just before instead of leaving them for you to fill in; RichDoctor asks the same on a Mac without Claude Code
- Improved adding a screenshot in RichDoctor: one press of Use this image finishes it, where it used to take two
- Fixed RichOS opening each time in the conversation created most recently, often one you never used; it now starts with no conversation selected so you pick one
- Fixed removing an account in Settings, Claude accounts breaking every conversation that had run on it; your conversations' history now stays on your Mac whichever account you remove
- Fixed a conversation whose earlier history is missing from your Mac failing on every message and sending you to check your sign-in; Rich now says the history is gone and your next message starts afresh in the same conversation
- Fixed RichOS showing agents working right after it started by counting teammates that had stopped when it last quit
- Fixed Stu being handed the same old jobs again every time RichOS started
- Fixed Dictation saying it was still downloading the voice AI every time you used it if its speech engine was being replaced just as RichOS started
- Fixed a Bust a bug report or Request a feature idea that Rich had written up being lost when RichOS quit, restarted or crashed before you sent it; it is now kept on your Mac with your own changes and you can close the panel and bring it back with the same button
- Fixed Send in Bust a bug making you wait for Rich to check your report again after he had changed it as you asked; sometimes it even had to be pressed twice
- Fixed a Bust a bug report or idea waiting to go out being lost when you pressed Change it and then Done without changing anything or when Rich could not make the change you asked for; it now goes back to waiting as it was
- Fixed RichDoctor offering a report you had already sent as an unfinished report to continue; choosing Start a new report now also stops it offering the old one again
- [technical] Fixed a conversation that could not start being tried again for every message waiting in it; after two failed tries the rest stop at once with the same reason and a failed start leaves no files behind
- [technical] Fixed the cleanup at the end of your team's work running out of time before it could remove that work's temporary files

## 1.2.5-nightly.20261101.85

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.2.5-nightly.20261101.85)

- Added a live square at the foot of the sidebar, opposite your name, showing how busy your Mac's processor and memory are; point at it or click it for the last minute in detail and right-click it to choose Readout, which also shows both as percentages
- Added RichCleaner (Settings, RichCleaner), which works with RichScribe: select a file on your Desktop, tap your RichScribe key and say what it is; RichOS files it in a fitting folder inside one you choose and keeps your note for you or Rich to find it by later; Recent files lets you search, change a note or undo a filing
- Added Rich asking you before installing a tool your team's work needs and your Mac lacks, such as Apple's command line developer tools: yes installs it once and the work carries on, no is remembered and telling him to install whatever is needed lets him do so from then on without asking
- Changed talking to Rich from your phone to go only through the RichConnect app: the phone web page is gone, its address now says to use the app and a copy installed on a phone removes itself the next time it reaches your Mac
- Changed the paired phone's card (Settings, Use Rich from your phone): it shows Connected or Reconnecting beside the phone's name and whether it can send you notifications, with what to switch on in RichConnect if not; a phone paired through the old web page is shown as needing to be paired again
- Changed switching to another Claude account, at the weekly or the five-hour limit, so it no longer stops teammates in the middle of their work: they finish on the old account and Stu moves once they are done
- Changed the pause near a Claude usage limit so a teammate's commands still run and only its next request to Claude waits
- Changed Settings, Claude accounts for an account with no five-hour limit: it shows only the weekly limit and your team is no longer paused for a five-hour limit that account doesn't have
- Changed Rich so he no longer suggests switching Claude accounts just because another account's limit has reset; RichOS pauses or switches by itself when the account in use reaches its limit
- Changed the setup screen to ask for an Ultra subscription on your Anthropic account instead of Max
- Fixed Rich being unable to carry on when a command he had left running finished after his reply; he now continues in the conversation, where you see what he says and can press Stop
- Fixed an update being held back after the work it waited on had ended: a damaged job record or a helper Rich started before his conversation closed no longer counts as work still running
- Fixed your team staying paused for up to five minutes after a Claude usage limit reset, with Settings, Claude accounts still showing the old figures; both now catch up within seconds
- Fixed your team staying on a Claude account after its weekly limit for one model, such as Fable, ran out, with automatic reviews still starting there; RichOS now treats that like the account's own weekly limit
- Fixed Stu waiting for hours with nothing said when he started a teammate while your Claude usage was on hold; he is now told at once which account is held, why and until when
- Fixed RichDoctor's window popping up over RichOS the first time RichOS starts; it now starts quietly in the menu bar
- [technical] Added dedicated connections between Codex and your RichOS conversations: each Codex task gets its own, messages from Codex go to Stu and you can ask Rich to make a conversation the one new Codex tasks connect to
- [technical] Added a way for Stu to mark a teammate's earlier attempt as replaced by redone work, so the job can close instead of staying open
- [technical] Changed your teammates' and Stu's ordinary commands to run and return their result directly, as in Terminal, instead of being moved to the background; a teammate that is paused still gets the word at once
- [technical] Changed Rich's and Stu's work to go through the same checks as in Terminal before a teammate starts and at the end of each turn; a refused teammate start now says what to fix
- [technical] Improved how much of your Claude usage your team spends: notes about your company and ongoing work and the standing notices each teammate gets now go once per conversation and again only after it is compacted or when they change, instead of with every message or step
- [technical] Improved how much of your Claude usage Rich and Stu spend: Stu's failed turns on RichOS's own reminders no longer wake Rich; Stu no longer gets a teammate's whole brief back each time he prepares or looks up a job
- [technical] Fixed your team's commands running RichOS's own Python, Node and Git instead of yours, so your own Python packages were missing; RichOS's copies are still used where your Mac has only Apple's placeholders
- [technical] Fixed commits by your team carrying a co-author line after you had turned that off in Claude Code's settings
- [technical] Fixed a conversation's Claude usage still being counted to its old account after it moved to another one, which skewed when RichOS paused or switched
- [technical] Fixed messages from Codex being lost when RichOS quit before a conversation was open to receive them
- [technical] Fixed Stu's request to start a teammate being refused without a clear reason when RichOS's checks before the start took longer than 20 seconds
- [technical] Fixed Stu being told that held work had gone quiet or was overdue while it was waiting on purpose, for a usage hold or for a time he had set
- [technical] Fixed a command Stu started with nohup not counting as running work, so an update or Quit did not wait for it and Stop did not end it
- [technical] Fixed pausing one teammate also freezing a shared build cache server, which hung other teammates' builds
- [technical] Fixed your team staying paused until the reset when every Claude account reached its five-hour limit with Switch chosen; they now resume one at a time in the last 20 minutes, as with Pause
- [technical] Fixed Stu's landing of finished work onto a release branch being refused when the repository's main folder was on another branch
- [technical] Fixed finished teammates' folders and branches being left in your repositories when the conversation that started them had ended for good; the next check now cleans them up
- [technical] Fixed the cleanup of leftover files deleting a live conversation's working folder when its session had started on the 1st to the 9th of a month and reporting a folder that was already gone as a failed cleanup
- [technical] Fixed RichOS writing a second, ever-growing copy of each conversation's record on your Mac, which could reach hundreds of megabytes
- [technical] Fixed your team sometimes reading your Mac's processor and memory use as unavailable when RichOS's data folder was reached through a link

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
