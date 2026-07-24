use crate::config_store::ConfigStore;
use crate::control::ServiceController;
use crate::windivert::{WindivertBackend, WindivertProbe};
use linkroute_core::{IpcError, IpcRequest, IpcResponse, RouteConfig};

pub fn handle_ipc_request(store: &ConfigStore, request: IpcRequest) -> IpcResponse {
    match request {
        IpcRequest::GetStatus => status_response(store),
        IpcRequest::GetConfig => match store.load_or_default() {
            Ok(loaded) => IpcResponse::Config { config: loaded.config },
            Err(error) => error_response("config-load-failed", error.to_string()),
        },
        IpcRequest::SetConfig { config } => save_config(store, config),
        IpcRequest::Enable => mutate_config(store, |config| config.enabled = true),
        IpcRequest::Pause => mutate_config(store, |config| config.enabled = false),
        IpcRequest::SwapOutlets => mutate_config(store, RouteConfig::swap_outlets),
        IpcRequest::CheckForUpdate => error_response(
            "update-check-not-wired",
            "update transport is not wired to GitHub Releases yet",
        ),
        IpcRequest::ExportDiagnostics => diagnostics_response(store),
    }
}

fn status_response(store: &ConfigStore) -> IpcResponse {
    let loaded = match store.load_or_default() {
        Ok(loaded) => loaded,
        Err(error) => return error_response("config-load-failed", error.to_string()),
    };
    let controller = ServiceController::new(loaded.config, Box::new(WindivertBackend::new()));
    let windivert_ready =
        WindivertProbe::probe_near_exe().map(|probe| probe.ready()).unwrap_or(false);

    IpcResponse::Status { status: controller.snapshot(env!("CARGO_PKG_VERSION"), windivert_ready) }
}

fn save_config(store: &ConfigStore, config: RouteConfig) -> IpcResponse {
    match store.save(&config) {
        Ok(()) => IpcResponse::Ack,
        Err(error) => error_response("config-save-failed", error.to_string()),
    }
}

fn mutate_config(store: &ConfigStore, mutate: impl FnOnce(&mut RouteConfig)) -> IpcResponse {
    let mut config = match store.load_or_default() {
        Ok(loaded) => loaded.config,
        Err(error) => return error_response("config-load-failed", error.to_string()),
    };
    mutate(&mut config);
    save_config(store, config)
}

fn diagnostics_response(store: &ConfigStore) -> IpcResponse {
    let config_state = match store.load_or_default() {
        Ok(loaded) => format!("config_source={:?}", loaded.source),
        Err(error) => format!("config_error={error}"),
    };
    let windivert = match WindivertProbe::probe_near_exe() {
        Ok(probe) => {
            format!("windivert_ready={} missing={}", probe.ready(), probe.missing_files().join(","))
        }
        Err(error) => format!("windivert_probe_error={error}"),
    };

    IpcResponse::Diagnostics {
        text: format!("LinkRoute {}\n{}\n{}\n", env!("CARGO_PKG_VERSION"), config_state, windivert),
    }
}

fn error_response(code: impl Into<String>, message: impl Into<String>) -> IpcResponse {
    IpcResponse::Error { error: IpcError::new(code, message) }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use linkroute_core::{IpcRequest, IpcResponse, RouteConfig};

    use crate::config_store::ConfigStore;

    use super::handle_ipc_request;

    #[test]
    fn get_config_returns_default_for_missing_file() {
        let dir = temp_dir("get_config");
        let store = ConfigStore::new(dir.clone());

        let response = handle_ipc_request(&store, IpcRequest::GetConfig);

        assert_eq!(response, IpcResponse::Config { config: RouteConfig::default() });
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn enable_persists_config() {
        let dir = temp_dir("enable");
        let store = ConfigStore::new(dir.clone());

        assert_eq!(handle_ipc_request(&store, IpcRequest::Enable), IpcResponse::Ack);
        let config = store.load_or_default().unwrap().config;

        assert!(config.enabled);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn diagnostics_returns_text() {
        let dir = temp_dir("diagnostics");
        let store = ConfigStore::new(dir.clone());

        let response = handle_ipc_request(&store, IpcRequest::ExportDiagnostics);

        assert!(matches!(response, IpcResponse::Diagnostics { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("linkroute_ipc_handler_{name}_{nonce}"))
    }
}
