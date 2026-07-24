#![allow(dead_code)]

use std::net::IpAddr;

use crate::backend::{BackendStatus, CaptureBackend};
use linkroute_core::{
    AdapterInventory, AdapterSelection, DnsCache, FlowDescriptor, FlowKey, FlowRecord, FlowTable,
    LinkState, RouteConfig, RouteDecision, RouteEngine,
};

pub struct ServiceController {
    engine: RouteEngine,
    backend: Box<dyn CaptureBackend>,
    links: LinkState,
    dns: DnsCache,
    flows: FlowTable,
    adapters: AdapterInventory,
    adapter_selection: AdapterSelection,
}

impl ServiceController {
    pub fn new(config: RouteConfig, backend: Box<dyn CaptureBackend>) -> Self {
        Self {
            engine: RouteEngine::new(config),
            backend,
            links: LinkState::all_up(),
            dns: DnsCache::default(),
            flows: FlowTable::default(),
            adapters: AdapterInventory::default(),
            adapter_selection: AdapterSelection::default(),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        self.backend.start(self.engine.config())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.engine.config_mut().enabled = false;
        self.flows.clear_managed();
        self.backend.stop()
    }

    pub fn swap_outlets_now(&mut self) -> Result<usize, String> {
        self.engine.config_mut().swap_outlets();
        let local_cleared = self.flows.clear_managed();
        let backend_cleared = self.backend.close_managed_flows()?;
        Ok(local_cleared.max(backend_cleared))
    }

    pub fn observe_dns_answer(
        &mut self,
        now_epoch_seconds: u64,
        queried_domain: &str,
        canonical_names: &[String],
        addresses: &[IpAddr],
        ttl_seconds: u32,
    ) -> Result<(), String> {
        self.dns
            .observe_answer(
                now_epoch_seconds,
                queried_domain,
                canonical_names,
                addresses,
                ttl_seconds,
            )
            .map_err(|error| error.to_string())
    }

    pub fn decide_and_record_flow(
        &mut self,
        now_epoch_seconds: u64,
        key: FlowKey,
        process_id: u32,
        exe_path: Option<String>,
        explicit_domain: Option<String>,
    ) -> RouteDecision {
        let domain = explicit_domain
            .or_else(|| self.dns.best_domain_for_ip(now_epoch_seconds, key.remote_addr));
        let flow = FlowDescriptor {
            protocol: key.protocol,
            exe_path: exe_path.clone(),
            domain: domain.clone(),
            remote_addr: Some(key.remote_addr.to_string()),
        };
        let decision = self.engine.decide(&flow, self.links);

        self.flows.insert(FlowRecord {
            key,
            process_id,
            exe_path,
            domain,
            decision: decision.clone(),
            created_at_epoch_seconds: now_epoch_seconds,
            last_seen_epoch_seconds: now_epoch_seconds,
        });
        self.backend.apply_decision(&flow, &decision, self.links);

        decision
    }

    pub fn decide_flow(&mut self, flow: &FlowDescriptor) {
        let decision = self.engine.decide(flow, self.links);
        self.backend.apply_decision(flow, &decision, self.links);
    }

    pub fn set_links(&mut self, links: LinkState) {
        self.links = links;
    }

    pub fn set_adapters(&mut self, adapters: AdapterInventory, selection: AdapterSelection) {
        self.links = adapters.link_state(&selection);
        self.adapters = adapters;
        self.adapter_selection = selection;
    }

    pub fn backend_status(&self) -> BackendStatus {
        self.backend.status()
    }

    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::backend::{BackendStatus, CaptureBackend};
    use linkroute_core::{
        ApplicationRule, DecisionSource, DomainMatchMode, DomainRule, FlowDescriptor, FlowKey,
        LinkState, Outlet, Protocol, RouteAction, RouteConfig, RouteDecision,
    };

    use super::ServiceController;

    #[test]
    fn dns_cache_domain_is_used_for_flow_decision() {
        let mut config = RouteConfig::enabled();
        config.domain_rules.push(
            DomainRule::new(
                "domain-api",
                "api.example.com",
                DomainMatchMode::Exact,
                Outlet::Ethernet,
            )
            .unwrap(),
        );
        let mut controller = ServiceController::new(config, Box::new(FakeBackend::default()));
        let remote = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
        controller.observe_dns_answer(100, "api.example.com", &[], &[remote], 60).unwrap();

        let decision = controller.decide_and_record_flow(110, sample_key(remote), 1000, None, None);

        assert_eq!(decision.action, RouteAction::UseOutlet(Outlet::Ethernet));
        assert_eq!(controller.flow_count(), 1);
    }

    #[test]
    fn swap_clears_managed_flows() {
        let mut config = RouteConfig::enabled();
        config.application_rules.push(
            ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", Outlet::Ethernet)
                .unwrap(),
        );
        let mut controller = ServiceController::new(config, Box::new(FakeBackend::default()));
        controller.start().unwrap();

        controller.decide_and_record_flow(
            100,
            sample_key(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20))),
            1000,
            Some(String::from(r"C:\Apps\browser.exe")),
            None,
        );

        assert_eq!(controller.swap_outlets_now().unwrap(), 1);
        assert_eq!(controller.flow_count(), 0);
    }

    fn sample_key(remote_addr: IpAddr) -> FlowKey {
        FlowKey {
            protocol: Protocol::Tcp,
            local_addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
            local_port: 50000,
            remote_addr,
            remote_port: 443,
        }
    }

    #[derive(Default)]
    struct FakeBackend {
        running: bool,
        active_flows: usize,
    }

    impl CaptureBackend for FakeBackend {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn start(&mut self, _config: &RouteConfig) -> Result<(), String> {
            self.running = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.active_flows = 0;
            Ok(())
        }

        fn status(&self) -> BackendStatus {
            BackendStatus {
                name: self.name(),
                running: self.running,
                active_flows: self.active_flows,
            }
        }

        fn close_managed_flows(&mut self) -> Result<usize, String> {
            let closed = self.active_flows;
            self.active_flows = 0;
            Ok(closed)
        }

        fn apply_decision(
            &mut self,
            _flow: &FlowDescriptor,
            decision: &RouteDecision,
            _links: LinkState,
        ) {
            if !matches!(decision.action, RouteAction::UseSystemDefault) {
                self.active_flows += 1;
            }
        }
    }

    #[test]
    fn fake_backend_starts_cleanly() {
        let mut backend = FakeBackend::default();
        assert_eq!(backend.name(), "fake");
        assert_eq!(backend.close_managed_flows(), Ok(0));
        assert_eq!(
            RouteDecision::system_default(DecisionSource::SystemDefault).action,
            RouteAction::UseSystemDefault
        );
    }
}
