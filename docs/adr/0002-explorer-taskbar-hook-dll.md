# 0002: In-Process Explorer Hook DLL and Shared Memory for Taskbar Start Interception

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

