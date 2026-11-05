# Feature catalog

Candidate features and enhancements for Rusty Start, from a research pass on 2026-11-04. Sources: code audits of the Windows and Linux backends and the UI, the open beta chain ([#21](https://github.com/jav76/rusty-start/issues/21)-[#27](https://github.com/jav76/rusty-start/issues/27)), current launchers (the Windows 11 Start menu, PowerToys Command Palette, Flow Launcher, Open-Shell, KRunner, Kickoff, Albert and others), and the requirements of each publishing channel. Each entry was checked against [AGENTS.md](../AGENTS.md), the decisions in [#6](https://github.com/jav76/rusty-start/issues/6) and the code on `main`. Dropped and merged ideas are listed at the end.

The [roadmap](roadmap.md) places every entry in a milestone. File an entry as an issue on the [project board](https://github.com/users/jav76/projects/3) before starting it, and keep its `Fxx` ID in the issue body so the roadmap stays traceable.

**Priority:** P0 blocks a release candidate (dead trigger, data loss, crash loop, security, legal). P1 is expected by most users before 1.0. P2 is valuable. P3 is nice to have. **Size:** S is a day or two, M up to a week, L several weeks, XL a project of its own.

## Summary

| Theme | Features | P0 | P1 | P2 | P3 |
| :--- | ---: | ---: | ---: | ---: | ---: |
| [Opening the menu and focus](#opening-the-menu-and-focus) | 13 | 3 | 4 | 5 | 1 |
| [Search](#search) | 10 | 0 | 1 | 6 | 3 |
| [Launching, app actions and power](#launching-app-actions-and-power) | 9 | 0 | 2 | 5 | 2 |
| [Browsing, personalization and first run](#browsing-personalization-and-first-run) | 10 | 0 | 3 | 5 | 2 |
| [Keyboard, accessibility and localization](#keyboard-accessibility-and-localization) | 12 | 0 | 4 | 7 | 1 |
| [Linux desktop reach](#linux-desktop-reach) | 5 | 0 | 0 | 2 | 3 |
| [Robustness, performance and diagnostics](#robustness-performance-and-diagnostics) | 14 | 3 | 6 | 3 | 2 |
| [Install, upgrade and distribution](#install-upgrade-and-distribution) | 17 | 1 | 8 | 5 | 3 |
| [Release engineering](#release-engineering) | 19 | 6 | 8 | 4 | 1 |
| [Docs, legal and community](#docs-legal-and-community) | 13 | 2 | 9 | 2 | 0 |
| **Total** | **122** | 15 | 45 | 44 | 18 |

## User-facing features

### Opening the menu and focus

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F20 | Keep the Windows-key hook alive | The Windows key keeps opening Rusty Start after load spikes, resume or a cancelled shutdown. | Windows | M | P0 | v0.2.0-beta.3 |
| F21 | Keep the Meta key working on Plasma when the host is not running | Plasma users never lose both Rusty Start and Kickoff. | Linux | M | P0 | v0.2.0-beta.3 |
| F93 | Re-register the KDE global shortcut after kglobalacceld restarts | Meta keeps working after a Plasma update or kglobalaccel crash. | Linux | S | P0 | v0.2.0-beta.3 |
| F22 | Never leave the menu open without keyboard focus | Typing never lands in the window behind the menu. | Windows | S | P1 | v0.2.0-beta.3 |
| F23 | Accept injected Windows-key taps | PowerToys remaps, KVMs, remote-play tools and the On-Screen Keyboard open Rusty Start. | Windows | S | P1 | v0.2.0-beta.3 |
| F24 | Handle Ctrl+Esc and make the hotkey configurable | Keyboards without a Win key, Win-lock gamers and VM users open the menu with a key they choose. | Windows | S | P1 | v0.2.0-beta.3 |
| F25 | Elevated windows: verify, document and warn | Users know what to expect over Task Manager or admin terminals and do not elevate every app by accident. | Windows | S | P1 | v0.2.0-beta.3 |
| F28 | Detect when the native Start menu opens anyway | A Windows update that breaks suppression shows up in the audit and log. | Windows | S | P2 | v0.2.0-beta.3 |
| F27 | Fall back to Plain Glass when the snapshot is black | No black screen when Win is tapped over a game. | Windows | S | P2 | v0.3.0-beta.2 |
| F114 | Detect conflicting Start replacements | Two menus fighting over Win get explained. | Windows | S | P2 | v0.3.0-beta.2 |
| F116 | Ignore the Windows key in full-screen apps | Gamers and presenters avoid stray opens. | Windows | S | P2 | v1.1.0 |
| F117 | Press-to-record shortcut field | Pick a hotkey without editing files. | Both | S | P3 | v1.1.0 |
| F26 | Windows key over elevated windows (uiAccess or elevated start) | The Windows key works even when an elevated window has focus. | Windows | M | P2 | Later |

<details>
<summary>Scope and evidence</summary>

- **F20 Keep the Windows-key hook alive.** The [PR #25](https://github.com/jav76/rusty-start/pull/25) flush moves off the hook thread before beta.1 (part of the [PR #25](https://github.com/jav76/rusty-start/pull/25) change). This item: the hook thread runs only the hook and posting; broadcasts, WM_SETTINGCHANGE handlers and WinEvent hooks move to another thread (add the rule to AGENTS.md). Raise the hook thread to THREAD_PRIORITY_HIGHEST. Reinstall on PBT_APMRESUMEAUTOMATIC, session unlock and TaskbarCreated. Show the last hook event time in the Setup Audit. Add raw-input removal detection only if hand tests still show silent removal, and disclose it in F87. _Builds on [PR #23](https://github.com/jav76/rusty-start/pull/23), [PR #25](https://github.com/jav76/rusty-start/pull/25), [#33](https://github.com/jav76/rusty-start/issues/33). Evidence: trigger/win32.rs:88-99, 290, 351-366; [PR #25](https://github.com/jav76/rusty-start/pull/25) window_procedure flush_all, FLUSH_TIMEOUT 2 s_
- **F21 Keep the Meta key working on Plasma when the host is not running.** Meta is bound to the host's own kglobalaccel component, so quit, a crash, apt upgrade and apt remove all leave Meta dead. Recommended: give Meta back to the Application Launcher on graceful quit and retake it at startup, plus a systemd user unit with Restart=on-failure that autostart starts. A desktop-file component (Spectacle-style) only if trace numbers stay at or under 75 ms through the per-tap spawn. Record the choice in an ADR amending 0005, with unit and component names in crates/protocol. Verify quit, kill -9, upgrade and remove on Plasma 6. [PR #27](https://github.com/jav76/rusty-start/pull/27) already fixes README:92-93. _Builds on [#38](https://github.com/jav76/rusty-start/issues/38), [PR #24](https://github.com/jav76/rusty-start/pull/24). Evidence: setup/linux.rs:156-183; trigger/linux.rs:92-121; packaging/linux/*.service_
- **F93 Re-register the KDE global shortcut after kglobalacceld restarts.** Watch NameOwnerChanged for kglobalaccel and re-run doRegister and setActive when it reappears; retry with backoff when the signal stream ends. Verify by restarting kglobalacceld. _Builds on new (Linux counterpart of [#33](https://github.com/jav76/rusty-start/issues/33)). Evidence: trigger/linux.rs listen_for_global_shortcut registers once and bails_
- **F22 Never leave the menu open without keyboard focus.** When SetForegroundWindow is refused, retry once after a no-op SendInput tagged with F23's dwExtraInfo. If that fails too, dismiss at once and log. Add the external-toggler case to the QA matrix. _Depends on F23. Evidence: menu.rs:404-416; backdrop/win32.rs:212-224; protocol/src/lib.rs:44-48_
- **F23 Accept injected Windows-key taps.** Tag every host SendInput with one private dwExtraInfo constant and ignore only those events; feed other injected events to the chord. Unit-test the filter in the platform-independent chord module. _Builds on [PR #23](https://github.com/jav76/rusty-start/pull/23) (extends). Evidence: trigger/win32.rs:376-377, 413-424_
- **F24 Handle Ctrl+Esc and make the hotkey configurable.** Ctrl+Esc toggles when use_win_key is on. Add a flat hotkey key ('ctrl+alt+space' default, 'none' or another chord) parsed in pure tested code, re-registered on change, with the Setup Audit reporting a taken hotkey; this lets AltGr layouts move off Ctrl+Alt+Space. List both shortcuts in the README. The press-to-record field moved to F117. _Evidence: trigger/win32.rs:133-161, 294-306; config.rs:187-194_
- **F25 Elevated windows: verify, document and warn.** Verify by hand that Win over an elevated foreground window opens the native menu while Ctrl+Alt+Space works. Document it in README known limitations and F89, and warn in the Setup Audit when the host itself runs elevated (TokenElevation). De-elevated launching moved to F26. _Evidence: trigger/win32.rs:371-404, 427; apps/win32.rs:234-248; README.md:76-77_
- **F28 Detect when the native Start menu opens anyway.** Log when SendInput inserts fewer than 2 events. Watch EVENT_SYSTEM_FOREGROUND for StartMenuExperienceHost for about 500 ms after a toggle, using an out-of-context WinEvent hook on its own thread. Count failures in the Setup Audit and log the Windows build. _Depends on F20. Evidence: trigger/win32.rs:391-396, 427; ADR 0002_
- **F27 Fall back to Plain Glass when the snapshot is black.** First verify in the QA matrix with a full-screen flip-model game. If the GDI snapshot is black, sample a fixed small set of pixels (bounded work) and open as Plain Glass. The ignore-in-full-screen setting moved to F116. _Evidence: backdrop/win32.rs:52-92_
- **F114 Detect conflicting Start replacements.** A Setup Audit row detecting Open-Shell, StartAllBack, Start11 and ExplorerPatcher, with an explanation of the conflict. _Evidence: release-completeness missing item_
- **F116 Ignore the Windows key in full-screen apps.** win_key_in_fullscreen = open | ignore, with foreground state from a WinEvent hook on a thread other than the hook thread. _Builds on split from F27. Depends on F20. Evidence: trigger/win32.rs:391-396_
- **F117 Press-to-record shortcut field.** A Settings field with a conflict message; on Plasma the row shows the binding and opens kcm_keys. _Builds on split from F24. Depends on F24. Evidence: settings_page.slint:148-183_
- **F26 Windows key over elevated windows (uiAccess or elevated start).** Either a signed uiAccess build in a secure location (needs a per-machine install), or an elevated scheduled-task start that launches apps de-elevated through the desktop IShellDispatch2 on a COM-initialized background thread. Record the choice in ADR 0008. _Depends on F25, F48, F66, F67. Evidence: README.md:76-77; uiAccess requirements_

</details>

### Search

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F01 | Fold diacritics in search and sort All apps by the folded key | French, German, Spanish and Polish users find accented app names by typing plain letters. | Both | S | P1 | v0.3.0-beta.1 |
| F03 | Match Linux apps by program name and untranslated name | nautilus, kgx and gnome-disks find Files, Console and Disks, and English names match on localized desktops. | Linux | S | P2 | v0.3.0-beta.1 |
| F02 | Typo-tolerant search tier below Substring | Typos and shorthand still find the app, and loose matches never beat exact ones. | Both | M | P2 | v1.1.0 |
| F04 | Remember picks per query, and aliases | 'settings', 'cmd' or 'ff' land on the same app every time. | Both | M | P2 | v1.1.0 |
| F05 | Known folders and places as results | Typing 'downloads' opens Downloads. | Both | M | P2 | v1.1.0 |
| F08 | User-defined commands | Scripts, quicklinks and folders in the menu without a plugin runtime. | Both | M | P2 | v1.1.0 |
| F07 | Run command for paths, URLs and command lines | Paste a path, share or settings URI and open it. | Both | M | P2 | Later |
| F06 | Inline calculator and unit conversion | Quick sums inline with nothing leaving the machine. | Both | M | P3 | Later |
| F09 | Search and switch to open windows | Switch windows by typing. | Both | L | P3 | Later |
| F10 | Use Everything for file results when installed | Whole-disk file results for Everything users. | Windows | M | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F01 Fold diacritics in search and sort All apps by the folded key.** Fold name, compact name, words, initials and keywords at index time (NFD plus stripping combining marks, and a small table for ß, æ, ø, ł, đ, œ, ı). Fold the query once per keystroke. Sort All apps by the folded key in Index::new, so É sorts with E on both platforms; there is no CompareStringEx or Linux collator, so search.rs stays platform-independent. Display names are unchanged. Folded queries join the budget keystrokes. _Evidence: crates/host/src/search.rs:41-53, 129-130, 147-153; app_list.rs:520-537_
- **F03 Match Linux apps by program name and untranslated name.** Add the Exec program basename (skipping env, VAR=value, flatpak run and /snap/bin wrappers) and the unlocalized Name and GenericName as keywords, computed in load(). Unit-test the wrapper parsing; update README:139-141. _Builds on extends the [#6](https://github.com/jav76/rusty-start/issues/6) keyword tier. Evidence: apps/linux.rs:77; desktop_entry.rs:59-61, 101-121_
- **F02 Typo-tolerant search tier below Substring.** Add Rank::Fuzzy with two rules: initials followed by a later word's prefix ('vs'+'code'), and bounded Damerau-Levenshtein for queries of 4 or more characters. It uses allocation-free DP with a reused scratch row and runs only for keys no earlier tier matched, so it never outranks another tier. 'vscode', 'notpad' and 'fierfox' join the budget keystrokes. _Evidence: search.rs:18-27, 55-71; apps/win32.rs:258_
- **F04 Remember picks per query, and aliases.** Keep bounded (query prefix, launch ID) pick records in usage.tsv, used only to break ties within a tier. Aliases are usage.tsv records set from an Add alias flyout entry and rank as Exact. Both are hash lookups. Amend ADR 0003. _Depends on F55, F94. Evidence: search.rs:144-163; usage.rs:99-107_
- **F05 Known folders and places as results.** A sync source resolved at startup: Windows known folders plus This PC and Recycle Bin; XDG user dirs plus Home and Trash. Opened through ShellExecute, FileManager1 or xdg-open. The preview backend gets sample places, and the folder list is shared with [#15](https://github.com/jav76/rusty-start/issues/15). An optional folder-button row can follow. _Builds on new ([#15](https://github.com/jav76/rusty-start/issues/15) related). Evidence: Searchable impls apps/mod.rs:52-60, power/mod.rs:161-165_
- **F08 User-defined commands.** Named commands with keywords and run or open targets, in the format F94 chooses (or a separate commands file). {query} becomes exactly one argv element, never passed through a shell. Linux launches get their own systemd scope. _Builds on new ([#13](https://github.com/jav76/rusty-start/issues/13) placeholder rules). Depends on F94. Evidence: config.rs:187-194, 236-265_
- **F07 Run command for paths, URLs and command lines.** Syntactic detection per keystroke, with existence checks on the [#14](https://github.com/jav76/rusty-start/issues/14) lane. '>' runs an argv with no shell interpolation; Ctrl+Shift+Enter elevates. Ranked below apps. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6) (deferred). Depends on [#14](https://github.com/jav76/rusty-start/issues/14). Evidence: app_list.rs:111-122_
- **F06 Inline calculator and unit conversion.** A dependency-free calculator.rs and a static unit table (no currency). It shows a row only for queries with an operator. The decimal separator is read at startup. Out of 1.0, recorded in [#6](https://github.com/jav76/rusty-start/issues/6). _Builds on [#6](https://github.com/jav76/rusty-start/issues/6) (deferred). Evidence: issue [#6](https://github.com/jav76/rusty-start/issues/6) Deferred list_
- **F09 Search and switch to open windows.** Plasma: probe the KWin WindowsRunner via krunner1. Windows: take an EnumWindows snapshot on the async lane thread, skipping cloaked and tool windows. The preview backend lists sample windows. _Depends on [#14](https://github.com/jav76/rusty-start/issues/14). Evidence: KWin windowsrunnerinterface; PowerToys Window Walker_
- **F10 Use Everything for file results when installed.** Query over local IPC probed at runtime and merged with ADR 0007 results. Check the SDK license first; window and message names stay in the Windows backend. _Builds on extends [#15](https://github.com/jav76/rusty-start/issues/15). Depends on [#14](https://github.com/jav76/rusty-start/issues/14), [#15](https://github.com/jav76/rusty-start/issues/15). Evidence: docs/adr/0007_

</details>

### Launching, app actions and power

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F11 | Hand the foreground to launched apps and the UAC prompt | Apps and elevation prompts open in front, on the menu's monitor. | Windows | M | P1 | v0.3.0-beta.1 |
| F13 | Pass xdg-activation tokens to launched apps | Running single-instance apps come to the front with launch feedback, as from Kickoff. | Linux | M | P1 | v0.3.0-beta.1 |
| F12 | Give launched apps the current environment after PATH changes | New PATH entries reach terminals and IDEs launched from the menu without signing out. | Windows | M | P2 | v0.3.0-beta.1 |
| F14 | Choose Windows flyout actions from shell properties | Run as administrator and Open file location appear for Edge, Chrome, VS Code and Terminal. | Windows | M | P2 | v0.3.0-beta.1 |
| F18 | Update and shut down, Update and restart, Hibernate on Windows | Pending updates install at shutdown, as from Start. | Windows | S | P2 | v0.3.0-beta.1 |
| F15 | Manage an app through the Linux software center | Remove or review an app from its flyout, as in Kickoff. | Linux | M | P2 | v1.1.0 |
| F16 | Launch PrefersNonDefaultGPU apps on the discrete GPU | Games launched from the menu use the dedicated GPU. | Linux | S | P2 | v1.1.0 |
| F19 | Recent documents per app in the flyout | Reopen an app's recent file from the menu. | Both | L | P3 | Later |
| F123 | More power actions | The power choices Start and Kickoff show. | Both | M | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F11 Hand the foreground to launched apps and the UAC prompt.** Reproduce first (a cold launch of a large app; Ctrl+Shift+Enter on cmd). The fix: AllowSetForegroundWindow on the UI thread (preferring the child PID via SEE_MASK_NOCLOSEPROCESS over ASFW_ANY). ShellExecuteExW runs on the background thread with the overlay HWND, SEE_MASK_FLAG_LOG_USAGE and SEE_MASK_HMONITOR. Set progress to 0 and make the overlay click-through at once. Hide it when the thread signals or a Rust-side 300 ms cap fires; the UI thread never waits. State close-path numbers. P2 if it does not reproduce. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) / [PR #21](https://github.com/jav76/rusty-start/pull/21), [#31](https://github.com/jav76/rusty-start/issues/31). Depends on [#10](https://github.com/jav76/rusty-start/issues/10). Evidence: menu.rs:507-509, 527; apps/win32.rs:219-248; [PR #21](https://github.com/jav76/rusty-start/pull/21) run_as_administrator_
- **F13 Pass xdg-activation tokens to launched apps.** Outbound only. Before hiding, request a token for the menu surface asynchronously with a capped wait, then pass it as XDG_ACTIVATION_TOKEN for Exec launches and as activation-token/desktop-startup-id in platform_data. Probe xdg_activation_v1 and fall back to today's behavior. Amend ADR 0005 (limitation at 0005:40-41). Using ToggleWithToken's inbound token moves to F60. _Evidence: apps/linux.rs:226-228, 239-250; trigger/linux.rs:141-147; ADR 0005:40-41, 61_
- **F12 Give launched apps the current environment after PATH changes.** On WM_SETTINGCHANGE('Environment'), forwarded to a worker thread rather than the hook thread, rebuild the environment block from HKCU and machine values (REG_EXPAND_SZ expansion, user PATH appended) and apply it. The merge rules are pure and unit-tested; SAFETY comments go on SetEnvironmentVariableW. _Depends on F20. Evidence: trigger/win32.rs:499-503; apps/win32.rs:234-248_
- **F14 Choose Windows flyout actions from shell properties.** At index time read PKEY_AppUserModel_PackageFamilyName, System.Link.TargetParsingPath and the shortcut location, then choose actions from them (no IContextMenu probe). Open file location opens the shortcut's folder. Uninstall opens ms-settings:appsfeatures-app?<PFN> for packaged apps, and packaged apps get App settings. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) / [PR #21](https://github.com/jav76/rusty-start/pull/21) follow-up. Depends on [#10](https://github.com/jav76/rusty-start/issues/10). Evidence: [PR #21](https://github.com/jav76/rusty-start/pull/21) shell_id.rs actions() and test packaged_apps_get_no_run_as_administrator_
- **F18 Update and shut down, Update and restart, Hibernate on Windows.** Through the [#34](https://github.com/jav76/rusty-start/issues/34) decision function: offer Update and shut down/restart via InitiateShutdownW with SHUTDOWN_INSTALL_UPDATES when updates are pending, and Hibernate when GetPwrCapabilities reports HiberFilePresent. The pending flag is read at startup and refreshed by a registry change notification on a worker thread, never on the open path. Switch user and the Linux paths moved to F123. _Builds on [#34](https://github.com/jav76/rusty-start/issues/34) (extends). Depends on [#34](https://github.com/jav76/rusty-start/issues/34). Evidence: power/win32.rs:40-62; power/mod.rs:24-34_
- **F15 Manage an app through the Linux software center.** Linux has no Manage or Uninstall entry today ([PR #21](https://github.com/jav76/rusty-start/pull/21) never offers one). When an x-scheme-handler/appstream handler exists in the loaded MimeType data, offer Manage, which opens appstream://<id>. Parse X-Flatpak at load. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) follow-up. Depends on [#10](https://github.com/jav76/rusty-start/issues/10). Evidence: [PR #21](https://github.com/jav76/rusty-start/pull/21) apps/linux.rs actions(); desktop_entry.rs:63-74_
- **F16 Launch PrefersNonDefaultGPU apps on the discrete GPU.** On the launch thread, read SwitcherooControl GPUs and add the non-default GPU's environment; prefer Exec over D-Bus activation when the key is set. Skip silently if the service is absent. The preview backend logs the chosen environment. _Evidence: desktop_entry.rs:63-74; apps/linux.rs:190-198, 218-237_
- **F19 Recent documents per app in the flyout.** IApplicationDocumentLists on a COM-initialized background thread, and recently-used.xbel filtered by app on Linux. Entries append when ready, and respect the OS recent-items switch. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6) (deferred). Depends on [#10](https://github.com/jav76/rusty-start/issues/10), [#14](https://github.com/jav76/rusty-start/issues/14). Evidence: Flow.Launcher#4136_
- **F123 More power actions.** Switch user (WTSDisconnectSession and SwitchToGreeter), Linux Hibernate, ScreenSaver.Lock before logind, lock_command, and Xfce/LXQt session managers. _Builds on split from F18. Depends on F18. Evidence: power/linux.rs:56-114_

</details>

### Browsing, personalization and first run

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F42 | Hide apps from the menu and search | Remove uninstall, readme and duplicate entries without uninstalling. | Both | M | P1 | v0.3.0-beta.1 |
| F43 | Launch history controls | People who share screens can remove or stop launch history. | Both | M | P1 | v0.3.0-beta.1 |
| F44 | Choose what the empty menu shows | A minimal pins-only menu, the control the Windows 11 redesign removed. | Both | S | P2 | v0.3.0-beta.1 |
| F45 | Rank non-program shortcuts below programs | Vendor clutter stops crowding the list. | Windows | S | P2 | v0.3.0-beta.1 |
| F40 | First-run welcome, consent and autostart prompt | New users see the menu is running and how to use it. | Both | M | P1 | v0.3.0-beta.2 |
| F41 | Keyboard shortcut list | Shortcuts are discoverable without the README. | Both | S | P2 | v0.3.0-beta.2 |
| F47 | Import pins from Start or Kickoff | Keep curated pins when switching. | Both | M | P2 | v1.1.0 |
| F121 | All apps folder rows | All apps grouped like Windows 11 Start. | Windows | M | P2 | v1.1.0 |
| F120 | Glass tooltips for icon buttons | Icon buttons explain themselves. | Both | S | P3 | v1.1.0 |
| F46 | Letter headers in All apps | Landmarks in long lists. | Both | M | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F42 Hide apps from the menu and search.** Add a Hide from list flyout entry and a Hidden apps list on the Settings Page, stored as hide records in usage.tsv (amend ADR 0003). Hidden apps leave every section; search shows them only on an exact name match. .lnk and .desktop files are never edited. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) (flyout entry). Depends on [#10](https://github.com/jav76/rusty-start/issues/10), F55. Evidence: usage.rs:31-36, 107; app_list.rs:90-122_
- **F43 Launch history controls.** Remove from Recent; Clear launch history with a second-press confirm; record_history (default off when Start_TrackProgs = 0, read at startup); show_recent (from F44). Unit-test forget_launch and clear_launches. Reset settings was dropped: the docs say to delete config.toml. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) (flyout entry). Depends on [#10](https://github.com/jav76/rusty-start/issues/10). Evidence: usage.rs:48-72; app_list.rs:19-20, 93-104_
- **F44 Choose what the empty menu shows.** empty_view = pinned-recent-all | pinned-only | all, as a flat key with a Settings control. pinned-only adds an accessible All apps row reachable by Tab and Down. Tests sit next to lists_pinned_then_recent_then_all_apps. show_recent lives in F43. _Evidence: app_list.rs:20, 93-104_
- **F45 Rank non-program shortcuts below programs.** Rank .url, .chm, .txt and uninstaller targets below programs in search and All apps. Folder rows moved to F121. _Evidence: apps/win32.rs:56-92, 250-260_
- **F40 First-run welcome, consent and autostart prompt.** Config::load reports whether config.toml existed. On the first start, open the menu once with a keyboard-dismissable, accessible welcome: how to open it, the F41 shortcut list, an explicit consent step before use_win_key is enabled, and a Start at sign-in button that runs Setup Install off the UI thread (binding Meta on Plasma). The first Card Anchor follows TaskbarAl. _Evidence: main.rs:2, 31-52; config.rs:210-219; Store policy 10.2.8_
- **F41 Keyboard shortcut list.** A Keyboard section on the Settings Page, generated from one Rust table, opened with F1 and reused in the F40 welcome. No rotating hints. Tooltips moved to F120. _Builds on [#36](https://github.com/jav76/rusty-start/issues/36), [PR #21](https://github.com/jav76/rusty-start/pull/21). Evidence: start_menu.slint:395; README.md:121-132_
- **F47 Import pins from Start or Kickoff.** A one-time import off the UI thread: Export-StartLayout mapping on Windows; kactivitymanagerd-statsrc favorites on Plasma (confirm the format on Plasma 6). Pure mapping with fixture tests; order preserved for [#11](https://github.com/jav76/rusty-start/issues/11). _Builds on related to [#11](https://github.com/jav76/rusty-start/issues/11). Depends on [#11](https://github.com/jav76/rusty-start/issues/11). Evidence: Export-StartLayout docs_
- **F121 All apps folder rows.** Record each item's Programs folder, show keyboard-accessible expandable folder rows, and use the folder as secondary text when names collide. _Builds on split from F45. Depends on F45. Evidence: apps/win32.rs:250-260_
- **F120 Glass tooltips for icon buttons.** A tooltip after about 500 ms of hover or on focus, using a declarative delay and translatable strings. _Builds on split from F41. Evidence: start_menu.slint:308-332_
- **F46 Letter headers in All apps.** A Header row per folded first letter, with digits and symbols under '#'. The existing header handling covers selection and scrolling. _Depends on F01. Evidence: app_list.rs:90-109_

</details>

### Keyboard, accessibility and localization

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F30 | Fix Tab focus and make the icon buttons and Power Flyout keyboard-reachable | Tab never swallows typing, and power actions work without a mouse. | Both | S | P1 | v0.2.0-beta.2 |
| F37 | Honor LANGUAGE when picking localized app names | App names match the display language chosen in Plasma. | Linux | S | P1 | v0.2.0-beta.2 |
| F31 | Screen reader support | Screen reader users hear which app Enter will launch. | Both | M | P1 | v0.3.0-beta.2 |
| F32 | Clamp card luminance so text stays readable | Text stays readable over light wallpapers. | Both | M | P1 | v0.3.0-beta.2 |
| F33 | Respect reduced-motion settings | A calm fade for people who turn animations off. | Both | M | P2 | v0.3.0-beta.2 |
| F34 | Opaque card under contrast themes and with transparency off | Low-vision users get an opaque, readable menu that respects their setting. | Both | M | P2 | v0.3.0-beta.2 |
| F38 | Fix the invisible pin hit target on touch | Touch taps launch instead of pinning. | Both | S | P2 | v0.3.0-beta.2 |
| F35 | Follow system text scaling | Larger text settings make larger rows. | Both | M | P2 | v1.1.0 |
| F36 | Translatable UI strings | Non-English users see localized labels. | Both | L | P2 | v1.1.0 |
| F118 | Contrast-theme system colors and portal contrast | Contrast theme colors are respected. | Both | M | P2 | v1.1.0 |
| F119 | Touch long press and a visible flyout button | Touch users reach the App Action Flyout. | Both | S | P2 | v1.1.0 |
| F39 | Alt+1..9 launch the nth visible row | One-chord launches. | Both | S | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F30 Fix Tab focus and make the icon buttons and Power Flyout keyboard-reachable.** Confirm by keyboard first. Set focus-on-tab-navigation and focus-on-click false on key-scope. Make the gear, back and power buttons focusable through GlassButton's FocusScope. Give PowerFlyout Up/Down/Home/End/Enter/Escape with focus returning to the button, and define the apps-page Tab cycle. _Builds on [#36](https://github.com/jav76/rusty-start/issues/36) (split out). Evidence: start_menu.slint:124-165, 265-332, 401-428; power_flyout.slint:18-47; Slint builtin_elements.rs:1405-1409_
- **F37 Honor LANGUAGE when picking localized app names.** Build the preference list from each LANGUAGE element in order, then LC_MESSAGES, unless the locale is C or POSIX. Test: language_list_is_preferred_over_lang. _Evidence: apps/linux.rs:51, 462-467_
- **F31 Screen reader support.** Label the search field. Add a polite live region whose text comes from Rust ('<app>, n of count', 'No matches'), shared with the [#31](https://github.com/jav76/rusty-start/issues/31)/F51 notice component. Add item index and count on rows, a text role on headers, and flyout announcements. Verify with Narrator and Orca, NVDA best effort. _Evidence: start_menu.slint:240-257; menu_row.slint:31-37_
- **F32 Clamp card luminance so text stays readable.** In the existing pass, compress the refracted luminance toward a named ceiling that holds 4.5:1 for primary text, and make hint, placeholder and detail colors opaque [#16](https://github.com/jav76/rusty-start/issues/16) tokens. progress = 0 stays transparent. Unit-test the CPU-side contrast helper and give the preview backend a bright synthetic backdrop. _Builds on [#16](https://github.com/jav76/rusty-start/issues/16) (extends). Depends on [#16](https://github.com/jav76/rusty-start/issues/16). Evidence: glass.rs:822-846; about 2.2:1 over white (evidence critic)_
- **F33 Respect reduced-motion settings.** Read Windows Animation effects (on a worker thread on WM_SETTINGCHANGE) and Plasma's AnimationDurationFactor, plus motion = system|reduced|full. Reduced mode drives an opacity-only ramp from the same progress value through a u_motion uniform. Preview honors the override. State first-frame numbers. _Depends on F20. Evidence: menu.rs:27-29; glass.rs:787-788, 814, 852-856_
- **F34 Opaque card under contrast themes and with transparency off.** Detect Windows contrast themes and Transparency effects off, plus an opaque_card setting on both platforms. In those modes draw the Plain Glass base at opacity 1 with no refraction, rim or sweep, turn the veil to Dim or Off, and add a selection outline. The system color palette and portal contrast moved to F118. _Builds on [#16](https://github.com/jav76/rusty-start/issues/16). Depends on [#16](https://github.com/jav76/rusty-start/issues/16), F20. Evidence: glass.rs:722-724, 871_
- **F38 Fix the invisible pin hit target on touch.** Enable and dimly show the pin target only on the highlighted row, so a tap at the right end of a row launches. Long press and the '...' button moved to F119. _Builds on [#10](https://github.com/jav76/rusty-start/issues/10) / [PR #21](https://github.com/jav76/rusty-start/pull/21). Depends on [#10](https://github.com/jav76/rusty-start/issues/10). Evidence: menu_row.slint:159-185_
- **F35 Follow system text scaling.** First slice: follow the system text scale through one Slint global for fonts and row height, reusing the [#32](https://github.com/jav76/rusty-start/issues/32) clamp. Density and card_size presets come later. Measure the first frame. _Builds on [#32](https://github.com/jav76/rusty-start/issues/32) (extends). Depends on [#32](https://github.com/jav76/rusty-start/issues/32). Evidence: config.rs:17-20; start_menu.slint:20, 181-182_
- **F36 Translatable UI strings.** Ship @tr extraction and bundled gettext catalogs together with the first real translation, routing Rust-side labels through the same catalogs. The IME hand-check moves to F78 now. _Evidence: no @tr in crates/host/ui; build.rs:9-11_
- **F118 Contrast-theme system colors and portal contrast.** Take GetSysColor colors through the [#16](https://github.com/jav76/rusty-start/issues/16) Theme global and read the portal contrast key. Record this in [#6](https://github.com/jav76/rusty-start/issues/6) as an accessibility exception to the light-mode deferral. _Builds on split from F34. Depends on F34. Evidence: no SPI_GETHIGHCONTRAST in crates/_
- **F119 Touch long press and a visible flyout button.** Long press timed in Rust, or an accessible '...' button; 40 px targets. _Builds on split from F38. Depends on F38. Evidence: [PR #21](https://github.com/jav76/rusty-start/pull/21) right-button only_
- **F39 Alt+1..9 launch the nth visible row.** Match Alt only when Control is not held, so AltGr+digit keeps typing. Digit badges appear while Alt is held. _Builds on [#36](https://github.com/jav76/rusty-start/issues/36) follow-up. Evidence: start_menu.slint:130-165_

</details>

### Linux desktop reach

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F56 | Exact non-Plasma setup steps and capability-based Plasma detection | GNOME, Sway and Hyprland users get a working shortcut on the first try. | Linux | S | P2 | v0.2.0-beta.3 |
| F58 | Decide and state Plasma X11 support | X11 users know what to expect. | Linux | S | P2 | v0.3.0-beta.2 |
| F57 | Icon theme on GNOME, Cinnamon and Budgie | The chosen icon theme instead of hicolor. | Linux | S | P3 | Later |
| F59 | Plasma panel widget that toggles Rusty Start | Clicking the panel launcher opens Rusty Start. | Linux | M | P3 | Later |
| F60 | Layer shell, wlroots capture and inbound activation | Sway, Hyprland, niri and COSMIC get a floating overlay and the Fog Veil. | Linux | XL | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F56 Exact non-Plasma setup steps and capability-based Plasma detection.** Print exact Sway, Hyprland and GNOME bindings using rusty-start toggle, plus floating window rules. Gate kglobalaccel and kwinrulesrc edits on org.kde.plasmashell having an owner, not on XDG_CURRENT_DESKTOP, and check NameHasOwner before doRegister. Add OnlyShowIn/NotShowIn to the AGENTS.md exceptions. _Builds on [PR #24](https://github.com/jav76/rusty-start/pull/24). Evidence: setup/linux.rs:33-65, 148-153; trigger/linux.rs:92-100_
- **F58 Decide and state Plasma X11 support.** Log once at startup when WAYLAND_DISPLAY is unset, and list X11 as unsupported in the README and F92, unless X11 is tested and a window type is set. _Evidence: backdrop/linux.rs:47-61; README.md:61-62_
- **F57 Icon theme on GNOME, Cinnamon and Budgie.** Read the theme through the portal Settings ReadOne, then fall back to the files and gtk-4.0/settings.ini. _Builds on [#35](https://github.com/jav76/rusty-start/issues/35). Depends on [#35](https://github.com/jav76/rusty-start/issues/35). Evidence: apps/linux.rs:478-500_
- **F59 Plasma panel widget that toggles Rusty Start.** A QML plasmoid calling Toggle(), with its D-Bus constants generated from crates/protocol or tested against it. Setup can offer to swap it for Kickoff, and undo reverses the swap. _Depends on F64. Evidence: setup/linux.rs:156-181_
- **F60 Layer shell, wlroots capture and inbound activation.** Probe zwlr_layer_shell_v1 behind a custom Slint platform, and ext_image_copy_capture_v1 or screencopy for the snapshot. Also use ToggleWithToken's inbound token through xdg_activation_v1 on the live surface (moved from F13). Needs a new ADR. _Builds on ADRs 0004, 0005 follow-up. Evidence: backdrop/linux.rs:1-11; trigger/linux.rs:141-147_

</details>

### Robustness, performance and diagnostics

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F48 | Embed an app manifest with per-monitor DPI awareness | A sharp, correctly placed menu above 100% scaling. | Windows | S | P0 | v0.2.0-beta.2 |
| F99 | Fail open to the native menu | A hung UI or missing GL never leaves users with no Start menu. | Both | M | P0 | v0.2.0-beta.3 |
| F50 | Restart after crashes | A crash does not quietly hand the Windows key back to the old menu. | Both | S | P1 | v0.2.0-beta.3 |
| F52 | Startup header, backtraces, symbols and log location | Crash reports trace to a line of code. | Both | M | P1 | v0.2.0-beta.3 |
| F53 | Diagnostics report and Open log folder | One-step bug reports. | Both | M | P1 | v0.2.0-beta.3 |
| F94 | Decide how config.toml grows before 1.0 | Settings files keep working when features add tables. | Both | S | P1 | v0.2.0-beta.3 |
| F100 | Crash-loop guard and post-crash notice | Crashes are visible and never loop the keyboard. | Both | S | P1 | v0.2.0-beta.3 |
| F95 | Flush config saves on Linux SIGTERM | A setting changed just before logout is not lost. | Linux | S | P2 | v0.2.0-beta.3 |
| F55 | 1.0 compatibility contract and upgrade fixtures | Pins, history, settings and toggle scripts survive upgrades and downgrades. | Both | S | P0 | v0.3.0-beta.1 |
| F51 | Empty, loading and failure states in the card | Users can tell a typo from a broken install. | Both | M | P1 | v0.3.0-beta.2 |
| F49 | Render icons at the monitor's scale | Crisp icons at 250-300%. | Both | S | P3 | v0.3.0-beta.2 |
| F54 | Idle footprint and sign-in-to-ready budget | Proof the resident host is cheap while hidden. | Both | S | P2 | v1.0.0-rc.1 |
| F122 | Recover from GPU context loss | Driver updates do not leave a blank menu. | Windows | M | P2 | v1.1.0 |
| F98 | Persist rendered icons | The first open after sign-in has icons. | Both | M | P3 | v1.1.0 |

<details>
<summary>Scope and evidence</summary>

- **F48 Embed an app manifest with per-monitor DPI awareness.** Verify with Task Manager's DPI awareness column on a 150% display. Embed a manifest regardless: PerMonitorV2, Windows 10/11 supportedOS, asInvoker, longPathAware (via lld-link under cargo-xwin). Log the awareness at startup. F64 reuses the same resource step. _Builds on [#32](https://github.com/jav76/rusty-start/issues/32) prerequisite. Evidence: main.rs:36-37; backdrop/win32.rs:141-158; winit event_loop.rs:199; build.rs_
- **F99 Fail open to the native menu.** The hook checks a UI-thread heartbeat and passes Win through when the heartbeat is stale or the renderer is unavailable. On Plasma, the host does not keep Meta when its window cannot be created. Record the principle in AGENTS.md and ADR 0008. This is the minimum [#37](https://github.com/jav76/rusty-start/issues/37) implementation. _Builds on [#37](https://github.com/jav76/rusty-start/issues/37). Depends on [#37](https://github.com/jav76/rusty-start/issues/37). Evidence: the hook suppresses Start synchronously (trigger/win32.rs:391-396)_
- **F50 Restart after crashes.** Set panic = 'abort' in the release profile so a background-thread panic ends the process. Call RegisterApplicationRestart with RESTART_NO_REBOOT | RESTART_NO_PATCH so it does not race the Run key. Linux restarts come from the F21 systemd unit. GL context-loss recovery moved to F122. _Builds on [#37](https://github.com/jav76/rusty-start/issues/37), [PR #22](https://github.com/jav76/rusty-start/pull/22). Evidence: main.rs:25-54; Cargo.toml:72-75_
- **F52 Startup header, backtraces, symbols and log location.** Log one startup line: version, commit, target, OS build, desktop, GL renderer strings, monitors. Capture Backtrace::force_capture in the panic hook. Use line-tables-only split debuginfo with a symbols archive per release. --version gains commit and target. Move the log to %LOCALAPPDATA%\RustyStart and $XDG_STATE_HOME/rusty-start before F55 freezes paths. Document WER LocalDumps and coredumpctl. _Builds on [PR #22](https://github.com/jav76/rusty-start/pull/22) (extends). Evidence: Cargo.toml:72-75; [PR #22](https://github.com/jav76/rusty-start/pull/22) describe_panic; config.rs:302-305_
- **F53 Diagnostics report and Open log folder.** rusty-start report, plus Copy diagnostics and Open log folder in the ABOUT row. Contents: version line, renderer, Veil Mode, Setup Audit, config, app count and the last 200 log lines with home paths redacted. Files are read on a background thread. _Builds on [PR #22](https://github.com/jav76/rusty-start/pull/22), [PR #24](https://github.com/jav76/rusty-start/pull/24). Depends on F52. Evidence: .github has no ISSUE_TEMPLATE; [PR #24](https://github.com/jav76/rusty-start/pull/24) ABOUT row_
- **F94 Decide how config.toml grows before 1.0.** An ADR choosing flat keys, separate files or a TOML parser. Recommended: flat keys for 1.0, with a guarantee the file stays valid TOML; lists live in usage.tsv records or a separate file, so a real parser can come later without breaking files. _Builds on [PR #25](https://github.com/jav76/rusty-start/pull/25). Evidence: config.rs:187-194, 236-265_
- **F100 Crash-loop guard and post-crash notice.** A marker file in the state dir, cleared on clean exit. After an abnormal exit, show a notice with Copy diagnostics and an issues-page link (browser only, no data in the URL). After 3 crashes in 10 minutes, start with the hook off and say so. _Depends on F50, F53. Evidence: release-completeness missing item_
- **F95 Flush config saves on Linux SIGTERM.** Handle SIGTERM at sign-out by flushing the 250 ms debounced config writer, matching [PR #25](https://github.com/jav76/rusty-start/pull/25)'s Windows flush. _Builds on [PR #25](https://github.com/jav76/rusty-start/pull/25) follow-up. Evidence: config.rs:30; [PR #25](https://github.com/jav76/rusty-start/pull/25) body_
- **F55 1.0 compatibility contract and upgrade fixtures.** Write a COMPATIBILITY ADR covering config keys and defaults, usage.tsv v1, CLI commands and exit codes, crates/protocol names, the mutex, file locations, and the semver rules. Usage serialize keeps unknown records. A newer version is read-only for the session (the [PR #25](https://github.com/jav76/rusty-start/pull/25) change). Fixture tests cover files from 0.1.0-alpha.1, 0.1.1-alpha.1 and 0.2.0-beta.1. The document is finalized at rc.1 through F113. _Builds on [PR #25](https://github.com/jav76/rusty-start/pull/25). Depends on F52, F94. Evidence: usage.rs:107 (skip) and serialize (drops); [PR #25](https://github.com/jav76/rusty-start/pull/25) body_
- **F51 Empty, loading and failure states in the card.** A status line in the [#31](https://github.com/jav76/rusty-start/issues/31) slot: no matches (with the [#13](https://github.com/jav76/rusty-start/issues/13) Web Fallback row when enabled), Loading apps, Could not read the app list. The Settings Page warns when saving is blocked or config was set aside ([PR #25](https://github.com/jav76/rusty-start/pull/25)). Strings come from Rust and are announced through F31. _Builds on [#31](https://github.com/jav76/rusty-start/issues/31), [#13](https://github.com/jav76/rusty-start/issues/13), [PR #25](https://github.com/jav76/rusty-start/pull/25). Depends on [#31](https://github.com/jav76/rusty-start/issues/31). Evidence: app_list.rs:111-122; menu.rs:88-90, 104-106_
- **F49 Render icons at the monitor's scale.** Render at ceil(24 x the highest monitor scale), pass the scale to IconLookup, add svgz to EXTENSIONS routed to render_svg, and optionally treat a fully transparent render as a failure. Re-render off the UI thread when the scale changes ([#32](https://github.com/jav76/rusty-start/issues/32)). The raster-images part was dropped: it is already enabled. _Builds on [#32](https://github.com/jav76/rusty-start/issues/32), [#35](https://github.com/jav76/rusty-start/issues/35). Depends on F48, [#32](https://github.com/jav76/rusty-start/issues/32). Evidence: apps/mod.rs:35-36; icon_theme.rs:13, 190_
- **F54 Idle footprint and sign-in-to-ready budget.** Measure private working set or RSS, GPU memory and idle wakeups while hidden, sign-in-to-first-open time and D-Bus cold start. Record them per RC in the sign-off and CHANGELOG, and add an idle budget to AGENTS.md. Buffers are kept by default. _Evidence: AGENTS.md Performance_
- **F122 Recover from GPU context loss.** After confirming a robust context is available, treat a lost context as fatal to the window: recreate it with Plain Glass and log the recovery. _Builds on split from F50. Depends on [#37](https://github.com/jav76/rusty-start/issues/37). Evidence: glass.rs:107-156_
- **F98 Persist rendered icons.** An on-disk cache keyed by launch ID, icon source and mtime, loaded off the UI thread and shared with [#35](https://github.com/jav76/rusty-start/issues/35) invalidation. _Builds on [#35](https://github.com/jav76/rusty-start/issues/35). Evidence: apps/win32.rs:94-97_

</details>

## Project and release work

Work users feel indirectly: trustworthy downloads, clean installs and upgrades, and docs.

### Install, upgrade and distribution

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F61 | Version-free folder inside the Windows zip | Autostart survives upgrades. | Windows | S | P1 | v0.2.0-beta.2 |
| F64 | Icon set, Windows version resource and branding | A real name, icon and version in Explorer, Task Manager and package managers. | Both | M | P1 | v0.2.0-beta.3 |
| F62 | Replace an older running host | The new version runs right after an update. | Both | M | P1 | v0.3.0-beta.1 |
| F63 | Upgrade and uninstall steps that leave nothing dead | Uninstalling gives the keyboard back. | Both | S | P1 | v0.3.0-beta.1 |
| F72 | Make updates discoverable without network calls | Users know how to update while the host stays offline. | Both | S | P2 | v0.3.0-beta.1 |
| F67 | Per-user NSIS installer | One-click install, clean upgrades and a real uninstall. | Windows | M | P1 | v1.0.0-rc.1 |
| F106 | Channel coexistence and migration paths | Switching channels keeps pins and autostart. | Both | S | P1 | v1.0.0-rc.1 |
| F65 | AUR packages | Arch-based Plasma users install with their AUR helper. | Linux | S | P2 | v1.0.0-rc.1 |
| F66 | Code-sign Windows binaries from the Linux runner | No Smart App Control blocks or scary SmartScreen warnings. | Windows | L | P0 | v1.0.0 |
| F68 | winget for stable releases | winget install jav76.RustyStart. | Windows | M | P1 | v1.0.0 |
| F69 | Signed apt repository | Updates through apt and Discover. | Linux | M | P1 | v1.0.0 |
| F71 | Scoop bucket | Scoop users update like their other tools. | Windows | S | P3 | v1.0.0 |
| F70 | RPM on releases | Fedora KDE and openSUSE get a native package. | Linux | S | P2 | v1.1.0 |
| F73 | Packaged MSIX mode | The Store build starts at sign-in and keeps its CLI. | Windows | L | P2 | v1.1.0 |
| F74 | Microsoft Store convenience build | The paid, auto-updating, Microsoft-signed build the README promises. | Windows | M | P2 | v1.1.0 |
| F75 | Vendored source tarball for community packagers | Native packages on distros the project does not package. | Linux | M | P3 | Later |
| F76 | Publish the protocol crate | Hotkey tools depend on exact names. | Both | S | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F61 Version-free folder inside the Windows zip.** Use 'Rusty Start/' as the zip's top folder and keep the version only in the archive name. Document 'quit, then extract over'. The Run-value mismatch prompt already exists. _Evidence: release.yml:120-123; run_key.rs:20-25_
- **F64 Icon set, Windows version resource and branding.** An SVG master logo with a generated .ico and hicolor PNGs. Embed the icon and VERSIONINFO through F48's resource step (confirm llvm-rc under cargo-xwin). The icon is reused for Icon= ([#38](https://github.com/jav76/rusty-start/issues/38)), AppStream, the installer and Store. Add Windows screenshots. _Builds on [#38](https://github.com/jav76/rusty-start/issues/38) (Icon=). Depends on F48, F110. Evidence: build.rs; desktop entry lacks Icon=_
- **F62 Replace an older running host.** Send version and path with the second-instance request (a WM_COPYDATA identifier and a D-Bus method defined in crates/protocol, joining F55). A newer binary, or one at another path, sends QuitHost and takes over; add --replace. On Linux, re-exec while hidden when /proc/self/exe is deleted (verify the KWin effect first). _Builds on [PR #24](https://github.com/jav76/rusty-start/pull/24), [#38](https://github.com/jav76/rusty-start/issues/38). Evidence: instance/win32.rs:22-38; protocol/src/lib.rs:52_
- **F63 Upgrade and uninstall steps that leave nothing dead.** README Upgrade and Uninstall sections; TryExec= in the autostart entry; prerm prints a per-user setup --undo notice; the NSIS uninstaller runs setup --undo (F67). _Builds on [#38](https://github.com/jav76/rusty-start/issues/38), [#39](https://github.com/jav76/rusty-start/issues/39). Depends on F21. Evidence: setup/values.rs:104-118; README.md_
- **F72 Make updates discoverable without network calls.** A Release notes button in the ABOUT row that opens the releases page in the browser. A README Updating section per channel. Fix the README text that implies auto-updating builds. _Builds on [PR #24](https://github.com/jav76/rusty-start/pull/24). Evidence: issue [#6](https://github.com/jav76/rusty-start/issues/6); README.md:225_
- **F67 Per-user NSIS installer.** makensis on the runner. Installs to %LOCALAPPDATA%\Programs\Rusty Start with an HKCU Uninstall entry and /S support. Runs quit before replacing files and relaunches after; migrates a Run value that points at an old zip folder; runs setup --undo on uninstall. Creates a Rusty Start Start Menu shortcut (from F29), and the app index excludes the host's own shortcut and AUMID. Unsigned in rc.1, signed from rc.2. The zip stays. _Depends on F62, F63, F64. Evidence: winget-cli#5235; README.md:79-88_
- **F106 Channel coexistence and migration paths.** Define and test zip to NSIS, winget portable to nullsoft, GitHub .deb to the apt repo, and two copies installed at once, with one QA row per move. _Depends on F67, F62. Evidence: release-completeness missing item_
- **F65 AUR packages.** rusty-start-bin from the release tarball, with a post_install hint to run setup. Pushed by github-actions-deploy-aur (SHA-pinned) from the isolated release environment. Tracks stable from 1.0; an rc pkgver is optional. _Builds on [#39](https://github.com/jav76/rusty-start/issues/39), [#29](https://github.com/jav76/rusty-start/issues/29). Depends on [#39](https://github.com/jav76/rusty-start/issues/39), [#29](https://github.com/jav76/rusty-start/issues/29), F85, F80, F90. Evidence: release.yml:64-67_
- **F66 Code-sign Windows binaries from the Linux runner.** Choose the identity in ADR 0008 and start validation in the beta.2 window. Sign the exe, installer and uninstaller with jsign in the bookworm container, with an RFC 3161 timestamp, from rc.2 onward, and verify with osslsigncode. Credentials live only in the protected release environment. Submit each release to Microsoft's false-positive portal until reputation builds. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6) (on hold). Depends on F64, F90, F80, F107. Evidence: README.md:82; release.yml:113-123_
- **F68 winget for stable releases.** First manifest by hand with komac new (nullsoft installer, x64 and arm64). Then release.yml runs komac update --submit for tags without '-', using a PAT kept in the release environment. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6). Depends on F67, F66, F80. Evidence: russellbanks/Komac_
- **F69 Signed apt repository.** A Cloudsmith OSS repo with a stable suite and a testing suite for -beta/-rc tags, a deb822 .sources snippet with Signed-By, and the attribution line. The testing suite runs during the RC window so rc.N exercises an apt upgrade. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6). Depends on [#38](https://github.com/jav76/rusty-start/issues/38), F80. Evidence: Cloudsmith OSS policy_
- **F71 Scoop bucket.** jav76/scoop-bucket updated from release.yml. Document quitting the host before scoop update. Request Extras at 100 stars or 50 forks. _Depends on F61, F80. Evidence: ScoopInstaller templates_
- **F70 RPM on releases.** cargo-generate-rpm with explicit requires, shipped together with a Fedora COPR so there is an update path. Tier 2 until it is tested. _Depends on [#39](https://github.com/jav76/rusty-start/issues/39), [#29](https://github.com/jav76/rusty-start/issues/29), F85. Evidence: crates/host/Cargo.toml:57-73_
- **F73 Packaged MSIX mode.** Detect package identity. Use windows.startupTask for autostart and handle the virtualized config path, an appExecutionAlias and runFullTrust. Test the hook, snapshot and launching inside the package. The decision is recorded in F90 before 1.0. _Depends on F90. Evidence: setup/win32.rs:22, 61; config.rs:303-305_
- **F74 Microsoft Store convenience build.** Free individual Partner Center account; reserve the name. Certification notes cover the opt-in hook (10.2.8) and consent from F40; privacy URL from F87; Sponsors routed per 10.8.2. Submissions through the msstore CLI. _Depends on F73, F87, F64. Evidence: README.md:221-226; Store policies_
- **F75 Vendored source tarball for community packagers.** Attach cargo vendor plus config. Document the 1.92 toolchain floor, nixpkgs patchelf over wrapProgram, and Slint's GPLv3 option. _Depends on [#39](https://github.com/jav76/rusty-start/issues/39). Evidence: Cargo.toml rust-version 1.92_
- **F76 Publish the protocol crate.** Make a stability promise from F55, publish by hand once, then use trusted publishing on stable tags. _Depends on F55. Evidence: crates/protocol/Cargo.toml_

</details>

### Release engineering

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F79 | Gate releases on green CI, protect tags, gate outside PRs | No tagged release can ship failing code. | n/a | S | P0 | v0.2.0-beta.1 |
| F97 | Reconcile [PR #21](https://github.com/jav76/rusty-start/pull/21) with the beta chain's logging lint | beta.1 ships with the flyout its changelog lists. | Both | S | P0 | v0.2.0-beta.1 |
| F77 | Run Windows tests and a smoke check in CI | Windows builds whose tests actually ran on Windows. | Windows | M | P0 | v0.2.0-beta.2 |
| F84 | Release procedure, milestones, exit criteria and severity | Everyone can see what stands between beta and 1.0. | n/a | S | P0 | v0.2.0-beta.2 |
| F80 | Isolate release builds from pull-request code | Binaries are never built on a machine that just ran outside code. | n/a | M | P0 | v0.2.0-beta.3 |
| F103 | Move Windows decision logic into cfg-free tested modules | Windows behavior is tested on every PR. | Windows | M | P1 | v0.2.0-beta.3 |
| F78 | Manual QA matrix and RC sign-off | Integration CI cannot test is checked the same way every RC. | Both | M | P0 | v1.0.0-rc.1 |
| F81 | Smoke-test every release artifact before publishing | Broken packages are caught before download. | Both | M | P1 | v1.0.0-rc.1 |
| F82 | Attestations, immutable releases and SBOM | Users can verify downloads came from this repo's workflow. | Both | M | P1 | v1.0.0-rc.1 |
| F83 | cargo-deny, Dependabot and an MSRV check | The no-network and MSRV promises become checkable. | n/a | S | P1 | v1.0.0-rc.1 |
| F104 | Soak and leak test before each RC | No slow leaks in a resident process. | Both | S | P1 | v1.0.0-rc.1 |
| F105 | Property tests for parsers and ranking; no panics on the hook path | A malformed file never kills the trigger. | Both | S | P1 | v1.0.0-rc.1 |
| F107 | Release secrets and signing key management | A leaked key cannot quietly sign malware. | n/a | S | P1 | v1.0.0-rc.1 |
| F101 | Linux GUI smoke test on a virtual KWin | Blank-window and activation regressions are caught in CI. | Linux | M | P2 | v1.0.0-rc.1 |
| F108 | Antivirus false-positive process | Installs are not quarantined. | Windows | S | P1 | v1.0.0 |
| F102 | Windows UI smoke through UI Automation | CI proves the menu renders on Windows. | Windows | M | P2 | v1.0.0 |
| F111 | Monthly Windows compatibility check | Windows updates that break suppression are found fast. | Windows | S | P2 | v1.0.0 |
| F115 | Reproducible release runner and continuity | A dead runner cannot block a security fix. | n/a | S | P2 | v1.0.0 |
| F124 | Deterministic archives, path remapping and cosign | Reproducible, independently signed archives. | Both | S | P3 | Later |

<details>
<summary>Scope and evidence</summary>

- **F79 Gate releases on green CI, protect tags, gate outside PRs.** Require the 'fmt, clippy, test' check on main. Add a refs/tags/v* ruleset limited to admins. [PR #27](https://github.com/jav76/rusty-start/pull/27)'s create job refuses commits not on main or without a green CI run. Add CODEOWNERS or drop the setting. Set fork-PR approval to all outside contributors (the first F80 step). _Builds on [PR #27](https://github.com/jav76/rusty-start/pull/27). Evidence: ruleset 25547412; fork-pr-contributor-approval first_time_contributors_
- **F97 Reconcile [PR #21](https://github.com/jav76/rusty-start/pull/21) with the beta chain's logging lint.** Rebase [PR #21](https://github.com/jav76/rusty-start/pull/21) onto the chain and convert its 8 eprintln! calls to log macros, or the second merge fails -D warnings. Merge it first so [PR #27](https://github.com/jav76/rusty-start/pull/27)'s changelog stays true. _Builds on [PR #21](https://github.com/jav76/rusty-start/pull/21), [PR #22](https://github.com/jav76/rusty-start/pull/22), [PR #27](https://github.com/jav76/rusty-start/pull/27). Evidence: [PR #22](https://github.com/jav76/rusty-start/pull/22) print_stderr = warn; [PR #21](https://github.com/jav76/rusty-start/pull/21) diff_
- **F77 Run Windows tests and a smoke check in CI.** First recheck the account Actions billing lock (the repo is public). Run clippy and cargo test natively on windows-2025 and windows-11-arm, or on an ephemeral Windows 11 VM limited to main and tags. Interim: cargo xwin test under wine. Add a hidden self-check (config, AppsFolder, icons, Run-key audit) and a start/toggle/quit smoke. No [#37](https://github.com/jav76/rusty-start/issues/37) dependency, since GUI smoke is F102. _Builds on [#37](https://github.com/jav76/rusty-start/issues/37) (GUI smoke only). Evidence: ci.yml:47-55; release.yml:15-17_
- **F84 Release procedure, milestones, exit criteria and severity.** Create the milestones and assign issues. RELEASING.md covers pre-tag checks, verification, post-release channel steps, rc.N numbering, a release/1.0 hotfix branch, withdrawals and frozen asset names. RC exit criteria. Severity definitions (P0 = dead trigger, data loss, crash loop, security), triage labels, a pinned known-issues issue per release, and the in-or-out calls. _Builds on [PR #27](https://github.com/jav76/rusty-start/pull/27), [#28](https://github.com/jav76/rusty-start/issues/28). Evidence: milestones API returns []_
- **F80 Isolate release builds from pull-request code.** Ephemeral or JIT runners recreated per job. Releases get their own label, used only by release.yml, behind a protected environment with a required reviewer that holds every release secret. Check that PR jobs cannot reach the host Docker socket. _Evidence: ci.yml:6, 21; release.yml:7-8_
- **F103 Move Windows decision logic into cfg-free tested modules.** Move Run-key matching, power decisions ([#34](https://github.com/jav76/rusty-start/issues/34)), the chord and injected-event filter (F23), the env merge (F12), hotkey parsing (F24) and action selection (F14) out of win32.rs so Linux CI tests them. _Evidence: no Windows runner today_
- **F78 Manual QA matrix and RC sign-off.** docs/qa/matrix.md and an RC sign-off issue template. Tier 1 rows are required (Windows 11 x64 24H2/25H2; Plasma 6 Wayland on Debian 13 and Kubuntu 26.04; mixed-DPI two monitors). Tier 2 rows are best effort (Windows on ARM with and without the GL pack, RDP or VM, Fedora, Arch). Scenarios include the [PR #21](https://github.com/jav76/rusty-start/pull/21)-[#26](https://github.com/jav76/rusty-start/issues/26) hand checks, triggers, upgrades, uninstall, trace numbers, screen readers, IME, refused foreground, a full-screen game and channel migrations. _Depends on F92. Evidence: PRs [#21](https://github.com/jav76/rusty-start/issues/21)-[#26](https://github.com/jav76/rusty-start/issues/26) 'Not yet checked by hand'_
- **F81 Smoke-test every release artifact before publishing.** A verify job before undrafting. apt install in trixie and ubuntu:24.04 with a --version match; lintian; arm64 under qemu; wine --version (shared with F77); llvm-readobj imports with no VCRUNTIME140 and no winhttp/wininet/ws2_32 connect functions; expected file lists. _Builds on [PR #27](https://github.com/jav76/rusty-start/pull/27), [#38](https://github.com/jav76/rusty-start/issues/38). Evidence: release.yml:115; [PR #27](https://github.com/jav76/rusty-start/pull/27) publish job_
- **F82 Attestations, immutable releases and SBOM.** actions/attest (SHA-pinned) over every asset; immutable releases; cargo-auditable; a CycloneDX SBOM per target; rustc, cargo-xwin and cargo-deb versions in the notes. Document gh attestation verify and the self-hosted provenance limit. Deterministic archives moved to F124. _Builds on [PR #27](https://github.com/jav76/rusty-start/pull/27). Evidence: release.yml:7-8, 52, 103_
- **F83 cargo-deny, Dependabot and an MSRV check.** cargo-deny: advisories, a license allowlist including Slint, bans on HTTP/TLS/socket crates, and crates.io-only sources. Dependabot for cargo and actions; secret scanning. A cargo +1.92 check-only job, with clippy staying on latest stable. _Depends on F80. Evidence: no deny.toml or dependabot.yml; Cargo.toml:8_
- **F104 Soak and leak test before each RC.** 1,000 scripted toggles plus a 24 h resident run, recording private bytes or RSS, GDI and USER handles and GPU memory. Any growth fails the RC. _Evidence: BitBlt snapshots, HICONs, GL textures_
- **F105 Property tests for parsers and ranking; no panics on the hook path.** proptest for desktop entries, index.theme, config and usage, plus ranking invariants. Review the hook procedure so it cannot panic inside extern "system". _Evidence: release-completeness missing item_
- **F107 Release secrets and signing key management.** Scope every key to the protected release environment. Publish the GPG fingerprint, keep a revocation certificate offline, require 2FA, and document rotation and compromise steps in RELEASING.md. _Depends on F80. Evidence: F65, F66, F68, F69 secrets_
- **F101 Linux GUI smoke test on a virtual KWin.** Install the .deb in a Plasma 6 container, run kwin_wayland --virtual with a session bus, toggle, and check rows through AT-SPI. Record first-frame time. _Depends on F31. Evidence: bookworm CI has Plasma 5 only_
- **F108 Antivirus false-positive process.** Scan every RC with Defender and VirusTotal before undrafting, submit false positives to WDSI, and document user steps. _Depends on F66. Evidence: winget validation runs a Defender scan_
- **F102 Windows UI smoke through UI Automation.** On the Windows runner, start the host, toggle through the protocol message, and read the UIA tree for the search field and rows. _Builds on [#37](https://github.com/jav76/rusty-start/issues/37). Depends on F77, [#37](https://github.com/jav76/rusty-start/issues/37). Evidence: hosted runners have only the basic display adapter_
- **F111 Monthly Windows compatibility check.** A scripted check after Patch Tuesday and on Insider Beta (mask, hook, snapshot, focus), with a build compatibility table and F28 counters as the signal. _Depends on F28. Evidence: ADR 0002 (build 26200)_
- **F115 Reproducible release runner and continuity.** Runner provisioning as code, documented restore, and a fallback to hosted runners for release.yml. _Depends on F77, F80. Evidence: one self-hosted runner_
- **F124 Deterministic archives, path remapping and cosign.** SOURCE_DATE_EPOCH, sorted tar and zip -X, --remap-path-prefix, and keyless cosign over SHA256SUMS. _Builds on split from F82. Depends on F82. Evidence: release.yml_

</details>

### Docs, legal and community

| ID | Feature | What users get | OS | Size | Priority | Milestone |
| :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| F86 | Meet the Slint royalty-free attribution condition | Keeps the right to ship every build, including the paid one. | Both | S | P0 | v0.2.0-beta.1 |
| F85 | Third-party license notices in every artifact | Downloads comply with dependency licenses. | Both | S | P0 | v0.2.0-beta.2 |
| F88 | Community health files and repo metadata | Testers know how to report bugs and vulnerabilities. | n/a | S | P1 | v0.2.0-beta.2 |
| F90 | Distribution, signing and Store decisions in ADR 0008 | Packagers and users know which channels exist and why. | n/a | S | P1 | v0.2.0-beta.2 |
| F91 | Fix the Sponsors link and support wording | Supporters reach a working page. | n/a | S | P2 | v0.2.0-beta.2 |
| F110 | Trademark and affiliation review | No takedown risk for the name or icon. | Both | S | P2 | v0.2.0-beta.3 |
| F87 | Privacy statement and product charter | Reviewers see exactly what the launcher does and never does. | Both | S | P1 | v1.0.0-rc.1 |
| F89 | User troubleshooting guide | Users fix the common failures themselves. | Both | M | P1 | v1.0.0-rc.1 |
| F92 | Support policy | Users know before installing whether they are supported. | Both | S | P1 | v1.0.0-rc.1 |
| F96 | Audit README, GLOSSARY and ADRs against the RC build | Docs describe what ships. | Both | S | P1 | v1.0.0-rc.1 |
| F109 | Security and privacy review of IPC and logs | The launcher's log never reads like a keylogger. | Both | S | P1 | v1.0.0-rc.1 |
| F112 | Recruit RC testers and open a feedback channel | RCs are tested beyond one machine. | Both | S | P1 | v1.0.0-rc.1 |
| F113 | Configuration, CLI and known-limitations reference | A complete reference for the frozen contract. | Both | S | P1 | v1.0.0-rc.1 |

<details>
<summary>Scope and evidence</summary>

- **F86 Meet the Slint royalty-free attribution condition.** Put the official Made with Slint badge on the README and the release-notes template (section 2(b)). Fix the license name at README:234. AboutSlint in the ABOUT row is optional. Note the GPLv3 option for packagers. _Builds on [PR #24](https://github.com/jav76/rusty-start/pull/24). Evidence: Slint LicenseRef-Slint-Royalty-free-2.0 section 2; README.md:11, 234_
- **F85 Third-party license notices in every artifact.** cargo-about per target. Ship notices in the zip and tar.gz and under /usr/share/doc/rusty-start, link them from the ABOUT row, and fail CI when stale. Record the license basis for the MSVC CRT. _Evidence: release.yml:66, 122_
- **F88 Community health files and repo metadata.** SECURITY.md and private vulnerability reporting; CONTRIBUTING; Code of Conduct; issue forms (the bug form asks for version and log until F53 lands); a PR template; Discussions or a pinned feedback issue; description, topics and homepage. _Evidence: .github has only FUNDING.yml and workflows_
- **F90 Distribution, signing and Store decisions in ADR 0008.** Channels by gate and the never list (Flathub, Snap, AppImage, MSI) with reasons; the signing identity; the update policy; Slint licensing; the Store packaged-mode decision; uiAccess versus scheduled task (F26); the fail-open principle (F99). _Builds on [#6](https://github.com/jav76/rusty-start/issues/6). Evidence: docs/adr has 0001-0007; no distribution ADR_
- **F91 Fix the Sponsors link and support wording.** Finish Sponsors setup or remove the link and FUNDING.yml; change 'offered' to 'planned' for paid builds. _Evidence: hasSponsorsListing false; README.md:225-226_
- **F110 Trademark and affiliation review.** No Windows, KDE or Rust logos or lookalikes; a not-affiliated line; check the name against the Rust trademark policy. _Evidence: release-completeness missing item_
- **F87 Privacy statement and product charter.** PRIVACY.md and a README section. No network or telemetry. The hook sees every key event and ignores all but the Windows key (and raw input if F20 adopts it). Snapshots are never written to disk. List what is stored and how to clear it; the Web Fallback opens the browser. The charter promises only what 1.0 does. Link both from ABOUT. _Builds on [#6](https://github.com/jav76/rusty-start/issues/6), [#13](https://github.com/jav76/rusty-start/issues/13). Evidence: README.md has no privacy section; trigger/win32.rs:368-427_
- **F89 User troubleshooting guide.** Covers Plain Glass causes, Setup Audit states, SmartScreen and Smart App Control, antivirus, a dead Win or Meta key, RDP and VMs, logs and RUSTY_START_LOG, the taskbar Start button FAQ, pinning rusty-start.exe or its shortcut to the taskbar, and a Plasma panel launcher (from F29). _Builds on [PR #26](https://github.com/jav76/rusty-start/pull/26), [#37](https://github.com/jav76/rusty-start/issues/37). Evidence: README.md:81-82; docs/ holds adr and media only_
- **F92 Support policy.** SUPPORT.md. Tier 1: Windows 11 x64, Plasma 6 Wayland on Debian 13 and Kubuntu 26.04. Tier 2: Windows on ARM, Linux arm64, other glibc 2.36+ distros. Unsupported: Windows 10, X11 (F58), GNOME. Fixes land on the latest release only; also the MSRV policy and what paid builds add. _Depends on F58. Evidence: README.md:61; release.yml:29-30_
- **F96 Audit README, GLOSSARY and ADRs against the RC build.** A docs-drift pass. Includes the known wrong claims (license name, auto-updating builds, Sponsors) and the wrong ADR 0005 line citations. _Evidence: ADR 0005 has 62 lines; README.md:225-234_
- **F109 Security and privacy review of IPC and logs.** A threat model in SECURITY.md for D-Bus and window-message callers; length and encoding checks for F62 payloads; a test that logs never contain typed queries or key codes. _Evidence: protocol/src/lib.rs_
- **F112 Recruit RC testers and open a feedback channel.** Recruit 10-20 testers across Tier 1 configs from KDE Discuss and Reddit, give them a test script from the matrix, and pin an RC discussion. _Depends on F78. Evidence: repo has 1 star_
- **F113 Configuration, CLI and known-limitations reference.** Every config key with default and range, --help with exit codes, and Known limitations. A unit test fails when Config gains an undocumented key. _Depends on F55. Evidence: config.rs:187-194_

</details>

## Dropped or merged

| ID | Idea | Why |
| :--- | :--- | :--- |
| F17 | Honor Windows power policies | Merged into [#34](https://github.com/jav76/rusty-start/issues/34) (two critics): one pure decision function for policies, capabilities and refusals. |
| F29 | Pinnable Rusty Start shortcut | Merged: F67's installer creates the Start Menu shortcut and the index excludes it; F89 documents pinning the exe and a Plasma panel launcher. |
| F21 (part) | Fix the README Meta claim | [PR #27](https://github.com/jav76/rusty-start/pull/27) already rewrites README:92-93. |
| F49 (part) | Enable resvg raster-images | Refuted: raster-images and svgz are already enabled through Slint's feature unification (cargo tree -i resvg). |
| F61 (part) | Offer to update a mismatched Run value | Refuted: run_key.rs:20-25 already reports Conflict, and Install updates it. |
| F03 (part) | 'code' finds VS Code example | Refuted: it already matches by WordPrefix (search.rs:60). |
| F15 (part) | Fix the dead Linux Uninstall entry | [PR #21](https://github.com/jav76/rusty-start/pull/21) never offers Uninstall on Linux; F15 is restated as new parity. |
| F01 (part) | CompareStringEx and Linux collator | Platform code in search.rs; replaced by sorting on the folded key. |
| F14 (part) | IContextMenu runas probe | COM complexity for little gain; PKEYs are enough. |
| F43 (part) | Reset settings button | Deleting config.toml does the same; documented instead. |
| F41 (part) | Rotating footer hints | Needs persisted session counting for little value. |
| F54 (part) | Free snapshot textures after close | Allocates on the open path against AGENTS.md unless measurements justify it. |
| missing: severity definitions | Severity, triage labels, known-issues page | Merged into F84. |
| missing: log location | Move log and state files before the freeze | Merged into F52. |
