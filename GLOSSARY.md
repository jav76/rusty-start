# Glossary

- **Fog Veil**: The full-monitor (`rcMonitor`, including the taskbar) progressive backdrop blur and atmospheric dim layer surrounding the floating Start menu card while the menu is open.
  - _Avoid_: modal overlay, dimmer, wallpaper blur.
- **Liquid-Glass Card**: The `640x720` floating Start menu surface rendered via an SDF rounded-rectangle fragment shader with curved-bevel lens refraction, subtle chromatic dispersion, deep interior frost, and fresnel specular highlights.
  - _Avoid_: acrylic box, modal dialog, popup panel.
- **Optical Condense**: The `180ms` open transition that pairs subtle card scale (`0.92 -> 1.0`) and vertical lift (`16px -> 0px`) with a transient lens-refraction pulse and a diagonal specular rim sweep while keyboard focus is active immediately at `t = 0ms`.
  - _Avoid_: fade-in, zoom animation.
- **Card Anchor**: The user-configurable alignment preset (`middle-left` by default, `center`, `bottom-left`, `bottom-center`) that determines where the `640x720` Liquid-Glass Card sits within the active monitor's Fog Veil.
  - _Avoid_: window dock, screen gravity.

