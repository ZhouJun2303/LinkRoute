#![allow(dead_code)]

use linkroute_core::{FlowDescriptor, LinkState, RouteConfig, RouteDecision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendStatus {
    pub name: &'static str,
    pub running: bool,
    pub active_flows: usize,
}

pub trait CaptureBackend {
    fn name(&self) -> &'static str;
    fn start(&mut self, config: &RouteConfig) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn status(&self) -> BackendStatus;
    fn close_managed_flows(&mut self) -> Result<usize, String>;
    fn apply_decision(&mut self, flow: &FlowDescriptor, decision: &RouteDecision, links: LinkState);
}
