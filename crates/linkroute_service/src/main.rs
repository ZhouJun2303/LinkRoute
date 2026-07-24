mod backend;
mod config_store;
mod control;
mod ipc_handler;
mod windivert;

use std::net::{IpAddr, Ipv4Addr};

use config_store::{ConfigSource, ConfigStore};
use control::ServiceController;
use ipc_handler::handle_ipc_request;
use linkroute_core::{
    request_from_json, response_to_json, ApplicationRule, FlowKey, IpcResponse, Outlet, Protocol,
    RouteAction, RouteConfig,
};
use windivert::{WindivertBackend, WindivertProbe, WindivertRuntime};

fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "--run".to_string());

    match command.as_str() {
        "--run" => run_service(),
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
                "usage: linkroute-service [--run|--check-config|--write-default-config|--status-json|--handle-request-json <json>|--handle-request-file <path>|--probe-windivert|--load-windivert|--self-test|--version]"
            );
            std::process::exit(2);
        }
    }
}

fn run_service() {
    let store = ConfigStore::program_data();
    let loaded = match store.load_or_default() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("could not load config: {error}");
            std::process::exit(1);
        }
    };
    report_config_source(&loaded.source, loaded.backup_path.as_deref());

    let backend = WindivertBackend::new();
    let mut controller = ServiceController::new(loaded.config, Box::new(backend));

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
    let store = ConfigStore::program_data();
    let loaded = match store.load_or_default() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("could not load config: {error}");
            std::process::exit(1);
        }
    };
    let controller = ServiceController::new(loaded.config, Box::new(WindivertBackend::new()));
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
