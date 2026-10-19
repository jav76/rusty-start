# Rusty Start

A performance-focused replacement for the Windows 11 Start menu, written in Rust.

## Status

Milestone 1 of 3. The host process runs resident with its menu window hidden and toggles it with a
development hotkey. It lists installed apps from the shell's Applications folder, searches them as you
type and launches the selected one. The low-level keyboard hook and the explorer hook are not yet
implemented.

## Requirements

- Windows 11
- Rust 1.92 or later

## Building and running

```
cargo run -p rusty-start-host
```

Press Ctrl+Alt+Space to open or close the menu. Type to search, use Up and Down to move the
selection, and press Enter or click an app to launch it. Escape closes the menu. Debug builds show a
console window for diagnostic output; release builds do not.

If another application already owns Ctrl+Alt+Space, the host logs a warning and keeps running. The
menu can still be toggled by posting the `RustyStart.ToggleMenu` message to the
`RustyStart.TriggerWindow` window.

On Linux the host builds in preview mode: the menu opens once at startup with a fixed sample app list,
so the UI can be iterated on without a Windows machine. Windows integration can only be tested on Windows.

## Project layout

| Path              | Purpose                                                              |
| ----------------- | -------------------------------------------------------------------- |
| `crates/host`     | Resident process: Slint UI, trigger handling, app index and search   |
| `crates/protocol` | Window class and message names shared with the future explorer hook  |

## License

MIT. Slint is used under one of its own licenses; see the Slint project for the terms that apply
to your distribution.
