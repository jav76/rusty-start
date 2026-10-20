#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible win32 backend")]

/// Placeholder guard so the host's startup code is identical on every platform.
pub struct InstanceGuard;

pub fn acquire() -> anyhow::Result<Option<InstanceGuard>> {
    Ok(Some(InstanceGuard))
}
