fn main() {
    // Pin the widget style so a Linux preview build renders the same widgets as Windows.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());

    slint_build::compile_with_config("ui/start_menu.slint", config).expect("failed to compile the Slint UI");
}
