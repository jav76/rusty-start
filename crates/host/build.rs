use std::env;

const BACKENDS: [&str; 3] = ["backend_win32", "backend_linux", "backend_preview"];

fn main() {
    select_backend();

    // Pin the widget style so a Linux preview build renders the same widgets as Windows.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());

    slint_build::compile_with_config("ui/start_menu.slint", config).expect("failed to compile the Slint UI");
}

/// Emits exactly one `backend_*` cfg. Platform modules select their implementation with it, so the
/// `preview` feature can force the preview backend on any OS.
fn select_backend() {
    for backend in BACKENDS {
        println!("cargo::rustc-check-cfg=cfg({backend})");
    }

    let target_os: String = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let backend: &str = if env::var_os("CARGO_FEATURE_PREVIEW").is_some() {
        "backend_preview"
    } else {
        match target_os.as_str() {
            "windows" => "backend_win32",
            "linux" => "backend_linux",
            _ => "backend_preview",
        }
    };
    println!("cargo::rustc-cfg={backend}");
}
