---
id: LL-11
title: Sync with peers in the background on Android
status: To Do
assignee: []
created_date: '2026-11-27T09:18:27Z'
labels: []
dependencies: []
references:
  - app/src-tauri/src/sync.rs
  - >-
    app/src-tauri/gen/android/app/src/main/java/app/littlelog/widget/WidgetDrainWorker.kt
  - PLAN.md
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Sync today only runs while the app process is alive: sync_in_background spawns a round when the app opens and after something is logged, and the Settings note says so. If both phones are closed nothing replicates, so a parent who logs a feed and pockets the phone does not reach the other device until someone reopens the app. PLAN.md §4 already asks for Android sync on a WorkManager schedule. The widget drain shows the pattern is viable: WorkManager starts the app process with no activity and JNI opens SQLite directly, so a scheduled sync can follow it, but it needs a headless entry point because AppState and the Tauri UI are not there.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A WorkManager job completes a full anti-entropy round with the known peers with no activity or WebView open
- [ ] #2 The schedule is periodic at the platform minimum (15 minutes) and keeps a retry with backoff after a failed round
- [ ] #3 Scheduled work survives the app being killed and the device rebooting
- [ ] #4 The job runs only when a network is available and does not require charging or an unmetered connection
- [ ] #5 A foreground round and a scheduled round never run against the same peer at the same time
- [ ] #6 The last successful background round updates last_synced_at, and Settings shows it after reopening the app
- [ ] #7 A failed round is logged and retried without crashing the app process
- [ ] #8 Manual Android check: log on device A, close the app on device B, wait for a scheduled run, reopen B and see the entry without using Sync now
- [ ] #9 The Settings note describes the new schedule instead of saying sync only runs on open and after logging
<!-- AC:END -->
