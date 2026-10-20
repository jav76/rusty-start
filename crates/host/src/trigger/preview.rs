/// There is no system Start menu to suppress outside Windows.
pub fn suppress_native_start_menu() {}

/// Placeholder guard so the host's startup code is identical on every platform.
pub struct TriggerGuard;

/// Opens the menu once. The toggle is queued and runs when the event loop starts.
pub fn install(on_toggle: impl Fn() + Send + 'static) -> anyhow::Result<TriggerGuard> {
    on_toggle();
    Ok(TriggerGuard)
}
