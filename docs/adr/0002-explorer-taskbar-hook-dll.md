# 0002: In-Process Explorer Hook DLL and Shared Memory for Taskbar Start Interception

## Status

Accepted, but does not work on current Windows 11 builds (checked 2026-10-20 on build 26200). See
**Finding** below. Taskbar Start clicks are left unsupported for now.

## Context

In Milestone 3 of `rusty-start`, the menu must materialize not only on keyboard triggers (Windows key, `Ctrl+Alt+Space`), but also when the user clicks the physical Windows 11 Taskbar Start button.

In Windows 11, the taskbar (`Shell_TrayWnd`) and secondary taskbars (`Shell_SecondaryTrayWnd`) host modern XAML Islands (`DesktopWindowXamlSource` / `Windows.UI.Composition.DesktopWindowContentBridge`). The legacy `Start` child window is hidden, and mouse clicks are dispatched to the XAML composition bridge.

## Decision

1. **In-Process `WH_GETMESSAGE` Hook (`rusty_start_hook.dll`)**:
   We install a thread-targeted `WH_GETMESSAGE` hook onto Explorer's primary taskbar thread (`GetWindowThreadProcessId(primary_hwnd)`). Windows injects `rusty_start_hook.dll` into `explorer.exe` when messages are retrieved.

2. **Suppression via `WM_NULL`**:
   When the hook detects `WM_LBUTTONDOWN` or `WM_LBUTTONUP` whose cursor point (`msg.pt`) falls within the Start button bounding box, it replaces `msg.message = WM_NULL`. Explorer's window procedure never receives the click event, completely suppressing the native Windows 11 Start menu without visual flickering. On `WM_LBUTTONUP`, the hook posts `RustyStart.ToggleMenu` to `RustyStart.TriggerWindow`.

3. **Preserving Secondary Actions**:
   Right-clicks (`WM_RBUTTONDOWN` / `WM_RBUTTONUP`) and middle-clicks are left intact, preserving the native Win+X "Quick Link" power user menu.

4. **Named Shared Memory (`Local\RustyStartHookShared`)**:
   Rather than performing heavy UI Automation inside Explorer's UI thread (which risks re-entrancy deadlocks), the host process queries Start button geometry from its own background thread and publishes the bounding rectangles, trigger HWND, and registered message ID to a named file mapping. The hook performs O(1) in-memory rectangle checks.

5. **Self-Healing on Explorer Restarts**:
   The host's `TriggerWindow` is a top-level window registered for `TaskbarCreated`. If `explorer.exe` restarts or crashes, `TriggerWindow` receives the broadcast, re-queries the new taskbar thread, and re-installs the hook.

6. **Unwind Safety**:
   Every entry point in `rusty_start_hook.dll` wraps execution in `std::panic::catch_unwind` to prevent Rust panics from unwinding across FFI boundaries into Explorer.

## Finding (2026-10-20)

On Windows 11 build 26200 the hook never sees a Start button click, so the native Start menu opens:

- All taskbar windows, including the XAML `Windows.UI.Input.InputSite.WindowClass`, belong to the
  `Shell_TrayWnd` thread the hook is installed on, and the shared memory held the correct Start
  rectangle.
- A diagnostic build of the hook reported every message ID it saw. Mouse moves arrived as
  `WM_MOUSEMOVE` for the `InputSite` window, but a real left click produced no `WM_LBUTTONDOWN`,
  `WM_LBUTTONUP` or `WM_POINTER*` message at all.
- The XAML taskbar takes button input through its lifted-input pipeline, which does not pass through
  `GetMessage`/`PeekMessage`. No `WH_GETMESSAGE` (or `WH_MOUSE`) hook can intercept it, wherever it is
  injected.

Options considered: a low-level mouse hook (`WH_MOUSE_LL`) in the host that swallows left clicks over
the Start button, or patching XAML input inside explorer. The decision for now is to leave taskbar
clicks unsupported and rely on the Windows key. A `WH_MOUSE_LL` hook in the host is the robust option
if this is revisited; it would supersede this ADR and make the injected DLL unnecessary for clicks.
