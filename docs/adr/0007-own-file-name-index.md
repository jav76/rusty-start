# 0007: Own Name-Only File Index Instead of Windows Search or Baloo

## Status

Accepted (2026-10-28).

## Context

Search is gaining file results. Each platform has an OS index: Windows Search, reached through OLE
DB (`ISearchQueryHelper`, `ICommand`, `IRowset`), and Baloo on Plasma, which has no D-Bus query API
and needs a `baloosearch6` subprocess per query or a C++ bridge. Both answer in tens to hundreds of
milliseconds when cold, cover only what the user lets them index, and return nothing when indexing
is off. Per-keystroke work must stay within the 1 ms budget, so file search runs on the async search
lane either way.

## Decision

The host keeps its own index of file names, in two steps:

1. Recent files: `shell:recent` on Windows and `recently-used.xbel` on Linux.
2. A name-only index of the user's folders (Desktop, Documents, Downloads, Pictures and the like,
   configurable with `file_search_folders`), built on a background thread, kept current with
   `ReadDirectoryChangesW` or inotify, skipping hidden and heavy folders (`.git`, `node_modules`)
   and capped in depth and count.

Matching reuses the platform-independent ranking in `search.rs` and runs on the async lane.

## Consequences

- Results and latency are the same on both platforms and under the project's control.
- The host owns a crawler and watcher, and holds roughly 10 MB per 100k indexed paths.
- File contents are not searched. The OS index can be added later as another async source.
