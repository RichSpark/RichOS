# Nightly changelog

What changed in each RichOS nightly build from 1.2.1-nightly.20261019.67 on, newest first; stable versions are in the [changelog](CHANGELOG.md).

## 1.3.8-nightly.20261127.120

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.8-nightly.20261127.120)

- Fixed work cut off when RichOS closed unexpectedly, for example in a crash, showing as Working after reopening while nothing ran; the conversation now says it is paused and writing anything there carries on Rich's answer or the team's job
- Fixed a new Codex report or a press of Stop setting off work left from while RichOS was closed, which used your usage; it now waits until you next write, talk or answer a question in that conversation
- Fixed RichFiles Window Gestures, such as double-clicking a desktop folder, Forward Delete on the desktop, Dock gestures or title-bar gestures, stopping working after a RichOS update, including the update to this build; they keep the permission you already gave

## 1.3.8-nightly.20261126.119

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.8-nightly.20261126.119)

- Added My RichOS, off until you turn it on in Settings, How it looks: a live monitor on the bar on the left, under your Mac's resources square, that shows whether RichOS is using tokens right now and how fast and how much memory the RichOS apps use; point at it for the last 15 minutes and open Tokens or Memory to see each chat, model and app
- Added resting the pointer on a file or folder in RichFiles List or Columns view to see its kind, size and date
- Changed Rich's recommended and default effort on Opus to High, including when RichOS moves him from Fable to Opus by itself; a level you already chose stays
- Changed renaming in RichFiles List and Columns view to happen right in the list: press F2, choose Rename or click a selected item again after a pause, then type the new name and press Return
- Changed Open With in RichFiles' right-click menu to list the apps that can open the file with their icons, the default first, followed by Choose Another App…
- Changed the RichFiles sidebar into a folder tree: only folders that hold other folders get an arrow, the arrow opens them in the sidebar without changing the files on the right and open folders stay open
- Changed how RichFiles shows files: a selected item gets a light fill instead of a heavy outline, the item under the pointer is tinted, clicking empty space deselects and right-clicking an item selects it; in List view the columns fit their names and the mouse wheel scrolls sideways
- Changed arrows and Steps in the RichGlance editor to come in Small, Medium and Large, with a much bigger Large; a new arrow starts at Medium and a size you pick after placing a step is used for the next one
- Changed RichGlance's menu bar icon to show the RichGlance eye instead of a viewfinder
- Improved the main buttons in RichFiles dialogs and Disk Cleanup's Move to Trash to be easier to read in light and dark mode, for example Create, Rename, Search and Connect
- Fixed RichOS using your Claude usage as soon as it opened: messages, Codex reports and reviews of Codex work left from while it was closed now wait until you next write or talk in that conversation
- Fixed pasting a copied file or folder into its own folder in RichFiles doing nothing; it now makes "name - Copy", then "name - Copy (2)"
- Fixed the effort steps in Settings jittering when the pointer was over "Recommended"
- Fixed the model and effort you picked in Settings being lost when you closed the window or quit RichOS with Settings still open
- Fixed Forward Delete on your Mac desktop not moving a folder you just made with New Folder to the Trash before you named it
- [technical] Fixed double-clicking a symbolic link to a folder on your Mac desktop opening it in Finder instead of RichFiles

## 1.3.7-nightly.20261124.118

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.7-nightly.20261124.118)

- Added Rich's profile: click his name or picture in a conversation to open it at the side, with what is under way for you right now, the model he thinks with and buttons to message him or talk to him
- Added dragging companies in the sidebar into any order; Home, Settings, RichFinder and every other list of companies follow it
- Added asking Rich how much memory RichOS and its companion apps use: RichDoctor now records it every minute, so Rich can tell you how it changed over the last two days and 90 days and whether one may be leaking
- Added resizing RichFinder's results by dragging any edge or corner; RichFinder keeps that size and grows to it when results appear
- Added double-clicking a folder on your Mac desktop to open it in RichFiles, on by default in RichFiles Settings, Keyboard, once you allow RichFiles Window Gestures in macOS
- Added a Line tool to the RichGlance editor (press L and hold Shift to snap it straight) and a Thickness menu from 1 to 16 pixels for lines and boxes
- Added editing a picture open in the RichGlance viewer: choose Edit, Edit, press Command-E or click the pencil button to mark it up in the editor
- Changed marks in the RichGlance editor: arrows are solid with a soft shadow, boxes and redactions have no white border and get a handle on each side to resize them one way; every tool keeps the color, size and thickness you last used, even after a restart
- Changed Capture when RichOS is quit in RichGlance to be on unless you turned it off before; it and Show RichGlance in the menu bar now have their own switches on the RichGlance page
- Changed the Mac desktop shortcut that moves selected items to the Trash to Forward Delete (Del or Fn-Delete on a Mac laptop) instead of Delete; it now also works when you press it right after clicking an item
- Changed messages from a Codex task you started yourself to arrive in the Codex chat of the company whose project folder the task works in, instead of your current company; work in a folder no company has waits until one does
- Changed About in RichOS, RichFiles and RichGlance to show the RichOS version and its build number, such as Version 1.3.7 (20261124.118)
- Changed RichScribe's menu bar icon, its menu and its button on the bar on the left to show the RichScribe symbol instead of a microphone
- Improved Make a video to remember your microphone and pointer choices from your last video, from either Make a video button, even after RichOS restarts
- Improved Codex work that runs out of usage partway: RichOS now uses a banked reset the moment the allowance runs out, starting with the one that expires soonest, instead of after the work stops
- Fixed Make a video leaving out the Git view in the Output panel; it now records it as it shows on screen
- Fixed Escape not closing RichCleaner's window right after it opened
- Fixed RichCleaner showing RichFiles' icon in its capture panel, Recent files and settings; its search box now says it searches your filed Desktop files
- Fixed the name beside an app's icon at the top of its page or settings, such as RichPass, not lining up with the middle of the icon
- [technical] Fixed the Codex chat for work on the RichOS project itself landing under whichever company was open: when RichOS opens it moves to the company that has the RichOS project, with nothing lost or repeated
- [technical] Added to RichDoctor's reports whether its memory readings work, how a Codex chat move and a banked reset during a run went, RichFinder results resizing, desktop folder openings, RichGlance viewer edits and saved tool settings; Bust a bug reports now also carry Output panel Git recording timings

## 1.3.7-nightly.20261123.117

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.7-nightly.20261123.117)

- Added asking Rich how much Codex usage you have left, including your banked resets and credit balance
- Changed RichFiles, RichGlance and Window Gestures to switch to the new version by themselves after a RichOS update, from your next update on: each waits until you are not in the middle of something, such as a copy or unsaved marks, then reopens your windows where they were
- Changed Codex work started from a RichOS conversation to run with full access to your Mac instead of only its own folder, so it can finish the whole job
- Improved Codex work that reaches its usage limit: RichOS now uses one of your banked resets by itself when you have one and says so in the conversation
- Fixed Codex work that stopped partway ending without a word: the conversation is now told why it stopped; resuming hands Codex the unfinished request again
- Fixed RichScribe cutting off a dictation in progress when RichOS restarted itself to finish an update; from your next update on, it finishes first
- Fixed RichOS sometimes failing to come back after it restarted itself to finish an update; this works from your next update on
- [technical] Added to RichDoctor's reports whether RichFiles, RichGlance and Window Gestures restarted after an update or waited because they were busy; they also note how the last Codex job went, including any usage limit or banked reset

## 1.3.7-nightly.20261122.116

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.7-nightly.20261122.116)

- Changed RichOS to stop using your Claude usage to get a new conversation ready when it opens onto Home; it does that once you choose a company or open a conversation
- Fixed RichOS handing Stu old messages from your earlier Codex handoffs when it opened after an update; only new messages from Codex reach him now

## 1.3.7-nightly.20261122.115

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.7-nightly.20261122.115)

- Added Capture Again and Copy Text to RichGlance, each with its own shortcut: Capture Again takes the same area as your last capture and opens the two side by side, as before and after or with the changed pixels marked; Copy Text copies the words in an area you select
- Added Copy Text, Pin to Screen, Attach to RichOS Chat and Compare to every capture in the RichGlance editor and in Recent Captures: a pinned picture floats above your other windows with its own opacity and click-through; an attached one waits in that chat's message box until you send it
- Added a search box to Recent Captures in RichGlance that finds captures by the words in them, leaving out anything you redacted; you can also drag a capture from Recent Captures into another app as one picture with its marks
- Added Keep Only Redacted Copy… in RichGlance to replace a kept capture with its redacted version so the original is gone
- Added Capture when RichOS is quit to RichGlance, off until you turn it on in Capture Settings… on the RichGlance page: your capture shortcuts then work from the moment you log in, even with RichOS closed, with a menu bar icon for Capture…, Capture Again, Copy Text and your pinned pictures that you can hide
- Added window tiling and a window switcher to RichFiles Window Gestures, each off until you turn it on in RichFiles Settings: drag a window's title bar to an edge or corner of the screen or press Control-Option-Command with an arrow, 1 to 4 or Return to tile or restore it; press Control-Option-Command-W to pick any open window, minimized ones too
- Added Group by Type to the sort menu in RichFiles, keeping your chosen order inside each kind of file
- Added Choose App… to Open With in RichFiles to open a file in any app
- Added Transfer May Be Stalled to the Transfers button in RichFiles when a copy or move has made no progress for 15 seconds
- Changed Disk Usage… in RichFiles into a disk cleanup view in its window: see what takes up space in a 3D picture or in lists of the largest items, large files, old installers and duplicates; add files to a review list and move them to the Trash with Undo Move; then put them back or delete them for good and see how much space you got back
- Changed New Folder in RichFiles to ask for the name first and create the folder only once you confirm it; if the suggested name is taken, the folder gets a number
- Changed every RichFiles window to remember its own size, place and pane widths instead of only the first window; a window you left partly on another display stays there as long as you can still reach its title bar
- Improved RichFiles to show a folder's first items sooner, open Open With at once while it looks for apps and let you widen each column in column view
- Improved typing the start of a file's name in RichFiles to land on the right file even while the folder is still loading
- Fixed RichFiles on some Macs that use RichGlance refusing to copy or move files and not keeping Undo after it restarted
- Fixed the splash screen's sound sometimes not playing the first time you open RichOS after starting your Mac
- [technical] Added to RichDoctor's reports timings for RichFiles disk cleanup, window placement, stalled transfers, Open With, folder listing, window tiling and the window switcher and for RichGlance comparisons, text reading, search, pins, redacted copies, dragging and the capture helper, without file names or window titles; launch timings also note when the splash sound was ready to play

## 1.3.7-nightly.20261121.112

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.7-nightly.20261121.112)

- Added RichBrief to the bar on the left: one page to start or stop meeting notes, see how long a recording has run, open the notes once Rich has written them, reconnect a disconnected calendar and pause meeting prompts; its button shows a dot while a meeting is being recorded
- Added RichGlance to the bar on the left: a page with Capture… and RichGlance's settings, including the capture shortcut, what Return does after a capture, Recent Captures and using RichGlance as your default image viewer
- Changed the RichGlance screenshot editor to one window with your Recent Captures in a shelf along the bottom that you can hide: click a capture to switch to it with its marks and Undo kept, start a new capture from the editor and zoom in and out, while each marking tool keeps its own size and highlights keep their own color
- Changed Recent Captures in RichGlance to be on unless you turned it off, with a new choice to keep captures on this Mac for 7 days or until you delete them
- Changed handing work between Rich and Codex to need no setup: Rich starts a separate Codex chat for each task he hands over; messages to Rich from a Codex task you started yourself arrive in a chat called Codex in your company
- Improved Talk to Rich after you interrupt him while he is speaking: he knows which sentences you actually heard, so if you ask him to go on he picks up at the first one you missed; if you only say okay, he answers briefly and waits
- Improved SwearMeter to show your count as soon as RichOS opens, from the last saved count, while it reads your Claude and Codex history again in the background once an hour
- Improved the splash screen to play while RichOS is still getting ready and to give way only once Home is ready
- Improved the RichOS window to disappear the moment you click its red close button
- Fixed chats showing Working and Rich saying work was still running when nothing was running anymore, for example after RichOS restarted; when RichOS cannot confirm a job's status it now says so
- Fixed the splash screen's sound sometimes starting late or being cut off before its end
- Fixed RichGlance showing Retina screenshots and other high-resolution pictures twice as large as Preview does at their real size
- Fixed the names that appear when you point at a button looking wrong in Make a video recordings: the screen showed through them, their arrow was missing and the one for Settings was blank
- [technical] Added to RichDoctor's reports when Talk to Rich was cut off while speaking (never the words themselves), how the splash sound played and whether messages from Codex are waiting to be routed; Bust a bug reports now also carry recent timings of closing the window
- [technical] Added a short note to Codex's own instructions on this Mac that points Codex to the RichOS guide, so a Codex task reaches Rich over one connection

## 1.3.6-nightly.20261121.111

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.6-nightly.20261121.111)

- Added When Fable is almost used up in Settings, under Rich's model and effort, on by default: instead of pausing, Rich and your team switch to Opus while your weekly quota has room and go back to Fable when it resets or, if you choose and have a second Claude account, move to an account that still has Fable first; a reply stopped by the Fable limit is tried again on Opus by itself and a note above the message box and in the chat says when the model changed
- Added a Fable limit bar to each account in Claude accounts; the Usage panel also marks where Rich switches to Opus
- Added New Terminal at Folder and New Terminal Tab at Folder when you right-click a folder in RichFiles
- Added pressing Delete on the Mac desktop to move the selected items to the Trash, on by default in RichFiles Settings, Keyboard, once you allow RichFiles Window Gestures in macOS
- Changed RichFiles to open as its own app in its own window instead of inside RichOS: the RichFiles icon on the left shows a page with Open RichFiles and three of its settings or, once you turn on Always open RichFiles in its own window there or in Settings, opens the app right away; Show in RichFiles in the Output panel and Make a video work with that window too
- Changed Backspace in RichFiles to go back to the previous folder; to move files to the Trash, press Delete with Fn or Command or use a separate Forward Delete key
- Changed RichGlance to open a picture smaller than its window at its real size instead of stretching it, to zoom from 5% up to 20 times and to remember its window's size and place
- Improved RichFiles to select a file the moment you press on it and to show Opening folder… while a folder opens
- Fixed Dock gestures and the other RichFiles Window Gestures stopping after an update: the helper you already allowed now starts again by itself, so nothing has to be turned on or allowed again
- Fixed the line in RichFiles Workspaces that says saving or opening a workspace failed being too faint to read easily in light and dark mode
- [technical] Added the Delete shortcut on the Mac desktop to RichDoctor's reports, without any file names
- [technical] Changed the weekly point where Rich moves your team to your next Claude account or pauses it to follow the measured pace when Claude reports a precise reading, up to 99.9% instead of 99%
- [technical] Changed how often RichOS asks Claude for your usage: every 5 minutes for the account in use, every minute only while an account is past 80% and filling fast, every 30 minutes for an added account not in use and every four hours for a free weekly reset offer; Refresh in the Usage panel now shows the latest saved reading
- [technical] Changed Rich's team to use Opus at medium for small design changes such as colors or alignment and Opus at high for routine architecture work such as adding a field or an endpoint, keeping Fable for original design, big strategy and deeper architecture; while Fable is almost used up, new jobs start on the fallback model and a job stopped by the Fable limit carries on from its saved progress

## 1.3.5-nightly.20261120.110

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.5-nightly.20261120.110)

- Added Needs you at the top of your chats, oldest first, for every chat with a question for you, work that stopped or a chat back as you asked; every chat also carries a mark that says Working, Paused, Stopped, Back, New or how many questions are waiting
- Added Bring back later… and Mark as unread when you right-click a chat: Later keeps it out of the way until this evening, tomorrow morning, Monday morning or a time you pick, then puts it back in Needs you; a new question or a problem in it brings it back sooner
- Added Undo for a few seconds after you archive or unpin a chat or bring one back later, also with Command-Z
- Added Mac notifications with sounds while RichOS is in the background when Rich needs you, work stops with a problem, a job that took five minutes or more finishes or a chat comes back as you asked, plus a sound when you pick an answer on a question card; clicking a notification opens that chat and Notifications in Settings turns either off
- Added a line above the message box that says how long your team has been working on that chat or, while the work is paused for your usage limit, about when it continues by itself
- Added Since you were last here in a chat you come back to, with the chat opening right there
- Added Window and Display beside Region when you capture with RichGlance, plus a 3, 5 or 10 second delay and an option to show the pointer; Return copies the picture or, if you choose so in Settings, RichGlance, opens it in the new editor
- Added Scrolling Capture… in RichGlance to turn a page longer than the screen into one long picture to copy, edit or save: scroll the area yourself or let RichGlance scroll down, right or both once you allow it in macOS; when the views cannot be joined exactly, RichGlance says so instead of giving you a broken picture
- Added a screenshot editor in RichGlance with arrows, text callouts, boxes, highlights, numbered steps, redaction and crop in four colors and two sizes, with Undo; Copy Image and Save PNG… make one flat picture in which whatever is under a redaction is gone
- Added Recent Captures in RichGlance, off until you turn it on in Settings, RichGlance or in its own window: it keeps your screenshots and their marks on this Mac for seven days so you can open one and change it again, also with Edit Last Capture or the Edit… button on the Copied note; turning it off deletes them
- Changed a reply to your message that was cut off by RichOS quitting, restarting or crashing: Rich now picks it back up by himself after checking what was already done, with a note above the message box; after three tries RichOS stops and tells you
- Changed the chat list to show all of a company's chats at once, without Show more; an archived chat now comes back when something new happens in it
- Changed updates to replace RichOS in your Applications folder, where you open it, instead of keeping the newer version in a second copy in your home folder; a second copy you already have is tidied up the next time you quit and, if macOS asks for your permission first, Settings, Updates says so and Try again finishes the update once you allow it
- Improved the splash screen to start sooner after you click RichOS in the Dock
- Improved the microphone level in Make a video in light mode: the bars that are not lit are a much lighter gray, so the level stands out
- Fixed the splash screen sometimes staying blank or playing a second time from the start; it now plays once to the end
- Fixed Make a video leaving out RichFiles when it was open inside RichOS and RichCleaner's own window when it was in front
- Fixed the name that appears when you point at a button on the left-hand bar being hidden behind RichFiles
- Fixed System-wide window gestures doing nothing on the first click of an app's Dock icon after the Dock had brought that app's window back by itself
- Fixed the gray descriptions in RichFiles Settings being too faint to read easily in light and dark mode
- Fixed Keep chat alive sometimes holding up your team's work in that chat when its check never got an answer
- Fixed meeting notes getting stuck on one meeting when RichOS could not save its progress, which kept the notes of later meetings from being written
- [technical] Added a second line under a chat's mark in technical mode naming who is working on it and on which model, why it stopped or which usage limit paused it
- [technical] Added start-up timings, splash screen events, update installation outcomes, how long Make a video took to record RichFiles and why a RichGlance scrolling capture could not finish to RichDoctor's reports; Bust a bug reports now also carry RichCleaner recording timings and chat list and notification outcomes, without anything from your chats
- [technical] Changed Rich's team to give product direction for a feature, such as what it should do, for whom and why, to Fable at medium and to check an answer for blind spots on the same model and effort the answer ran on
- [technical] Changed the Codex review of your team's work to run on GPT-6.2 Sol at high effort
- [technical] Fixed rare cases where your team's work could stall or lose track of something: a teammate whose refused start could still be retried being withdrawn when the job finished, a damaged record in one chat stopping new teammates in every other chat, a rewritten assignment after the first one was refused being unable to finish, the Output panel missing files a job had already added to one repository when adding them to the next had to be retried, a review verdict or home picture milestone being lost when passing it on failed, cleanup at the end of a reply stopping when it could not open its archive and a running review being treated as finished
- [technical] Fixed Rich missing a follow-up he was waiting to pick up after a brief read failure and keeping an older version of an onboarding note after the same note had been restored many times
- [technical] Fixed Rich's check that his pushes are not failing on GitHub judging an older push after a newer one from another copy of the repository; it also no longer forgets a push repeated with nothing new or made from a copy that was later removed
- [technical] Fixed reviews of Codex handovers dropping one of two jobs handed over at the same commit, counting other jobs' failed tries against a new one or reviewing an older version of a handover whose instructions arrived late
- [technical] Fixed low-disk cleanup skipping unused iPhone simulators when Xcode is installed outside the Applications folder and removing a Rust build folder when it could not tell whether a build was still using it

## 1.3.4-nightly.20261118.107

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.4-nightly.20261118.107)

- Added Capture Region… to RichGlance's File menu: it freezes the screen under your pointer so you can drag out an exact area with a magnifier and its size in pixels, move a corner one pixel at a time with the arrow keys, then press Return to copy it as a picture or choose Save PNG… to keep it as a file; Escape goes back one step at a time
- Added RichGlance in Settings with Capture from any app, off until you turn it on: while RichOS is running, even minimized or closed, Control-Shift-C or a shortcut you choose starts a capture from any app
- Added System-wide window gestures in RichFiles Settings, both off until you turn them on: double-click any app's title bar to make its window fill the screen or put it back as it was and click the front app's Dock icon to minimize its window or bring it back; once you allow them in macOS they keep working after you quit RichOS and RichFiles
- Added a short Updated to RichOS note beside Settings once an update is in place, with See what's new
- Improved updating so you can follow it to the end: a small button beside Settings shows the download and the check that follows even while Settings is closed, the Updates row shows each step with a running clock and a short countdown appears before RichOS restarts
- [technical] Added update timings, RichGlance captures and the window gestures to RichDoctor's reports, without anything on your screen or any window titles

## 1.3.4-nightly.20261117.104

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.4-nightly.20261117.104)

- Added RichGlance, an image viewer that comes with RichFiles: right-click a picture in RichFiles or Finder and choose Open With, then use the arrow keys to go through the pictures in that folder, Control and scroll to zoom, drag to move around and Escape to close; GIF and WebP pictures play and File, Open in Preview opens the picture for editing
- Added an info button in RichGlance that shows the note, keywords, folder and date RichCleaner saved for a picture: point at it to take a look, click it or press I to keep it showing, with Show by default and a shortcut you can change
- Added Default image viewer in RichFiles Settings: one switch makes RichGlance open your pictures everywhere on your Mac, in Finder and on the Desktop too; turning it off brings back the apps you had before
- Changed Rich's team to skip the design mockup it makes before building a new or changed screen when you tell it you approve the design without seeing it
- Fixed Make a video cutting the first letter off each wrapped line and drawing text that was not on screen, such as hidden text under the message box's hint or what a closed list holds
- Fixed the Max warning in Settings vanishing as it opened when you chose Max again right after closing it
- [technical] Added a launch log that keeps the timings of every start of RichOS, including starts from the Dock or Finder
- [technical] Added RichGlance's timings, without file names or notes, to RichDoctor's reports
- [technical] Changed Rich's team to start every teammate on a model and an effort level picked for the kind of job, such as Opus at high for everyday coding and Fable at medium for hard problems, never at Max
- [technical] Changed Rich's team to drop a review finding no user would notice instead of filing it as a follow-up

## 1.3.4-nightly.20261117.102

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.4-nightly.20261117.102)

- Added Keep chat alive at the top of every chat, off until you check it: while your Mac is on, it keeps that chat ready so Rich answers right away when you come back hours later or the next morning, for a small hourly cost to your Claude Code quota
- Added Context meter in Settings, off by default: it shows at the top of each chat how full Rich's context window is when he talks with you and when he directs your team, such as "Rich 50% 500k/1000k"
- Added an effort level under each of Rich's models in Settings, now called Rich's model and effort: five steps from Low to Max, kept separately for Opus and Fable, with the recommended level marked and a warning if you pick Max; a change also reaches chats that are already running
- Added follow-ups that wait for something: when you ask Rich to tell you or do something once his team has finished a job or you have answered one of his questions, he now picks it up in that chat by himself when that happens
- Changed Rich to run on Fable by default both when he talks with you and when he directs your team; if you were on Opus you are now on Fable too and Opus is still one press away in Settings
- Changed every splash screen to play from start to finish: a click or key no longer skips it, holding Space no longer pauses it and the light that followed the mouse pointer is gone
- Changed your company's name to appear only on images you save from Your openings, no longer on the opening itself
- Improved opening RichFiles the first time: RichOS gets it ready in the background after starting and a click before then shows Opening RichFiles… at once instead of seeming to do nothing
- Improved Rich in a long chat to follow your latest Your instructions once that chat has been compacted, instead of only from your next new chat
- Fixed Make a video recordings whose picture and sound stopped partway through, often after about 15 seconds
- Fixed the home screen picture showing up half built or already finished after the splash screen; it now builds itself in front of you once the splash is gone

## 1.3.4-nightly.20261115.101

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.4-nightly.20261115.101)

- Added Make a video on every screen besides the home screen, at the top right of Settings: it records RichOS as you use it, text you highlight included, with or without your voice and the mouse pointer until you press the red Stop beside the settings button, then lets you watch it and save it as a video file
- Added a Git tab to the Output panel: it shows the history of the repositories connected to the chat's company, either just the changes Rich's team made from this chat or the whole history, with each change shown before and after side by side; View Changes on a text file the team changed opens it the same way
- Added a Git button to RichFiles that opens the same history and side-by-side changes for the repository of the folder you are in
- Added Filter by filename above the list of a chat's files in the Output panel
- Added Rich's team reading any website in the browser you are already signed in to, such as your store's admin or a report behind your sign-in: it opens the page in the background, presses only what shows more of the page, such as a tab or a closed panel, never anything that sends, saves, buys, deletes or posts and closes only the tabs it opened
- Fixed RichCleaner's own window not closing with its red close button; Escape now closes it too
- Fixed turning on your voice in Make a video making RichOS think a call had started and offer to take notes
- [technical] Added the Git view's timings to RichDoctor's reports, alongside RichFiles' other timings

## 1.3.3-nightly.20261115.100

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.3-nightly.20261115.100)

- Added Your openings in Settings, under Splash screen: a gallery of the opening screens you have, with more earned as you use RichOS, such as for the AI power you harness and your months with Rich; play any of them, make one the opening RichOS starts with or have a different one each day, save one as a picture in your Downloads folder and add your company's name in gold to every opening
- Added stages to the home screen picture as Rich learns more, from One Light at the first memory to Self-Made at 2,500: each stage lights up a new part of the picture, a line says how far the next stage is, new lights arrive with how many are new since you last looked, Watch it grow plays your picture from its first light and Rich mentions a new stage once in his next reply
- Added Make a video at the top right of the home screen: record the picture with or without your voice and the mouse pointer, stopping when you choose or after Watch it grow plays once, then watch it and save it as a video file
- Changed the splash screen to show at every start, since it now covers RichOS starting up; its switch in Settings now turns off the splash screen's new two-note sound instead
- Changed two of Rich's team members of the same kind working at once to show as, for example, Mark and Mark2 instead of both as Mark
- Improved starting RichOS: the splash screen now appears almost the moment you click RichOS and an update you downloaded is put in place when you quit instead of holding up your next start
- Fixed the setup screen appearing after every update: RichOS now installs its new engine by itself while you keep working and anything already running carries on
- Fixed what you told Rich during onboarding and what his team found then not reaching his memory of you and your companies; it is now added by itself and shows on the home screen picture without a restart
- Fixed the home screen showing the example instead of the picture of your connected repositories while RichOS's engine was being installed, being replaced or had been opened in Finder; that last case also made RichOS download the engine again

## 1.3.2-nightly.20261114.99

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.2-nightly.20261114.99)

- Added RichFiles on the navigation rail, a file browser with a folder tree, the full path and full file names, tabs, Windows or Mac keyboard shortcuts, Compare Folders, Find Duplicate Files and Disk Usage; Undock RichFiles opens it as its own app and Show in RichFiles in a file's menu in the Output panel opens that file's folder
- Added the five tools that come with RichOS to a new user's first screen, each saying what it does when you point at it; while Rich reads up on you a card offers Turn on RichScribe, which asks for its key, your microphone and Accessibility right in the card; when he comes back with his questions, Talk now lets you answer out loud
- Added Rich's team reading your LinkedIn profile and the LinkedIn pages of the companies on it in the browser you are already signed in to: it opens them in the background, reads them with the Accessibility permission RichScribe uses and closes only the tabs it opened
- Added moments to Rich's answers about a video or recording: each shows a time and the words said there and pressing one plays it from that moment in the Output panel; a YouTube video he watched is saved in Downloads, RichOS Videos so it can play there
- Added Source links to meeting notes: every decision, commitment and open question ends with the time it was said and pressing it opens the transcript in the Output panel at that passage; the notes now start with the decisions
- Added asking Rich about a call while RichBrief is still recording it: in that call's conversation he can read about the last minute of what was said
- Added more ways to ask Rich about past meetings: draft a follow-up email you can copy, see who owes what across your meetings, spot what could catch you out in an upcoming meeting and check whether a draft rests on a decision that later changed
- Added a notice in the conversation when your Google or Microsoft calendar is disconnected, with a Reconnect button; meetings on that account stop prompting from the calendar until you reconnect and when your browser cannot open RichOS shows the sign-in address to open yourself
- Added filing several Desktop files at once in RichCleaner: select them, say one note and they move together into one folder, each with its own Undo; Rich can show you the set again later
- Added RichOS cleaning up its own leftovers by itself when your Mac's disk drops below 15% free, checking every 15 minutes while it is open; before, it cleaned up only around starting and quitting
- Added a SwearMeter button on the navigation rail while Show SwearMeter is on: point at it to see the last 30 days, a daily average, all time and each month; click it to keep the card open
- Added Space to open Talk to Rich from the home screen, as Enter already did
- Changed onboarding to four quick inputs (your name, your gender, your website and your public profiles): Rich's team reads up while you get on with your day, then he comes back by himself with his read of your business, only the questions nobody else can answer and a finished piece of work as a file; a later company asks only for its website
- Changed Rich to tell you plainly what he or his team could not read or reach and work with what was found, instead of asking you to sign in, export, paste or connect it
- Changed SwearMeter to also count, on your Mac, what you typed in Claude Code and Codex; the home screen now shows the last 30 days and Settings shows all history, the last 30 days and each month
- Changed the home screen's picture to show your own work from the repositories connected to your companies: their documents, numbered decisions and days with commits; without a connected repository it still comes from company memory
- Improved meeting notes to arrive sooner after a call ends: RichBrief now transcribes the call while it is still going on
- Improved RichCleaner to file into a folder you already have, starting with the ones it used recently; when it cannot pick one it tells you what to do
- Fixed RichScribe dropping words in longer dictations and adding words you never said, such as "Thank you." at the end
- Fixed RichScribe's Live preview going blank or stalling after a slow update and taking a long time to show new words after a pause
- Fixed opening a filed original kept in iCloud failing with a message to try again later; RichOS now downloads it, shows Downloading from iCloud… and opens it
- Fixed Rich sometimes answering a false internal warning on screen right after he saved your notes or instructions
- [technical] Added RichFiles' timings, without file names, to RichDoctor's reports and recent moment playback results to Bust a bug reports
- [technical] Added build output in repositories connected to RichOS to its cleanup, removed only when no build uses it and nothing in it changed recently; a drive holding the shared build caches running short also starts the cleanup
- [technical] Changed Dean and the 12 ready-made teammates that ran on Sonnet to run on Opus, so every ready-made teammate now does
- [technical] Changed what a teammate gets when Claude Code would stop to ask permission for its command: Claude Code's own reason and a prompt to do it a simpler way, in place of an error from RichOS and a separate check on delete commands
- [technical] Fixed removing a finished teammate's branch getting stuck on a lock Git left behind; a removal that keeps failing is now reported to Rich

## 1.3.1-nightly.20261110.95

[GitHub release](https://github.com/RichSpark/RichOS/releases/tag/v1.3.1-nightly.20261110.95)

- Added Search folders to Settings: choose Off, Chosen folders or All eligible, tick folders in your home folder or on a connected drive, see how many documents RichFinder covers and press Reindex; system folders, app data, hidden folders and code dependencies are never searched and a change works on your next search without a restart
- Added Rich in each company to Settings, Search folders: limit which of your search folders Rich can search in that company's conversations; Rich can also search only the folders you name
- Added a Sources list beside RichFinder's results: uncheck a folder to leave it out of this search only, see which folders could not be searched because they are unavailable and open Search folders from there
- Changed the Super Finder checkbox and status line in RichFinder to say RichFinder
- Fixed choosing a word-for-word match from a reply Rich gave in several parts opening only the conversation; it now takes you to the part with the match

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
