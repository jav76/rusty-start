# Rusty Start

A performance-focused replacement for the Windows 11 Start menu, written in Rust. It also runs as an
app launcher on Linux, with KDE Plasma 6 on Wayland as the first supported desktop.

## Status

The host runs resident with its menu hidden and opens it when you tap the Windows (Meta) key. It
lists installed apps with their icons, keeps pinned and recent apps at the top, searches as you
type, launches apps, and offers lock, sign out, sleep, restart and shut down.

Known limitation: clicking the taskbar Start button still opens the native Windows 11 Start menu. The
XAML taskbar receives clicks outside the message queue, so the explorer hook (`rusty_start_hook.dll`)
never sees them (see `docs/adr/0002-explorer-taskbar-hook-dll.md`). Supporting the click is deferred;
the Windows key is the supported trigger.

## Requirements

- Windows 11, or Linux with KDE Plasma 6 on Wayland (tested on Debian 13). Other Linux desktops run
  the menu without the Meta key binding or desktop snapshots.
- Rust 1.92 or later from [rustup](https://rustup.rs). Debian's packaged Rust is too old.
- On Linux: `pkg-config` and `libfontconfig-dev` to build.

## Building and running

```
cargo build --workspace
cargo run -p rusty-start-host
```

Debug builds show a console window for diagnostic output on Windows; release builds do not. Only one
host runs per session: launching `rusty-start` again toggles the running host's menu and exits.

## Installing on Linux

Releases ship a `.deb` and a `.tar.gz` for amd64 and arm64. The package installs `/usr/bin/rusty-start`,
its desktop entry and a D-Bus service file, so a Meta tap starts the host when it is not running.

```
sudo apt install ./rusty-start_*.deb
rusty-start setup
rusty-start
```

`rusty-start setup` is run once per user and prints every change it makes. On KDE Plasma it:

- moves the Meta key from the Application Launcher to Rusty Start's KDE global shortcut ("Toggle
  the menu" under System Settings > Shortcuts), keeping the launcher's other shortcuts such as
  Alt+F1,
- adds a KWin window rule that keeps the menu out of the taskbar and task switcher,
- adds an autostart entry, and reloads KWin.

`rusty-start setup --undo` reverses all of it and gives the Meta key back to the Application
Launcher. On other desktops, bind a keyboard shortcut to `rusty-start` instead: a second launch
toggles the menu.

Running from a build directory (`cargo run`), add `--dev`: `rusty-start setup --dev` also installs a
hidden desktop entry for the current executable. KWin only shares desktop snapshots with executables
listed in an installed desktop entry; without one the menu opens as Plain Glass, a dimmed veil with
an unrefracted card.

## Usage

| Action                            | Keys or mouse                                                   |
| --------------------------------- | --------------------------------------------------------------- |
| Open or close the menu            | Tap Win (Meta), or Ctrl+Alt+Space on Windows                    |
| Search                            | Type while the menu is open                                     |
| Move the selection                | Up and Down                                                     |
| Launch                            | Enter, or click a row                                           |
| Pin or unpin the selected app     | Ctrl+P, or the pin button on a row                              |
| Power options                     | Power button in the bottom-right corner, or search for them     |
| Change the card position          | Anchor pill in the top-right corner                             |
| Close                             | Escape, a click outside the card, or switching to another app   |

- Windows shortcuts such as Win+R, Win+E and Win+Shift+S work as usual, with the menu open or closed.
  Right-clicking the Start button (the Win+X menu) is unaffected. On Plasma, Meta shortcuts keep
  working because KWin only calls Rusty Start for a Meta tap on its own.
- With an empty search the list shows **Pinned** apps, up to five **Recent** apps, then **All apps**.
  Search results rank apps you launch often and recently above others that match equally well.
- Power actions found by search (for example `lock` or `shutdown`) are listed below the app results.
  Sign out, Restart and Shut down run only when selected a second time.
- The app list refreshes in the background when apps are installed or removed, and the new list
  appears the next time the menu opens. On Linux, apps come from `.desktop` files in
  `~/.local/share/applications` and the system and Flatpak application folders, and launched apps
  run in their own systemd scope, so they keep running if Rusty Start stops. Icons load after the list, so rows show a placeholder tile
  for a second or two after startup.

## Configuration

Settings live in `%APPDATA%\RustyStart\config.toml` (`$XDG_CONFIG_HOME/rusty-start/config.toml` on
Linux):

| Key                | Values                                                              | Default         |
| ------------------ | ------------------------------------------------------------------- | --------------- |
| `card_anchor`      | `middle-left`, `center`, `bottom-left`, `bottom-center`             | `middle-left`   |
| `edge_margin`      | Inset from the screen edge in logical pixels, `0` to `512`          | `48`            |
| `use_win_key`      | `true` or `false`: open the menu with the Windows key (Windows only) | `true`          |
| `use_taskbar_hook` | `true` or `false`: install the explorer taskbar hook (Windows only) | `true`          |

On Linux the Meta key binding belongs to the desktop, so `use_win_key` has no effect there; use
`rusty-start setup` or `rusty-start setup --undo` instead.

Pins and launch history are saved to `usage.tsv` in the same folder (format in
`docs/adr/0003-usage-file-format.md`).

If another application already owns Ctrl+Alt+Space, the host logs a warning and keeps running. Other
programs can toggle the menu by posting the `RustyStart.ToggleMenu` message to the
`RustyStart.TriggerWindow` window, after calling `AllowSetForegroundWindow` with the host's process ID
so the menu can take keyboard focus (see `crates/protocol`). On Linux, call `Toggle()` on the
session bus: `busctl --user call io.github.jav76.RustyStart /io/github/jav76/RustyStart
io.github.jav76.RustyStart Toggle`.

## Checks

CI runs in a `debian:bookworm` container on a self-hosted Linux runner for every push to `main` and
every pull request, and lints the Windows backend with cross-target clippy. Run the same checks
locally before opening a PR:

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
iterated on without desktop integration. Windows integration can only be tested on Windows, and
Linux desktop integration on a Linux desktop session.

## Project layout

| Path              | Purpose                                                                                      |
| ----------------- | -------------------------------------------------------------------------------------------- |
| `crates/host`     | Resident process: Liquid-Glass GPU shaders, desktop capture, Slint UI, triggers, app search, pins and power actions |
| `crates/hook`     | In-process Explorer hook DLL: Start button click suppression and IPC trigger dispatch        |
| `crates/protocol` | Window class, message names, mutex name, and shared memory geometry shared by host and hook |
| `GLOSSARY.md`     | Domain terminology such as the Fog Veil, Liquid-Glass Card, Card Anchor and Frecency          |
| `docs/adr`        | Architecture Decision Records                                                                |
| `packaging/linux` | Desktop entry and D-Bus service file installed by the `.deb`                                 |
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
