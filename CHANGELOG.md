# Changelog

All notable changes to Rusty Start are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/). A release tag `vX.Y.Z` must equal the version in
`Cargo.toml` and have a section below; the release workflow refuses the tag otherwise and uses the
section as the release notes.

## [Unreleased]

## [0.2.0-beta.1] - 2026-11-03

### Added

- The App Action Flyout: right-click, the Menu key or Shift+F10 on an app row offers Pin or Unpin,
  Open file location, Run as administrator and Uninstall on Windows, and the desktop entry's own
  actions on Linux (#21).
- `rusty-start --version`, `--help`, `toggle` and `quit`, plus the version and a Quit button on the
  Settings Page. Other programs can stop the host with the `RustyStart.QuitHost` message or the
  D-Bus `Quit()` method (#24).
- A log file, `rusty-start.log` in the configuration folder, rotated at 1 MiB, that also records
  panics; `RUSTY_START_LOG` sets the level. Windows release builds show a message box for a panic
  and for `rusty-start setup` output when there is no console (#22).
- Setup Audit rows on Plasma for whether the effective desktop entry authorizes this build for
  desktop snapshots, and for what the latest snapshot attempt came to (#26).
- Keyword matches (desktop entry keywords, program file names) rank in their own tier below name
  matches (#20).
- A per-keystroke latency budget test in CI: rebuilding the rows for 1,000 apps must take under
  1 ms (#18).

### Changed

- `config.toml` and `usage.tsv` that cannot be read are moved to `<name>.broken-<timestamp>` instead
  of being overwritten; saving `config.toml` keeps comments and unknown keys; unknown keys and
  invalid values are logged by name; pending saves are written before sign out, restart and shut
  down (#25).
- `rusty-start setup` without `--dev` removes a development desktop entry that points at another
  build, which masked the packaged entry and left the menu as Plain Glass (#26).
- Releases with a pre-release suffix are marked pre-release, every release carries `SHA256SUMS`,
  and the release notes come from this file.

### Fixed

- A solo Windows-key tap after Win+L, a UAC prompt or Ctrl+Alt+Del no longer goes dead: lost key
  releases are detected and the modifier state is re-read from the keyboard (#23).

### Removed

- The explorer taskbar hook DLL, which never saw Start clicks on current Windows 11 builds (#19).
  Taskbar Start clicks stay unsupported; the Windows key is the trigger.

## [0.1.1-alpha.1] - 2026-10-23

### Added

- The Settings Page: gear button and Ctrl+, open card position, Veil Mode, blur strength, the Meta
  key switch and the Setup Audit with Install and Undo (#5).

## [0.1.0-alpha.1] - 2026-10-21

### Added

- Windows 11: a resident host with a GPU-rendered Liquid-Glass menu over a Fog Veil, opened by a
  solo Windows-key tap, with search as you type, pinned and recent apps, app icons and power
  actions.
- Linux with KDE Plasma 6 on Wayland: a D-Bus host with a KWin overlay and desktop snapshots,
  apps from desktop entries launched into their own systemd scopes, power actions through the
  session manager and logind, `rusty-start setup` for the Meta key, KWin rule and autostart, and a
  `.deb` package (#1).
- Release builds for Linux amd64 and arm64 and Windows x64 and arm64 from the self-hosted runner (#4).
