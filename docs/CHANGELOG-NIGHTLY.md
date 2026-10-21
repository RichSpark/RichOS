# Nightly changelog

What changed in each RichOS nightly build from 1.2.1-nightly.20261019.67 on, newest first; stable versions are in the [changelog](CHANGELOG.md).

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
