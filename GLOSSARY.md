# Glossary

- **Fog Veil**: The full-monitor (`rcMonitor`, including the taskbar) progressive backdrop blur and atmospheric dim layer surrounding the floating Start menu card while the menu is open.
  - _Avoid_: modal overlay, dimmer, wallpaper blur.
- **Liquid-Glass Card**: The `640x720` floating Start menu surface rendered via an SDF rounded-rectangle fragment shader with curved-bevel lens refraction, subtle chromatic dispersion, deep interior frost, and fresnel specular highlights.
  - _Avoid_: acrylic box, modal dialog, popup panel.
- **Optical Condense**: The `180ms` open transition that pairs subtle card scale (`0.92 -> 1.0`) and vertical lift (`16px -> 0px`) with a transient lens-refraction pulse and a diagonal specular rim sweep while keyboard focus is active immediately at `t = 0ms`.
  - _Avoid_: fade-in, zoom animation.
- **Pinned Apps**: Apps the user pinned (Ctrl+P or the row's pin button), listed first in pin order whenever the search is empty.
  - _Avoid_: favorites, bookmarks.
- **Recent Apps**: Up to five most recently launched apps that are not pinned, listed between the Pinned Apps and All apps.
  - _Avoid_: history, MRU list.
- **Frecency**: Launch count weighted by how recently the app was last launched. Breaks ties between search results of the same match quality.
  - _Avoid_: popularity, usage score.
- **Power Flyout**: The panel above the card's power button listing Lock, Sign out, Sleep, Restart and Shut down.
  - _Avoid_: shutdown menu, power menu.
- **Card Anchor**: The user-configurable alignment preset (`middle-left` by default, `center`, `bottom-left`, `bottom-center`) that determines where the `640x720` Liquid-Glass Card sits within the active monitor's Fog Veil.
  - _Avoid_: window dock, screen gravity.

