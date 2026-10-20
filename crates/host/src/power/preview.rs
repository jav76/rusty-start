use super::PowerAction;

pub fn perform(action: PowerAction) {
    eprintln!("preview mode: would {}", action.label().to_lowercase());
}
