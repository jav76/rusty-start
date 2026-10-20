# 0005: One Linux Binary, a Session-Bus Host and a Fullscreen winit Overlay

## Status

Accepted (2026-10-20). KDE Plasma 6 on Wayland is the first Linux target.

## Context

Linux desktops differ in what they let a launcher do. Wayland clients cannot position windows,
read the global cursor position, grab keys or capture the screen without compositor support. KWin
binds Meta alone only through `kwinrc [ModifierOnlyShortcuts]`, which can call a D-Bus method
without arguments. Slint 1.18 destroys its window on every hide under Wayland and rebuilds the winit
attributes without the backend's attributes hook.

## Decision

- **One binary per CPU architecture.** Each Linux backend probes its capability at runtime (a bus
  name, a protocol, an authorization) and falls back when it is missing. There are no per-desktop
  builds; `XDG_CURRENT_DESKTOP` only chooses setup instructions and log messages.
- **The host owns `io.github.jav76.RustyStart` on the session bus.** Owning the name is the
  single-instance lock, like the named mutex on Windows. The host serves `Toggle()` for compositor
  bindings and `ToggleWithToken(s)` for second launches, which forward their
  `XDG_ACTIVATION_TOKEN` and exit. Names live in `crates/protocol`.
- **The overlay is a fullscreen winit window**, not a layer-shell surface. Fullscreen is requested
  through Slint's window properties before every show, which survive Slint's re-creation. The
  in-window Meta tap handler is off, because the compositor already delivers the toggle.

## Consequences

- Slint's accessibility, IME and input handling keep working on Linux.
- Every open re-creates the window and the GL resources. If the first visible frame regularly takes
  more than 50 ms after the toggle on KWin, a layer-shell surface on a custom Slint platform
  replaces this window model; all of it lives in `backdrop/linux.rs`.
- Windows after the first cannot receive an activation token, so focus on later opens follows the
  compositor's focus-stealing policy (a KWin window rule installed by `rusty-start setup`).
- Slint creates the session's first window at startup, before the first open. Toggling Slint's
  fullscreen property on that live window reaches the compositor as `unset_fullscreen`, so the
  backend asks winit directly when a window exists and uses the property only after a hide.
- The fullscreen window can appear in the taskbar and Alt+Tab while open unless the same KWin rule
  hides it.

## Measurements (2026-10-20)

Release build on Debian 13, Plasma/KWin 6.3.6, Wayland, toggled with `busctl --user call ... Toggle`:

- First visible frame after show: 25-34 ms warm, 61-96 ms on the first open of a session (EGL
  context creation). Within the 50 ms gate for warm opens, so the layer-shell spike is not needed.
- Debug builds take about 180 ms, all of it in Slint's first unoptimized frame on the new context.
  Judge latency on release builds only.
- KWin gave keyboard focus to every window opened by a token-less `Toggle()`, without a window rule.
- The overlay followed the active output on a 2560x1440 and a 3440x1440 monitor.
