use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use linkroute_core::{
    request_to_json, response_from_json, IpcError, IpcRequest, IpcResponse, RouteConfig,
    ServiceSnapshot,
};

use crate::named_pipe_client;

const SERVICE_EXE_NAME: &str = "linkroute-service.exe";

#[derive(Debug, Clone)]
pub struct ServiceBridge {
    service_path: PathBuf,
}

impl ServiceBridge {
    pub fn auto() -> Self {
        match std::env::current_exe() {
            Ok(ui_exe) => Self::for_ui_exe(&ui_exe),
            Err(_) => Self::from_service_path(SERVICE_EXE_NAME),
        }
    }

    pub fn for_ui_exe(ui_exe: &Path) -> Self {
        let candidates = candidate_service_paths(ui_exe);
        let service_path = candidates
            .iter()
            .find(|path| path.is_file())
            .cloned()
            .or_else(|| candidates.into_iter().next())
            .unwrap_or_else(|| PathBuf::from(SERVICE_EXE_NAME));

        Self { service_path }
    }

    pub fn from_service_path(path: impl Into<PathBuf>) -> Self {
        Self { service_path: path.into() }
    }

    pub fn service_path(&self) -> &Path {
        &self.service_path
    }

    pub fn send(&self, request: IpcRequest) -> Result<IpcResponse, ServiceBridgeError> {
        let json = request_to_json(&request)
            .map_err(|error| ServiceBridgeError::Json(error.to_string()))?;

        if let Ok(Some(response_text)) = named_pipe_client::send_request_json(&json) {
            return parse_response(&response_text);
        }

        let response_text = self.send_process_json(&json)?;
        parse_response(&response_text)
    }

    fn send_process_json(&self, json: &str) -> Result<String, ServiceBridgeError> {
        let request_path = temp_request_path();
        fs::write(&request_path, json.as_bytes()).map_err(|error| ServiceBridgeError::Io {
            action: "write request",
            path: request_path.clone(),
            message: error.to_string(),
        })?;

        let output_result = Command::new(&self.service_path)
            .arg("--handle-request-file")
            .arg(&request_path)
            .output();
        let _ = fs::remove_file(&request_path);

        let output = output_result.map_err(|error| ServiceBridgeError::Io {
            action: "start service command",
            path: self.service_path.clone(),
            message: error.to_string(),
        })?;

        if !output.status.success() {
            return Err(ServiceBridgeError::ServiceExit {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        String::from_utf8(output.stdout)
            .map_err(|error| ServiceBridgeError::Json(error.to_string()))
    }

    pub fn status(&self) -> Result<ServiceSnapshot, ServiceBridgeError> {
        match self.send(IpcRequest::GetStatus)? {
            IpcResponse::Status { status } => Ok(status),
            response => {
                Err(ServiceBridgeError::UnexpectedResponse { expected: "status", response })
            }
        }
    }

    pub fn config(&self) -> Result<RouteConfig, ServiceBridgeError> {
        match self.send(IpcRequest::GetConfig)? {
            IpcResponse::Config { config } => Ok(config),
            response => {
                Err(ServiceBridgeError::UnexpectedResponse { expected: "config", response })
            }
        }
    }

    pub fn enable(&self) -> Result<(), ServiceBridgeError> {
        self.expect_ack(IpcRequest::Enable, "ack")
    }

    pub fn pause(&self) -> Result<(), ServiceBridgeError> {
        self.expect_ack(IpcRequest::Pause, "ack")
    }

    pub fn swap_outlets(&self) -> Result<(), ServiceBridgeError> {
        self.expect_ack(IpcRequest::SwapOutlets, "ack")
    }

    pub fn diagnostics(&self) -> Result<String, ServiceBridgeError> {
        match self.send(IpcRequest::ExportDiagnostics)? {
            IpcResponse::Diagnostics { text } => Ok(text),
            response => {
                Err(ServiceBridgeError::UnexpectedResponse { expected: "diagnostics", response })
            }
        }
    }

    pub fn toggle_enabled(&self) -> Result<bool, ServiceBridgeError> {
        let status = self.status()?;
        if status.enabled {
            self.pause()?;
            Ok(false)
        } else {
            self.enable()?;
            Ok(true)
        }
    }

    fn expect_ack(
        &self,
        request: IpcRequest,
        expected: &'static str,
    ) -> Result<(), ServiceBridgeError> {
        match self.send(request)? {
            IpcResponse::Ack => Ok(()),
            response => Err(ServiceBridgeError::UnexpectedResponse { expected, response }),
        }
    }
}

#[derive(Debug)]
pub enum ServiceBridgeError {
    Io { action: &'static str, path: PathBuf, message: String },
    Json(String),
    Service(IpcError),
    ServiceExit { code: Option<i32>, stderr: String },
    UnexpectedResponse { expected: &'static str, response: IpcResponse },
}

impl fmt::Display for ServiceBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { action, path, message } => {
                write!(formatter, "could not {action} at {}: {message}", path.display())
            }
            Self::Json(message) => write!(formatter, "IPC JSON error: {message}"),
            Self::Service(error) => {
                write!(formatter, "service error {}: {}", error.code, error.message)
            }
            Self::ServiceExit { code, stderr } => {
                write!(formatter, "service command exited with code {code:?}: {stderr}")
            }
            Self::UnexpectedResponse { expected, response } => {
                write!(formatter, "expected {expected} response, got {response:?}")
            }
        }
    }
}

impl std::error::Error for ServiceBridgeError {}

pub fn status_text(status: &ServiceSnapshot) -> String {
    format!(
        "LinkRoute {}\nRouting: {}\nBackend: {} ({})\nWinDivert: {}\nEthernet: {}\nWi-Fi: {}\nVPN: {}\nActive flows: {}",
        status.service_version,
        if status.enabled { "enabled" } else { "paused" },
        status.backend_name,
        if status.backend_running { "running" } else { "stopped" },
        yes_no(status.windivert_ready),
        up_down(status.link_state.ethernet_up),
        up_down(status.link_state.wifi_up),
        if status.link_state.vpn_active { "active" } else { "inactive" },
        status.active_flows
    )
}

fn parse_response(text: &str) -> Result<IpcResponse, ServiceBridgeError> {
    let response = response_from_json(text.trim())
        .map_err(|error| ServiceBridgeError::Json(error.to_string()))?;
    match response {
        IpcResponse::Error { error } => Err(ServiceBridgeError::Service(error)),
        other => Ok(other),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn up_down(value: bool) -> &'static str {
    if value {
        "up"
    } else {
        "down"
    }
}

fn candidate_service_paths(ui_exe: &Path) -> Vec<PathBuf> {
    let Some(parent) = ui_exe.parent() else {
        return Vec::new();
    };

    let mut candidates = vec![parent.join(SERVICE_EXE_NAME)];
    if parent.file_name().and_then(|name| name.to_str()) == Some("deps") {
        if let Some(debug_dir) = parent.parent() {
            candidates.push(debug_dir.join(SERVICE_EXE_NAME));
        }
    }
    candidates
}

fn temp_request_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("linkroute-ui-{}-{nonce}.json", std::process::id()))
}

#[cfg(test)]
mod tests {
    use linkroute_core::{LinkState, ServiceSnapshot};

    use super::{candidate_service_paths, status_text, ServiceBridge};

    #[test]
    fn service_path_is_next_to_ui_binary() {
        let bridge =
            ServiceBridge::for_ui_exe(std::path::Path::new(r"C:\Tools\LinkRoute\linkroute-ui.exe"));

        assert_eq!(
            bridge.service_path(),
            std::path::Path::new(r"C:\Tools\LinkRoute\linkroute-service.exe")
        );
    }

    #[test]
    fn dev_test_binary_also_checks_debug_directory() {
        let candidates = candidate_service_paths(std::path::Path::new(
            r"D:\repo\target\debug\deps\linkroute_ui-abc.exe",
        ));

        assert_eq!(
            candidates[1],
            std::path::Path::new(r"D:\repo\target\debug\linkroute-service.exe")
        );
    }

    #[test]
    fn status_summary_includes_routing_and_link_state() {
        let status = ServiceSnapshot {
            enabled: true,
            backend_name: String::from("windivert"),
            backend_running: false,
            active_flows: 3,
            windivert_ready: true,
            link_state: LinkState { ethernet_up: true, wifi_up: false, vpn_active: true },
            ..ServiceSnapshot::new("0.1.0")
        };

        let text = status_text(&status);

        assert!(text.contains("Routing: enabled"));
        assert!(text.contains("Wi-Fi: down"));
        assert!(text.contains("VPN: active"));
    }
}
