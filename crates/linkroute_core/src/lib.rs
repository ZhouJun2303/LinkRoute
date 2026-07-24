#![forbid(unsafe_code)]

pub mod adapter;
pub mod config;
pub mod dns;
pub mod engine;
pub mod flow;
pub mod ipc;
pub mod model;
pub mod normalize;
pub mod packet;

pub use adapter::{AdapterInventory, AdapterKind, AdapterSelection, NetworkAdapter};
pub use config::{ConfigIssue, RouteConfig};
pub use dns::{DnsCache, DnsRecord};
pub use engine::RouteEngine;
pub use flow::{FlowKey, FlowRecord, FlowTable};
pub use ipc::{
    request_from_json, request_to_json, response_from_json, response_to_json, AdapterSnapshot,
    IpcError, IpcRequest, IpcResponse, ServiceSnapshot, IPC_PIPE_NAME, IPC_PROTOCOL_VERSION,
};
pub use model::{
    ApplicationRule, DecisionSource, DomainMatchMode, DomainRule, FlowDescriptor, LinkState,
    Outlet, Protocol, RouteAction, RouteDecision,
};
pub use normalize::{
    domain_matches, exe_paths_equal, normalize_domain, normalize_exe_path, NormalizeError,
};
pub use packet::{parse_ip_packet, PacketDirection, PacketParseError, ParsedPacket};
