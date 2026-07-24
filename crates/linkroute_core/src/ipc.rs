use serde::{Deserialize, Serialize};

use crate::{AdapterKind, LinkState, Outlet, RouteConfig};

pub const IPC_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IpcRequest {
    GetStatus,
    GetConfig,
    SetConfig { config: RouteConfig },
    Enable,
    Pause,
    SwapOutlets,
    CheckForUpdate,
    ExportDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IpcResponse {
    Ack,
    Status { status: ServiceSnapshot },
    Config { config: RouteConfig },
    Diagnostics { text: String },
    Error { error: IpcError },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub protocol_version: u32,
    pub service_version: String,
    pub enabled: bool,
    pub backend_name: String,
    pub backend_running: bool,
    pub active_flows: usize,
    pub link_state: LinkState,
    pub adapters: Vec<AdapterSnapshot>,
    pub windivert_ready: bool,
}

impl ServiceSnapshot {
    pub fn new(service_version: impl Into<String>) -> Self {
        Self {
            protocol_version: IPC_PROTOCOL_VERSION,
            service_version: service_version.into(),
            enabled: false,
            backend_name: String::from("none"),
            backend_running: false,
            active_flows: 0,
            link_state: LinkState::all_up(),
            adapters: Vec::new(),
            windivert_ready: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterSnapshot {
    pub id: String,
    pub name: String,
    pub kind: AdapterKind,
    pub outlet: Option<Outlet>,
    pub interface_index: u32,
    pub up: bool,
    pub physical: bool,
}

pub fn request_from_json(text: &str) -> Result<IpcRequest, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn request_to_json(request: &IpcRequest) -> Result<String, serde_json::Error> {
    serde_json::to_string(request)
}

pub fn response_from_json(text: &str) -> Result<IpcResponse, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn response_to_json(response: &IpcResponse) -> Result<String, serde_json::Error> {
    serde_json::to_string(response)
}

#[cfg(test)]
mod tests {
    use crate::ApplicationRule;

    use super::*;

    #[test]
    fn request_roundtrips_set_config() {
        let mut config = RouteConfig::enabled();
        config.application_rules.push(
            ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", Outlet::Wifi).unwrap(),
        );
        let request = IpcRequest::SetConfig { config };
        let json = request_to_json(&request).unwrap();
        let decoded = request_from_json(&json).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn response_roundtrips_status() {
        let status = ServiceSnapshot {
            enabled: true,
            backend_name: String::from("windivert"),
            backend_running: true,
            active_flows: 2,
            windivert_ready: true,
            ..ServiceSnapshot::new("0.1.0")
        };
        let response = IpcResponse::Status { status };
        let json = response_to_json(&response).unwrap();
        let decoded = response_from_json(&json).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn parses_pause_request_json() {
        let request = request_from_json(r#"{"type":"pause"}"#).unwrap();

        assert_eq!(request, IpcRequest::Pause);
    }
}
