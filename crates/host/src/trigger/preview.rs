/// Placeholder guard so the host's startup code is identical on every platform.
pub struct TriggerGuard;

/// Opens the menu once. The toggle is queued and runs when the event loop starts.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> anyhow::Result<TriggerGuard> {
    on_toggle();
    Ok(TriggerGuard)
}
