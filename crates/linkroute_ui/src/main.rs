#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod named_pipe_client;
mod service_bridge;
mod tray;

use linkroute_core::{response_to_json, IpcRequest, IpcResponse};
use service_bridge::{status_text, ServiceBridge};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let command = std::env::args().nth(1).unwrap_or_else(default_command);
    let bridge = ServiceBridge::auto();

    match command.as_str() {
        "--tray" => tray::run_tray(bridge),
        "--status" => {
            let status = bridge.status().map_err(|error| error.to_string())?;
            println!("{}", status_text(&status));
            Ok(())
        }
        "--status-json" => {
            let response = bridge.send(IpcRequest::GetStatus).map_err(|error| error.to_string())?;
            print_response_json(&response)
        }
        "--config-json" => {
            let config = bridge.config().map_err(|error| error.to_string())?;
            print_response_json(&IpcResponse::Config { config })
        }
        "--enable" => {
            bridge.enable().map_err(|error| error.to_string())?;
            println!("LinkRoute enabled");
            Ok(())
        }
        "--pause" => {
            bridge.pause().map_err(|error| error.to_string())?;
            println!("LinkRoute paused");
            Ok(())
        }
        "--toggle" => {
            let enabled = bridge.toggle_enabled().map_err(|error| error.to_string())?;
            println!("LinkRoute {}", if enabled { "enabled" } else { "paused" });
            Ok(())
        }
        "--swap" => {
            bridge.swap_outlets().map_err(|error| error.to_string())?;
            println!("LinkRoute outlets swapped");
            Ok(())
        }
        "--diagnostics" => {
            let diagnostics = bridge.diagnostics().map_err(|error| error.to_string())?;
            print!("{diagnostics}");
            Ok(())
        }
        "--service-path" => {
            println!("{}", bridge.service_path().display());
            Ok(())
        }
        "--version" => {
            println!("linkroute-ui {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => Err(format!(
            "unknown command: {other}\nusage: linkroute-ui [--tray|--status|--status-json|--config-json|--enable|--pause|--toggle|--swap|--diagnostics|--service-path|--version]"
        )),
    }
}

fn print_response_json(response: &IpcResponse) -> Result<(), String> {
    let json = response_to_json(response).map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(())
}

fn default_command() -> String {
    if cfg!(windows) { "--tray" } else { "--status" }.to_string()
}
