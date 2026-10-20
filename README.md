# Rusty Start

A performance-focused replacement for the Windows 11 Start menu, written in Rust.

## Status

The host process runs resident with its menu window hidden and toggles it when you tap the Windows
key or press the Ctrl+Alt+Space fallback hotkey. It lists installed apps with their icons from the
shell's Applications folder, keeps pinned and recent apps at the top, searches as you type, launches
the selected app, and offers lock, sign out, sleep, restart and shut down.

Known limitation: clicking the taskbar Start button still opens the native Windows 11 Start menu. The
explorer hook (`rusty_start_hook.dll`) never sees the click on current Windows 11 builds, because the
XAML taskbar receives clicks outside the message queue (see `docs/adr/0002-explorer-taskbar-hook-dll.md`).

## Requirements

- Windows 11
- Rust 1.92 or later

## Building and running

```
cargo build --workspace
cargo run -p rusty-start-host
```

Tap the Windows key or press Ctrl+Alt+Space to open or close the menu. System shortcuts such as `Win+R`, `Win+E`, and `Win+Shift+S` pass through normally,
and right-clicking the Start button (Win+X menu) is unaffected. When opened, the active monitor
smoothly fogs over while the `640x720` Liquid-Glass menu card condenses into place (anchored to the
middle-left of the screen by default). Type to search, use Up and Down to move the selection, and
press Enter or click an app to launch it. The menu takes keyboard focus as soon as it opens. Press
Escape, click the fogged backdrop outside the card, or switch to another window (Alt+Tab, a click on
another app) to dismiss the menu.

With an empty search the list shows **Pinned** apps, up to five **Recent** apps and then **All
apps**. Press Ctrl+P or click the pin button on a row to pin or unpin the selected app. Search
results rank apps you launch often and recently above others that match equally well.

The power button in the bottom-right corner opens Lock, Sign out, Sleep, Restart and Shut down.
Typing their names (for example `lock` or `shutdown`) lists them below the app results. From search,
Sign out, Restart and Shut down ask you to select them a second time before they run.

Pins and launch history are saved to `usage.tsv` next to `config.toml` (format in
`docs/adr/0003-usage-file-format.md`). Click the anchor pill in the top-right corner of the card (or edit
`%APPDATA%\RustyStart\config.toml` on Windows / `$XDG_CONFIG_HOME/rusty-start/config.toml` on Linux)
to switch between `middle-left`, `center`, `bottom-left` and `bottom-center` positioning, or set
`use_win_key = false` or `use_taskbar_hook = false` to disable specific triggers. Debug builds show
a console window for diagnostic output; release builds do not.

Only one host runs per session. Launching `rusty-start` again toggles the running host's menu and
exits. The app list refreshes in the background when apps are installed or removed (changes in the
Start Menu Programs folders or the shell's Applications folder), and the new list appears the next
time the menu opens. Icons load in the background after the list, so rows show a placeholder tile
for the first second or two after startup.

If another application already owns Ctrl+Alt+Space, the host logs a warning and keeps running. The
menu can still be toggled by posting the `RustyStart.ToggleMenu` message to the
`RustyStart.TriggerWindow` window. Senders should call `AllowSetForegroundWindow` with the host's
process ID first so the menu can take keyboard focus (see `crates/protocol`).

## Checks

CI runs on `windows-latest` for every push to `main` and every pull request. Run the same checks
locally before opening a PR:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

On Linux the host builds in preview mode: the menu opens once at startup over a synthetic desktop
backdrop with a fixed sample app list, so the Liquid-Glass shaders and UI can be iterated on without
a Windows machine. Windows integration can only be tested on Windows.

## Project layout

| Path              | Purpose                                                                                      |
| ----------------- | -------------------------------------------------------------------------------------------- |
| `crates/host`     | Resident process: Liquid-Glass GPU shaders, desktop capture, Slint UI, triggers, app search, pins and power actions |
| `crates/hook`     | In-process Explorer hook DLL: Start button click suppression and IPC trigger dispatch        |
| `crates/protocol` | Window class, message names, mutex name, and shared memory geometry shared by host and hook |
| `GLOSSARY.md`     | Domain terminology such as the Fog Veil, Liquid-Glass Card, Card Anchor and Frecency          |
| `docs/adr`        | Architecture Decision Records                                                                |
| `AGENTS.md`       | Coding standards and workflow rules for contributors and coding agents                       |

## Support & Distribution Model

Rusty Start is developed as **convenience-ware / donationware**:

- **100% Free and Open Source**: The entire codebase is freely licensed under MIT. You can build from source, inspect the code, or download standalone releases for free.
- **Convenience Builds**: Packaged, auto-updating builds (e.g., Microsoft Store releases) are offered as convenience purchases to support active maintenance and feature development.
- **Donations & Sponsorship**: If Rusty Start improves your daily workflow, consider sponsoring development via [GitHub Sponsors](https://github.com/sponsors/jav76).

## License

Rusty Start is licensed under the [MIT License](LICENSE).

### Third-Party Licenses

- **Slint**: Rusty Start's UI is built with the Slint GUI toolkit. Binaries distributed for desktop use rely on Slint's Royalty-Free Desktop and Small Business License (or GPLv3). See the [Slint project](https://slint.dev) for licensing details and options.
