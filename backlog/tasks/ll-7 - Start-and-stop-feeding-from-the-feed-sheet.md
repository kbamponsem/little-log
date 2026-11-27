---
id: LL-7
title: Start and stop feeding from the feed sheet
status: In Progress
assignee:
  - '@kwabena'
created_date: '2026-11-27T09:16:15Z'
updated_date: '2026-11-27T14:26:08Z'
labels: []
dependencies:
  - LL-6
references:
  - web/src/components/sheets/ll-feed-sheet.ts
  - web/src/components/screens/ll-today.ts
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The feed sheet only logs a finished feed: pick a kind, set minutes or millilitres, choose When, save. It cannot show a feed in progress or stop one later. Core now carries running feed sessions (LL-6), so this is the app side: start a session, show it running, stop it, and keep the quick-log path for a past feed. The running feed also has to live outside the app: while the app is backgrounded, an ongoing Android notification shows a live chronometer from the start time, and its Stop action ends the feed without reopening the app. That stop must be authored through the core event log rather than written straight to SQLite, or it will not replicate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The feed sheet offers Start feeding when no feed is running, and Stop feeding with elapsed time when one is
- [x] #2 Starting a second feed while one is running is refused with a clear message rather than silently replacing it
- [x] #3 A running feed is visible on Today with elapsed time, and stopping it records the end time and shows the duration
- [ ] #4 Logging a completed feed in the past still works with kind, amount, When and notes
- [ ] #5 A running feed survives closing and reopening the app, and can be stopped afterwards
- [x] #6 While a feed runs and the app is backgrounded, an ongoing notification shows a live chronometer counting from the feed start
- [x] #7 The notification has a Stop action that ends the feed without opening the app and authors the stop through the core event log so it replicates
- [x] #8 The notification appears when the app is backgrounded with a feed already running and disappears once the feed stops
- [x] #9 Android 13+ notification permission is requested, and a denied permission leaves feeding working without a crash
- [ ] #10 Manual Android check on emulator-5554: start, background, see the chronometer, stop from the notification, reopen and confirm one entry with the right duration
- [x] #11 web build succeeds, and cargo test --workspace plus cargo clippy --workspace --all-targets -- -D warnings pass
- [x] #12 While a feed is running, the Today feed button shows a live h:mm:ss clock with an animated hourglass
- [x] #13 After a stop from the notification, an app that is already foreground stops showing the running feed without a restart
- [x] #14 The notification body shows the activity label and the elapsed time at large size, with the Stop action
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Web: add store.runningFeed(); give the feed sheet a start/stop mode that keeps the existing completed-feed form; show the running feed on Today and in entry details.\n2. Android: add a FeedBridge JNI pair in Rust (running start time, stop) that opens the same SQLite file and authors the stop with record_new_entry so it is an Edit event, not a direct write.\n3. Kotlin: a FeedTimer helper builds an ongoing notification with setUsesChronometer(true) and a Stop BroadcastReceiver; MainActivity syncs it in onStop so leaving the app shows the timer; create the channel and request POST_NOTIFICATIONS on Android 13+.\n4. Manifest: add the permission and the non-exported receiver.\n5. Verify: npm build, cargo tests/clippy, then install on emulator-5554 and exercise start/background/chronometer/stop/reopen.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the web side and the Android timer.

Web: store.runningFeed(); the feed sheet has a live mode (kind + ticking elapsed + Stop feeding) and an idle mode (Start feeding plus the original Log feeding now form); Today shows Stop feeding with elapsed time while running; entry rows show "feeding now" or the derived duration; CSV details match.

Android: FeedBridge JNI (running start, stop) in app/src-tauri/src/widget.rs; the stop authors an Edit through record_new_entry, so it replicates. FeedTimer posts an ongoing notification with setUsesChronometer(true) and a Stop broadcast action; MainActivity syncs it in onStop; manifest has POST_NOTIFICATIONS and the non-exported receiver; Android 13+ permission is requested on start.

Migration: v12 pinned old feeds at their start, v13 gives them a plausible end (breast feeds use the recorded minutes; millilitre feeds get ten minutes) because the emulator showed every historical feed as "Feeding for 912h".

Verified on emulator-5554 with screenshots: idle sheet shows Start feeding; running sheet shows Breast left + elapsed + Stop feeding; Today shows Stop feeding; notification shows a live chronometer and a Stop action; tapping Stop left exactly one feed row with ended_at and a 69 s duration, removed the notification, and refreshed last-feed; with POST_NOTIFICATIONS revoked the feed still started and stopped and no notification was posted, with no crash; migration reached user_version 13 with 15 min and 10 min derived ends.

Not verified: logging a completed past feed in this build, and a clean cold start showing the running feed. After a force-stop/relaunch the emulator began ANR-ing with "No response to onStartJob" from WorkManager (the Glance/widget worker, not this change); the app never got past the splash, so those two paths need a retest on a healthy emulator.

Follow-up on the notification Stop report.

Diagnosed three separate problems: (1) the Stop action is collapsed behind the notification chevron, so it looks missing; (2) tapping Stop while the app is foreground wrote the event but the WebView never refreshed, so the feed still looked running; (3) the receiver did SQLite and Glance work on the main thread, which produced a WorkManager "No response to onStartJob" ANR right after a stop.

Fixes: the notification body now carries the "feed" action, so tapping the notification opens the feed sheet in Stop mode (MainActivity passes it through pending_action, and focus now re-reads it); focus and a 10 s poll while a feed runs refresh the store; the receiver uses goAsync plus a background thread, keeps the notification if the stop throws, and MainActivity moves the widget refresh off the main thread; the timer tile now shows a live 0:00:00 clock with a flipping hourglass.

Verified: timer tile 0:00:02 -> 0:00:12 with the hourglass mid-flip; tapping the notification body opened "Feeding / Breast left / Stop feeding"; a foreground Stop wrote the event, removed the notification, and the tile reverted to "Feed / Last: 1 min ago" within the poll window; permission-denied path still works.

Still blocked: the emulator (emulator-5554) now ANRs on plain launch with WorkManager "No response to onStartJob" before any interaction, so the post-threading-fix Stop could not be re-verified, and the earlier unchecked paths (completed past feed, cold start with a running feed) remain unverified.

Notification UI polished: a custom RemoteViews body (res/layout/notification_feeding.xml) draws the activity at 22sp and a live Chronometer at 28sp, baseline-aligned; the Chronometer base is translated from wall clock to elapsedRealtime. The small icon is a new hourglass drawable tinted into a green badge, the header timestamp is suppressed so the body is the only clock, copy is "Tap to finish", and the Stop action remains. Tapping the body opens the feed sheet in Stop mode.\n\nThe home-screen Glance widget got more air: 14dp outer padding, 8dp gaps between cards, 4dp inner horizontal padding and 6dp between icon and label.\n\nReplaced the 10 s full refresh with AppStore.refreshIfFeedStopped: it re-reads entries and only notifies when the running feed actually ended, so the screen no longer re-renders every tick.\n\nVerified with screenshots: collapsed notification shows the green hourglass badge, large Feeding and 02:54; expanded shows the same plus Stop. Stop from the notification ended the feed, removed the notification and produced no ANR in the final build. Widget spacing verified on the home screen.
<!-- SECTION:NOTES:END -->
