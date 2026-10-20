# Rusty Start

A performance-focused replacement for the Windows 11 Start menu, written in Rust.

## Status

Milestone 3 of 3 complete. The host process runs resident with its menu window hidden and toggles it
when you click the Windows 11 Taskbar Start button (intercepted in-process via `rusty_start_hook.dll`),
tap the Windows key, or press the Ctrl+Alt+Space fallback hotkey. It lists installed apps from the
shell's Applications folder, searches them as you type and launches the selected one.

## Requirements

- Windows 11
- Rust 1.92 or later

## Building and running

```
cargo build --workspace
cargo run -p rusty-start-host
```

Click the Windows 11 Taskbar Start button, tap the Windows key, or press Ctrl+Alt+Space to open or
close the menu. System shortcuts such as `Win+R`, `Win+E`, and `Win+Shift+S` pass through normally,
and right-clicking the Start button (Win+X menu) remains fully functional. When opened, the active monitor
smoothly fogs over while the `640x720` Liquid-Glass menu card condenses into place (anchored to the
middle-left of the screen by default). Type to search, use Up and Down to move the selection, and
press Enter or click an app to launch it. Press Escape or click the fogged backdrop outside the card
to dismiss the menu. Click the anchor pill in the top-right corner of the card (or edit
`%APPDATA%\RustyStart\config.toml` on Windows / `$XDG_CONFIG_HOME/rusty-start/config.toml` on Linux)
to switch between `middle-left`, `center`, `bottom-left` and `bottom-center` positioning, or set
`use_win_key = false` or `use_taskbar_hook = false` to disable specific triggers. Debug builds show
a console window for diagnostic output; release builds do not.

If another application already owns Ctrl+Alt+Space, the host logs a warning and keeps running. The
menu can still be toggled by posting the `RustyStart.ToggleMenu` message to the
`RustyStart.TriggerWindow` window.

On Linux the host builds in preview mode: the menu opens once at startup over a synthetic desktop
backdrop with a fixed sample app list, so the Liquid-Glass shaders and UI can be iterated on without
a Windows machine. Windows integration can only be tested on Windows.

## Project layout

| Path              | Purpose                                                                                      |
| ----------------- | -------------------------------------------------------------------------------------------- |
| `crates/host`     | Resident process: Liquid-Glass GPU shaders, desktop capture, Slint UI, triggers, app search  |
| `crates/hook`     | In-process Explorer hook DLL: Start button click suppression and IPC trigger dispatch        |
| `crates/protocol` | Window class, message names, and shared memory geometry shared between host and hook        |
| `GLOSSARY.md`     | Domain terminology for the Fog Veil, Liquid-Glass Card, Optical Condense and Card Anchor     |
| `docs/adr`        | Architecture Decision Records                                                                |

## Support & Distribution Model

Rusty Start is developed as **convenience-ware / donationware**:

- **100% Free and Open Source**: The entire codebase is freely licensed under MIT. You can build from source, inspect the code, or download standalone releases for free.
- **Convenience Builds**: Packaged, auto-updating builds (e.g., Microsoft Store releases) are offered as convenience purchases to support active maintenance and feature development.
- **Donations & Sponsorship**: If Rusty Start improves your daily workflow, consider sponsoring development via [GitHub Sponsors](https://github.com/sponsors/jav76).

## License

Rusty Start is licensed under the [MIT License](LICENSE).

### Third-Party Licenses

- **Slint**: Rusty Start's UI is built with the Slint GUI toolkit. Binaries distributed for desktop use rely on Slint's Royalty-Free Desktop and Small Business License (or GPLv3). See the [Slint project](https://slint.dev) for licensing details and options.
