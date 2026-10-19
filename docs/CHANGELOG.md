# Changelog

What changed in each stable version of RichOS, newest first; nightly builds are in the [nightly changelog](CHANGELOG-NIGHTLY.md).

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
