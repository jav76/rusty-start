# 0006: Windows Autostart Through the User's `Run` Key

## Status

Accepted (2026-10-21).

## Context

Until now `rusty-start setup` did nothing on Windows, so users started the host by hand after every
sign-in. The Settings Page offers Install, Undo and a Setup Audit on every platform, so Windows
needs one piece of integration setup can add, check and reverse without elevation. The release ships
as a `.zip` that can be extracted anywhere, so the executable path is not fixed.

## Decision

Setup writes one `REG_SZ` value, `RustyStart`, under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, holding the quoted path of the running
executable. Undo deletes that value. The audit compares the value with the running executable,
case-insensitively: equal is installed, absent is missing, anything else is a conflict that Install
overwrites. The value name is Rusty Start's own, so overwriting or deleting it never touches another
program's entry.

Alternatives considered:

- A shortcut in the Startup folder needs COM (`IShellLink`) to write and parse, and users move or
  rename shortcuts, which makes the audit and undo less reliable.
- A Task Scheduler task can start earlier and survive policy changes, but needs the Task Scheduler
  COM API or `schtasks`, and some configurations need elevation.

## Consequences

- No elevation, no new dependency beyond the `Win32_System_Registry` feature, and one value to read,
  write and delete.
- The value appears in Task Manager's Startup apps, where users can turn it off. The audit then
  still reports it as installed, because Windows records the disabled state elsewhere.
- Moving the extracted folder leaves a stale path, which the audit reports as a conflict until
  Install runs again.
