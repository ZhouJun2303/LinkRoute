mod adapters;
mod backend;
mod config_store;
mod control;
mod ipc_handler;
mod named_pipe;
mod nat;
mod process_context;
mod windivert;
mod windows_service;

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use config_store::{ConfigSource, ConfigStore};
use control::ServiceController;
use ipc_handler::handle_ipc_request;
use linkroute_core::{
    request_from_json, response_to_json, ApplicationRule, FlowKey, IpcRequest, IpcResponse, Outlet,
    Protocol, RouteAction, RouteConfig,
};
use windivert::{WindivertBackend, WindivertProbe, WindivertRuntime};

const SERVICE_NAME: &str = "LinkRoute";

fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "--run".to_string());

    match command.as_str() {
        "--run" => run_service(),
        "--run-service" => run_windows_service(),
        "--install-service" => install_service(),
        "--uninstall-service" => uninstall_service(),
        "--start-service" => start_service(),
        "--stop-service" => stop_service(),
        "--serve-pipe-once" => serve_pipe_once(),
        "--check-config" => check_config(),
        "--write-default-config" => write_default_config(),
        "--status-json" => status_json(),
        "--handle-request-json" => handle_request_json(),
        "--handle-request-file" => handle_request_file(),
        "--probe-windivert" => probe_windivert(),
        "--load-windivert" => load_windivert(),
        "--self-test" => self_test(),
        "--version" => println!("linkroute-service {}", env!("CARGO_PKG_VERSION")),
        other => {
            eprintln!("unknown command: {other}");
            eprintln!(
                "usage: linkroute-service [--run|--run-service|--install-service|--uninstall-service|--start-service|--stop-service|--serve-pipe-once|--check-config|--write-default-config|--status-json|--handle-request-json <json>|--handle-request-file <path>|--probe-windivert|--load-windivert|--self-test|--version]"
            );
            std::process::exit(2);
        }
    }
}

fn run_service() {
    let (mut controller, source, backup_path) = match load_controller() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("could not load service: {error}");
            std::process::exit(1);
        }
    };
    report_config_source(&source, backup_path.as_deref());

    if let Err(error) = controller.start() {
        eprintln!("service failed to start: {error}");
        std::process::exit(1);
    }

    let status = controller.backend_status();
    println!(
        "LinkRoute service scaffold started with backend={} running={} flows={}",
        status.name, status.running, status.active_flows
    );
}

fn run_windows_service() {
    if let Err(error) = windows_service::run_service_dispatcher() {
        eprintln!("service dispatcher failed: {error}");
        std::process::exit(1);
    }
}

pub(crate) fn start_service_controller() -> Result<ServiceController, String> {
    let (mut controller, _, _) = load_controller()?;
    controller.start()?;
    Ok(controller)
}

pub(crate) fn serve_control_pipe_once(
    controller: &ServiceController,
    timeout: Duration,
) -> Result<bool, String> {
    let store = ConfigStore::program_data();
    named_pipe::serve_once(timeout, |request| match request {
        IpcRequest::GetStatus => runtime_status_response(controller, &store),
        other => handle_ipc_request(&store, other),
    })
}

fn runtime_status_response(controller: &ServiceController, store: &ConfigStore) -> IpcResponse {
    let windivert_ready =
        WindivertProbe::probe_near_exe().map(|probe| probe.ready()).unwrap_or(false);
    let mut status = controller.snapshot(env!("CARGO_PKG_VERSION"), windivert_ready);
    if let Ok(loaded) = store.load_or_default() {
        status.enabled = loaded.config.enabled;
    }
    IpcResponse::Status { status }
}

fn load_controller() -> Result<(ServiceController, ConfigSource, Option<PathBuf>), String> {
    let store = ConfigStore::program_data();
    let loaded = store.load_or_default().map_err(|error| error.to_string())?;
    let mut controller = ServiceController::new(loaded.config, Box::new(WindivertBackend::new()));
    refresh_adapters(&mut controller);
    Ok((controller, loaded.source, loaded.backup_path))
}

fn refresh_adapters(controller: &mut ServiceController) {
    match adapters::enumerate_adapters() {
        Ok((inventory, selection)) => controller.set_adapters(inventory, selection),
        Err(error) => eprintln!("warning: could not enumerate network adapters: {error}"),
    }
}

fn install_service() {
    exit_on_service_command(install_service_command());
}

fn uninstall_service() {
    exit_on_service_command(uninstall_service_command());
}

fn start_service() {
    exit_on_service_command(start_service_command());
}

fn stop_service() {
    exit_on_service_command(stop_service_command());
}

fn serve_pipe_once() {
    let controller = match start_service_controller() {
        Ok(controller) => controller,
        Err(error) => {
            eprintln!("could not start temporary pipe service: {error}");
            std::process::exit(1);
        }
    };
    match serve_control_pipe_once(&controller, Duration::from_secs(10)) {
        Ok(true) => println!("control pipe request handled"),
        Ok(false) => {
            eprintln!("timed out waiting for control pipe request");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("control pipe failed: {error}");
            std::process::exit(1);
        }
    }
}

fn install_service_command() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let bin_path = service_bin_path(&exe);
    let exists = sc_query_installed()?;
    let args = if exists {
        vec![
            String::from("config"),
            String::from(SERVICE_NAME),
            String::from("binPath="),
            bin_path,
            String::from("start="),
            String::from("auto"),
            String::from("DisplayName="),
            String::from("LinkRoute Service"),
        ]
    } else {
        vec![
            String::from("create"),
            String::from(SERVICE_NAME),
            String::from("binPath="),
            bin_path,
            String::from("start="),
            String::from("auto"),
            String::from("DisplayName="),
            String::from("LinkRoute Service"),
        ]
    };
    run_sc(&args, "install service")?;

    let description_args = vec![
        String::from("description"),
        String::from(SERVICE_NAME),
        String::from("Routes selected application and domain traffic over Ethernet or Wi-Fi."),
    ];
    if let Err(error) = run_sc(&description_args, "set service description") {
        eprintln!("warning: {error}");
    }

    println!("LinkRoute service installed");
    Ok(())
}

fn uninstall_service_command() -> Result<(), String> {
    stop_service_command()?;
    if !sc_query_installed()? {
        println!("LinkRoute service is not installed");
        return Ok(());
    }

    let args = vec![String::from("delete"), String::from(SERVICE_NAME)];
    run_sc(&args, "delete service")?;
    println!("LinkRoute service deleted");
    Ok(())
}

fn start_service_command() -> Result<(), String> {
    if !sc_query_installed()? {
        return Err(String::from("LinkRoute service is not installed"));
    }

    let args = vec![String::from("start"), String::from(SERVICE_NAME)];
    match run_sc(&args, "start service") {
        Ok(()) => println!("LinkRoute service start requested"),
        Err(error) if error.contains("1056") => println!("LinkRoute service is already running"),
        Err(error) => return Err(error),
    }
    Ok(())
}

fn stop_service_command() -> Result<(), String> {
    if !sc_query_installed()? {
        println!("LinkRoute service is not installed");
        return Ok(());
    }

    let args = vec![String::from("stop"), String::from(SERVICE_NAME)];
    match run_sc(&args, "stop service") {
        Ok(()) => println!("LinkRoute service stop requested"),
        Err(error) if error.contains("1062") => println!("LinkRoute service is already stopped"),
        Err(error) => eprintln!("warning: {error}"),
    }
    Ok(())
}

fn service_bin_path(exe: &Path) -> String {
    format!("\"{}\" --run-service", exe.display())
}

fn sc_query_installed() -> Result<bool, String> {
    let output = Command::new("sc.exe")
        .args(["query", SERVICE_NAME])
        .output()
        .map_err(|error| format!("could not query service: {error}"))?;
    Ok(output.status.success())
}

fn run_sc(args: &[String], action: &str) -> Result<(), String> {
    let output = Command::new("sc.exe")
        .args(args)
        .output()
        .map_err(|error| format!("could not run sc.exe for {action}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if stderr.is_empty() { stdout } else { stderr };
    Err(format!("sc.exe failed to {action}: {details}"))
}

fn exit_on_service_command(result: Result<(), String>) {
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn check_config() {
    let store = ConfigStore::program_data();
    match store.load_or_default() {
        Ok(loaded) => {
            report_config_source(&loaded.source, loaded.backup_path.as_deref());
            println!("config is valid");
        }
        Err(error) => {
            eprintln!("config check failed: {error}");
            std::process::exit(1);
        }
    }
}

fn write_default_config() {
    let store = ConfigStore::program_data();
    if let Err(error) = store.save(&RouteConfig::default()) {
        eprintln!("could not write default config: {error}");
        std::process::exit(1);
    }
    println!("default config written to {}", store.config_path().display());
}

fn report_config_source(source: &ConfigSource, backup_path: Option<&std::path::Path>) {
    match source {
        ConfigSource::Existing => println!("config loaded from disk"),
        ConfigSource::DefaultMissing => println!("config missing; using defaults"),
        ConfigSource::DefaultRecovered => {
            if let Some(path) = backup_path {
                println!("invalid config backed up to {}; using defaults", path.display());
            } else {
                println!("invalid config; using defaults");
            }
        }
    }
}

fn handle_request_json() {
    let Some(json) = std::env::args().nth(2) else {
        eprintln!("missing request JSON");
        std::process::exit(2);
    };
    handle_request_text(&json);
}

fn handle_request_file() {
    let Some(path) = std::env::args().nth(2) else {
        eprintln!("missing request file path");
        std::process::exit(2);
    };
    let json = match std::fs::read_to_string(path) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("could not read request file: {error}");
            std::process::exit(1);
        }
    };
    handle_request_text(&json);
}

fn handle_request_text(json: &str) {
    let request = match request_from_json(json) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("invalid request JSON: {error}");
            std::process::exit(2);
        }
    };
    let store = ConfigStore::program_data();
    let response = handle_ipc_request(&store, request);
    match response_to_json(&response) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("could not serialize response: {error}");
            std::process::exit(1);
        }
    }
}

fn status_json() {
    let (controller, _, _) = match load_controller() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("could not load service status: {error}");
            std::process::exit(1);
        }
    };
    let windivert_ready =
        WindivertProbe::probe_near_exe().map(|probe| probe.ready()).unwrap_or(false);
    let response = IpcResponse::Status {
        status: controller.snapshot(env!("CARGO_PKG_VERSION"), windivert_ready),
    };

    match response_to_json(&response) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("could not serialize status: {error}");
            std::process::exit(1);
        }
    }
}

fn probe_windivert() {
    match WindivertProbe::probe_near_exe() {
        Ok(probe) if probe.ready() => {
            println!("WinDivert files found in {}", probe.search_dir.display());
        }
        Ok(probe) => {
            eprintln!(
                "WinDivert files missing in {}: {}",
                probe.search_dir.display(),
                probe.missing_files().join(", ")
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("WinDivert probe failed: {error}");
            std::process::exit(1);
        }
    }
}

fn load_windivert() {
    match WindivertProbe::probe_near_exe() {
        Ok(probe) if probe.ready() => match WindivertRuntime::load_from_dir(&probe.search_dir) {
            Ok(runtime) => println!(
                "WinDivert DLL loaded from {}; symbols={}",
                runtime.dll_path().display(),
                runtime.resolved_symbol_count()
            ),
            Err(error) => {
                eprintln!("WinDivert DLL load failed: {error}");
                std::process::exit(1);
            }
        },
        Ok(probe) => {
            eprintln!(
                "WinDivert files missing in {}: {}",
                probe.search_dir.display(),
                probe.missing_files().join(", ")
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("WinDivert probe failed: {error}");
            std::process::exit(1);
        }
    }
}

fn self_test() {
    let mut config = RouteConfig::enabled();
    config.application_rules.push(
        ApplicationRule::new(
            "app-self-test",
            "Self Test",
            r"C:\Apps\self-test.exe",
            Outlet::Ethernet,
        )
        .expect("static self-test path must be valid"),
    );

    let mut controller = ServiceController::new(config, Box::new(WindivertBackend::new()));
    controller.start().expect("backend scaffold should start");
    let decision = controller.decide_and_record_flow(
        100,
        FlowKey {
            protocol: Protocol::Tcp,
            local_addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
            local_port: 50000,
            remote_addr: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)),
            remote_port: 443,
        },
        1000,
        Some(String::from(r"C:\Apps\self-test.exe")),
        None,
    );

    if matches!(decision.action, RouteAction::UseOutlet(Outlet::Ethernet)) {
        println!("self-test ok: flow recorded and routed to ethernet");
    } else {
        eprintln!("self-test failed: unexpected decision {decision:?}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::service_bin_path;

    #[test]
    fn service_bin_path_quotes_executable_and_adds_run_service() {
        let bin_path =
            service_bin_path(Path::new(r"C:\Program Files\LinkRoute\linkroute-service.exe"));

        assert_eq!(bin_path, r#""C:\Program Files\LinkRoute\linkroute-service.exe" --run-service"#);
    }
}
