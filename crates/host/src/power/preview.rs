use super::PowerAction;

/// Preview builds offer every action.
pub fn available() -> Vec<PowerAction> {
    PowerAction::ALL.to_vec()
}

pub fn perform(action: PowerAction) {
    eprintln!("preview mode: would {}", action.label().to_lowercase());
}
