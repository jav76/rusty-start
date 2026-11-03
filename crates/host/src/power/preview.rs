use super::PowerAction;

/// Preview builds offer every action.
pub fn available() -> Vec<PowerAction> {
    PowerAction::ALL.to_vec()
}

/// Logs instead of acting, so a preview build never ends the developer's session.
#[expect(clippy::unnecessary_wraps, reason = "signature matches the fallible backends")]
pub fn run(action: PowerAction) -> anyhow::Result<()> {
    log::info!("preview mode: would {}", action.label().to_lowercase());
    Ok(())
}
