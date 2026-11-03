#![expect(clippy::unnecessary_wraps, reason = "signatures match the fallible win32 backend")]

/// Placeholder guard so the host's startup code is identical on every platform.
pub struct InstanceGuard;

pub fn acquire() -> anyhow::Result<Option<InstanceGuard>> {
    Ok(Some(InstanceGuard))
}

/// Preview builds never share a host, so there is nobody to send to.
pub fn send(_request: super::HostRequest) -> anyhow::Result<bool> {
    Ok(false)
}
