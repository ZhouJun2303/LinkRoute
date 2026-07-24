#![cfg_attr(windows, windows_subsystem = "windows")]

use linkroute_core::RouteConfig;

fn main() {
    let config = RouteConfig::default();
    let issues = config.validate();

    if !issues.is_empty() {
        eprintln!("LinkRoute UI could not load config: {issues:?}");
        std::process::exit(1);
    }

    show_shell_notice();
}

fn show_shell_notice() {
    #[cfg(windows)]
    {
        // The real Win32 window and tray host will be added after the packet spike passes.
        if let Err(error) = std::process::Command::new("cmd")
            .args([
                "/C",
                "echo LinkRoute UI scaffold. Native window/tray implementation is pending.",
            ])
            .status()
        {
            eprintln!("failed to show UI scaffold notice: {error}");
        }
    }

    #[cfg(not(windows))]
    {
        println!("LinkRoute UI scaffold. Native window/tray implementation is pending.");
    }
}
