# 0003: Tab-Separated `usage.tsv` for Pinned Apps and Launch History

## Status

Accepted (2026-10-20).

## Context

Pinned apps, the Recent section and frecency ranking need state that survives restarts. It is
written on every launch and pin toggle, read once at startup, and keyed by launch ID: an
AppUserModelID (`Microsoft.WindowsCalculator_8wekyb3d8bbwe!App`) or a known-folder path
(`{6D809377-...}\Tools\app.exe`). `config.toml` is hand-parsed and holds user settings, not data
the host rewrites constantly, and the host has no serialization dependency.

## Decision

Store it in `usage.tsv` next to `config.toml` (`%APPDATA%\RustyStart` on Windows), one record per
line, fields separated by tabs:

```
# Rusty Start pinned apps and launch history. Fields are tab-separated.
version	1
pin	<launch ID>
launch	<count>	<last launch, Unix seconds>	<launch ID>
```

- `pin` lines keep pin order. `launch` lines are sorted by launch ID so the file diffs cleanly.
- The launch ID is the last field, so it may contain spaces, backslashes and `!`. File names and
  AppUserModelIDs cannot contain tabs or newlines; IDs that do are not saved.
- Unknown record types and malformed lines are skipped, so later versions can add records.
- A missing or different `version` fails the load. The host then keeps pins in memory only for that
  session, so a newer or damaged file is never overwritten with less data.
- Writes go through a background thread (`rusty-start-usage`) that writes a temporary file and
  renames it over `usage.tsv`, so a crash never leaves a truncated file.

## Amendment (2026-10-20): Linux launch IDs

On Linux the launch ID is the desktop file ID from the Desktop Entry specification, for example
`org.kde.konsole.desktop` or `kde4-konsole.desktop` for `kde4/konsole.desktop`. It contains no tabs
or newlines, so the format is unchanged and needs no version bump. A file is never shared between
platforms, so the two kinds of ID never mix.

## Consequences

- No new dependency, and the format is easy to read and edit by hand.
- Pins and history for uninstalled apps stay in the file and come back if the app is reinstalled.
- Changing the meaning of a field needs a version bump and a migration in `Usage::parse`.
