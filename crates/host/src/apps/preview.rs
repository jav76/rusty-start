use super::App;

const SAMPLE_APPS: [&str; 12] = [
    "Calculator",
    "Command Prompt",
    "File Explorer",
    "Microsoft Edge",
    "Notepad",
    "Paint",
    "PowerShell",
    "Settings",
    "Snipping Tool",
    "Task Manager",
    "Terminal",
    "Visual Studio Code",
];

pub fn load() -> anyhow::Result<Vec<App>> {
    Ok(SAMPLE_APPS.iter().map(|name| App { name: name.to_string(), launch_id: name.to_string() }).collect())
}

pub fn launch(app: &App) {
    eprintln!("preview mode: would launch {}", app.name);
}
