# 0004: KWin ScreenShot2 Snapshot with a Plain Glass Fallback on Linux

## Status

Accepted (2026-10-20). Supplements ADR 0001 for the Linux backend.

## Context

The Fog Veil and Liquid-Glass Card refract a snapshot of the desktop. Wayland clients cannot read
the screen, so the GDI capture of ADR 0001 has no Linux counterpart. The options on KWin were
`org.kde.KWin.ScreenShot2` (raw pixels over a pipe, only for apps whose installed `.desktop` file
lists the interface in `X-KDE-DBUS-Restricted-Interfaces`), KWin's live blur protocol behind a
transparent window (no refraction or dispersion, and a blur that cannot ramp from zero, the reason
ADR 0001 rejected DWM Acrylic), the xdg-desktop-portal screenshot (writes a PNG file and may
prompt), or the wallpaper alone.

## Decision

- Capture the active screen with `CaptureActiveScreen` (`native-resolution`, no cursor) on the
  `rusty-start-capture` thread, started when the toggle arrives, while the window is created and
  shown fully transparent with keyboard focus. The same shader runs as on Windows.
- The Optical Condense starts when the snapshot lands, or after 100 ms as **Plain Glass**: a dimmed
  veil and a frosted, unrefracted card over the live desktop, drawn by a branch of the same shader.
- When KWin is missing or refuses the executable, the host logs why once and opens as Plain Glass
  for the rest of the session without waiting.

## Consequences

- Linux looks like Windows on KWin, but only for installed builds or dev builds with a desktop
  entry (`rusty-start setup --dev`). Other compositors get Plain Glass until they gain a capture
  path.
- Measured on a 3440x1440 monitor (Plasma 6.3.6, release build): capture 18-29 ms (about 10 ms in
  KWin, 6-10 ms reading the pipe, 4-6 ms converting to bottom-up RGBA), first visible frame
  43-66 ms after show, against 22-27 ms as Plain Glass. Moving the conversion to the GPU (texture
  swizzle and a flipped sample) would save about 5 ms.
