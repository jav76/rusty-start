# Rusty Start

A performance-focused replacement for the Windows 11 Start menu, written in Rust.

## Status

The host runs resident with its menu hidden and opens it when you tap the Windows key. It lists
installed apps with their icons, keeps pinned and recent apps at the top, searches as you type,
launches apps, and offers lock, sign out, sleep, restart and shut down.

Known limitation: clicking the taskbar Start button still opens the native Windows 11 Start menu. The
XAML taskbar receives clicks outside the message queue, so the explorer hook (`rusty_start_hook.dll`)
never sees them (see `docs/adr/0002-explorer-taskbar-hook-dll.md`). Supporting the click is deferred;
the Windows key is the supported trigger.

## Requirements

- Windows 11
- Rust 1.92 or later

## Building and running

```
cargo build --workspace
cargo run -p rusty-start-host
```

Debug builds show a console window for diagnostic output; release builds do not. Only one host runs
per session: launching `rusty-start` again toggles the running host's menu and exits.

## Usage

| Action                            | Keys or mouse                                                   |
| --------------------------------- | --------------------------------------------------------------- |
| Open or close the menu            | Tap Win, or Ctrl+Alt+Space                                      |
| Search                            | Type while the menu is open                                     |
| Move the selection                | Up and Down                                                     |
| Launch                            | Enter, or click a row                                           |
| Pin or unpin the selected app     | Ctrl+P, or the pin button on a row                              |
| Power options                     | Power button in the bottom-right corner, or search for them     |
| Change the card position          | Anchor pill in the top-right corner                             |
| Close                             | Escape, a click outside the card, or switching to another app   |

- Windows shortcuts such as Win+R, Win+E and Win+Shift+S work as usual, with the menu open or closed.
  Right-clicking the Start button (the Win+X menu) is unaffected.
- With an empty search the list shows **Pinned** apps, up to five **Recent** apps, then **All apps**.
  Search results rank apps you launch often and recently above others that match equally well.
- Power actions found by search (for example `lock` or `shutdown`) are listed below the app results.
  Sign out, Restart and Shut down run only when selected a second time.
- The app list refreshes in the background when apps are installed or removed, and the new list
  appears the next time the menu opens. Icons load after the list, so rows show a placeholder tile
  for a second or two after startup.

## Configuration

Settings live in `%APPDATA%\RustyStart\config.toml` (`$XDG_CONFIG_HOME/rusty-start/config.toml` on
Linux):

| Key                | Values                                                              | Default         |
| ------------------ | ------------------------------------------------------------------- | --------------- |
| `card_anchor`      | `middle-left`, `center`, `bottom-left`, `bottom-center`             | `middle-left`   |
| `edge_margin`      | Inset from the screen edge in logical pixels, `0` to `512`          | `48`            |
| `use_win_key`      | `true` or `false`: open the menu with the Windows key               | `true`          |
| `use_taskbar_hook` | `true` or `false`: install the explorer taskbar hook                | `true`          |

Pins and launch history are saved to `usage.tsv` in the same folder (format in
`docs/adr/0003-usage-file-format.md`).

If another application already owns Ctrl+Alt+Space, the host logs a warning and keeps running. Other
programs can toggle the menu by posting the `RustyStart.ToggleMenu` message to the
`RustyStart.TriggerWindow` window, after calling `AllowSetForegroundWindow` with the host's process ID
so the menu can take keyboard focus (see `crates/protocol`).

## Checks

CI runs on `windows-latest` and in a `debian:bookworm` container for every push to `main` and every
pull request. Run the same checks locally before opening a PR:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p rusty-start-host --all-targets --locked --features preview -- -D warnings
cargo test --workspace --locked
```

Linux builds need `pkg-config` and `libfontconfig-dev` (`sudo apt install pkg-config
libfontconfig-dev` on Debian).

`--features preview` builds the preview backend on any OS: the menu opens once at startup over a
synthetic desktop backdrop with a fixed sample app list, so the Liquid-Glass shaders and UI can be
iterated on without desktop integration. Linux support (KDE Plasma 6 on Wayland first) is in
progress: the host runs as a D-Bus service, lists apps from their `.desktop` files with theme icons,
launches them, captures the desktop through KWin for the Liquid-Glass shaders and performs power
actions through the session manager and logind. Windows integration can only be tested on Windows.

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
