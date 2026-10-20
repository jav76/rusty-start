# Agent guide

Rules for any coding agent (Claude Code, Copilot, Antigravity, Codex) working in this repository.
Tooling enforces most of the formatting; this file covers what tools cannot. When a rule here
conflicts with a tool, fix the tool config instead of working around it.

## Commands

Run these before calling a change done. CI runs them in a `debian:bookworm` container on the
project's self-hosted Linux runner, and checks the Windows backend with
`cargo clippy --target x86_64-pc-windows-msvc`. Windows-only tests run only on Windows, so run
`cargo test --workspace --locked` there for changes to Windows code.

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p rusty-start-host --all-targets --locked --features preview -- -D warnings
cargo test --workspace --locked
```

- `cargo run -p rusty-start-host` runs the host. Add `--features preview` to run the preview
  backend (sample apps, synthetic backdrop, logged power actions) on any OS.
- Linux builds need `pkg-config` and `libfontconfig-dev`.
- Formatting comes from `rustfmt.toml`, `.editorconfig` and `.gitattributes` (LF everywhere).
  Use stable rustfmt only, with no nightly-only options.
- Lints are configured once in the root `Cargo.toml` under `[workspace.lints]`. Every crate opts
  in with `[lints] workspace = true`.

## Repository map

| Path                       | What lives there                                                     |
| -------------------------- | -------------------------------------------------------------------- |
| `crates/host`              | Resident process: Slint UI, GPU shaders, triggers, app index, search |
| `crates/host/ui`           | Slint markup, compiled by `build.rs`                                  |
| `crates/hook`              | DLL injected into `explorer.exe`. Treat it as hostile territory       |
| `crates/protocol`          | Names and shared-memory layout used by both host and hook             |
| `docs/adr`                 | Architecture Decision Records                                         |
| `packaging/linux`          | Desktop entry and D-Bus service file the `.deb` installs              |
| `GLOSSARY.md`              | Domain terms. Use them in code, comments and docs                     |

## Rust

**Language and toolchain**
- Edition 2024, MSRV from `rust-version` in the root `Cargo.toml`. Do not use features newer
  than the MSRV, and bump it deliberately in its own change.
- Prefer modern syntax: let chains, `let ... else`, `&raw const` / `&raw mut`, `ptr.cast()`,
  inline format args (`"{name}"`), `is_none_or`, `LazyLock` over `lazy_static`.

**Errors**
- The host uses `anyhow`. Add context at every fallible boundary:
  `.context("failed to open the Applications folder")`. Messages are lowercase, describe the
  failed action, and have no trailing period.
- No `unwrap()` outside tests. `expect("...")` is allowed only for true invariants, and the
  message states the invariant (`"surface initialized above"`).
- No `todo!()` or `unimplemented!()` in committed code. Return an error or leave the feature out.
- Log recoverable failures once, where they are handled: `eprintln!("failed to X: {error:#}")`.
  Never log and also return the same error.
- Library-style code with callers that match on errors (for example `crates/protocol`) uses a
  typed error enum instead of `anyhow`.

**Style**
- Annotate the type on `let` bindings whose type is not obvious from the right-hand side. This is
  house style, which is why `clippy::redundant_type_annotations` is off.
- Spell names out: `error`, `window`, `start_menu`, not `e`, `w`, `sm`. Short names are fine for
  closure parameters with obvious meaning and for math in shaders.
- Group imports as `std`, then external crates, then `crate`/`super`, with a blank line between
  groups and one `use` per crate root with nested braces. Avoid glob imports except
  `use super::*;` in test modules.
- Keep functions small and single-purpose. If a function needs section comments, split it.
- Silence a lint locally with `#[expect(lint, reason = "...")]`, never a bare `#[allow]`. Prefer
  fixing the code.
- Comments explain why, not what. Do not add comments that restate the code. Public items get a
  `///` doc comment when their contract is not obvious from the name and signature.

**Structure**
- Platform-specific code follows the existing split: `mod.rs` re-exports from `win32.rs`,
  `linux.rs` or `preview.rs` with identical public APIs, selected by the `backend_win32`,
  `backend_linux` and `backend_preview` cfgs that `crates/host/build.rs` emits. The `preview`
  feature forces `backend_preview` on any OS. Use `cfg(windows)` only for code that depends on the
  OS rather than the backend (for example the config folder). Keep the preview side compiling and
  behaving plausibly so the UI can be developed without desktop integration.
- On Linux, probe each capability at runtime (a D-Bus name, a protocol, an authorization) and give
  it a defined fallback. One binary serves every desktop; never branch on `XDG_CURRENT_DESKTOP`
  except to choose setup text or log messages.
- Pure logic (search ranking, config parsing, geometry) stays platform-independent and unit
  tested. Push Win32 calls to the edges.
- Functions that must run on the UI thread say so in their module or item docs. Hand work back
  to the UI thread with `slint::Weak::upgrade_in_event_loop`. Never block it on I/O, COM
  enumeration or process launch; use a named background thread (`rusty-start-<purpose>`).
- New dependencies go in `[workspace.dependencies]` with `default-features = false` where
  practical and only the features used. Keep `windows` crate feature lists sorted.

## Unsafe code and Win32

- Every `unsafe` block has a `// SAFETY:` comment stating why each precondition holds. Every
  `unsafe fn` has a `# Safety` doc section. Keep blocks as small as the API allows.
- Wrap every handle or resource (HWND, HDC, HBITMAP, COM apartment, mapped view, hook) in an
  owning type whose `Drop` releases it. When drop order matters, order the fields and say so.
- Check every return value. Convert failures with `windows::core::Error::from_thread()` or the
  crate's `Result`, and add context. Ignoring a result needs `let _ =` and a reason when it is
  not obvious (cleanup in `Drop` is obvious).
- COM is initialized per thread through a `!Send` guard. Do not call COM APIs on threads that
  have not initialized it.
- Never fight a framework's window management after the fact. If winit or Slint needs different
  window behavior, configure it at creation (for example the winit window attributes hook).
- Cross-process names, message IDs and shared-memory layouts live only in `crates/protocol`.
  A layout change updates both the host and the hook in the same change.

## Explorer hook DLL (`crates/hook`)

Code here runs inside `explorer.exe`. A bug crashes or freezes the user's shell.

- Every exported or callback entry point wraps its body in `std::panic::catch_unwind`. Never set
  `panic = "abort"` in any profile.
- Hook procedures do O(1) work: read shared memory, compare, post a message. No allocation in the
  hot path, no blocking calls, no locks held across calls, no UI Automation, no `SendMessage`.
- Always call the next hook. Never swallow messages other than the documented Start button
  clicks (ADR 0002).
- The host must stay usable when the hook is missing or fails to inject.

## Slint (`crates/host/ui`)

- 4-space indentation, kebab-case names, one exported component per file.
- Declare property direction explicitly (`in`, `out`, `in-out`). Default to `in`. Use `in-out`
  only when both Rust and the UI write it.
- Keep markup declarative. Search, launching, persistence and timing logic belong in Rust; the
  UI raises callbacks named as verbs (`launch`, `dismiss`, `search`).
- Rust triggers UI-side actions through `public function`s (`reset-search`), not by poking
  several properties.
- Sizes are logical pixels. Never hard-code physical pixels or assume a scale factor.
- Anything animated in both Slint and the shaders (card lift, scale, opacity) derives from the
  same progress value and easing curve, so the content and the glass never drift apart.
- Keyboard first: every action reachable by mouse is reachable by keyboard, and focus is never
  stranded. Custom interactive elements set `accessible-role` and `accessible-label`.
- Comments in `.slint` files follow the same rule as Rust: only when the intent is not obvious.

## GLSL shaders (`crates/host/src/glass.rs`)

- Shader bodies are version-agnostic. The version header (`330 core` or `300 es`) is prepended
  at compile time, so use only features available in both.
- Output premultiplied alpha. `progress = 0` must produce a fully transparent frame so the
  window can appear and disappear without a visible change.
- Look up uniforms once at program creation. Do not allocate, compile or upload unchanged data
  per frame.
- Save and restore every piece of GL state touched in `BeforeRendering`; FemtoVG renders next and
  assumes its own state.
- Name tunable constants instead of scattering magic numbers, and keep the GLOSSARY terms (Fog
  Veil, Liquid-Glass Card, Optical Condense) in comments.

## Performance

This project exists to be faster than the built-in Start menu. Treat latency as a feature.

- The path from trigger to first rendered frame does no disk I/O, shell enumeration or
  allocation proportional to installed apps. Precompute off the UI thread and reuse buffers.
- Per-keystroke work is bounded by the app count with no I/O.
- When a change touches the open/close path or rendering, state the before/after measurement or
  observation in the PR.

## Testing

- Unit-test pure logic next to the code (`#[cfg(test)] mod tests`). Test names are sentences
  describing behavior: `exact_match_ranks_above_prefix_match`.
- Tests are deterministic and need no Windows desktop, network or installed apps.
- Windows integration (hooks, focus, DWM, launching) is verified manually. Describe what was
  checked in the PR.
- Do not drive the user's mouse or keyboard, or stop their running `rusty-start`, without asking.

## Docs, commits and PRs

- Keep `README.md` in sync with behavior, controls and configuration. Add new domain terms to
  `GLOSSARY.md`.
- Record hard-to-reverse decisions (new process boundaries, injection strategy, rendering
  pipeline, file formats) as `docs/adr/NNNN-short-title.md`.
- Write concise, practical prose. No em-dashes: use hyphens, colons or separate sentences.
- Commit messages use Conventional Commits: `feat(host): ...`, `fix(hook): ...`, `docs: ...`,
  with a scope matching the crate. The summary is imperative and under 72 characters.
- No AI attribution in commits or PRs (`Co-Authored-By`, "Generated with" footers).
- PR descriptions cover a summary, evidence that it works, and whether the change is easy to
  revert along with its blast radius.
- GitHub Actions pin third-party actions to a full commit SHA with the version in a comment.
