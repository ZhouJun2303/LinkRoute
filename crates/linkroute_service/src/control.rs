#![allow(dead_code)]

use std::net::IpAddr;

use crate::backend::{BackendStatus, CaptureBackend};
use crate::nat::{transport_port, NatMapping, NatTable};
use linkroute_core::{
    parse_ip_packet, rewrite_packet_destination, rewrite_packet_source, AdapterInventory,
    AdapterSelection, AdapterSnapshot, DnsCache, FlowDescriptor, FlowKey, FlowRecord, FlowTable,
    LinkState, Outlet, PacketDirection, RouteAction, RouteConfig, RouteDecision, RouteEngine,
    ServiceSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketRouteOutcome {
    Forwarded { decision: Option<RouteDecision> },
    Dropped { decision: RouteDecision, reason: String },
    Rewritten { direction: PacketDirection, mapping: NatMapping },
}

pub struct ServiceController {
    engine: RouteEngine,
    backend: Box<dyn CaptureBackend>,
    links: LinkState,
    dns: DnsCache,
    flows: FlowTable,
    nat: NatTable,
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
            nat: NatTable::default(),
            adapters: AdapterInventory::default(),
            adapter_selection: AdapterSelection::default(),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        self.backend.start(self.engine.config())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.backend.stop()
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.engine.config_mut().enabled = false;
        self.flows.clear_managed();
        self.nat.clear();
        self.backend.stop()
    }

    pub fn swap_outlets_now(&mut self) -> Result<usize, String> {
        self.engine.config_mut().swap_outlets();
        let local_cleared = self.flows.clear_managed();
        let nat_cleared = self.nat.clear();
        let backend_cleared = self.backend.close_managed_flows()?;
        Ok(local_cleared.max(nat_cleared).max(backend_cleared))
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

    pub fn decide_packet(
        &mut self,
        now_epoch_seconds: u64,
        packet: &[u8],
        direction: PacketDirection,
        process_id: u32,
        exe_path: Option<String>,
        explicit_domain: Option<String>,
    ) -> Result<RouteDecision, String> {
        let parsed = parse_ip_packet(packet).map_err(|error| error.to_string())?;
        let key = parsed.flow_key(direction);
        Ok(self.decide_and_record_flow(
            now_epoch_seconds,
            key,
            process_id,
            exe_path,
            explicit_domain,
        ))
    }

    pub fn process_packet(
        &mut self,
        now_epoch_seconds: u64,
        packet: &mut [u8],
        direction: PacketDirection,
        process_id: u32,
        exe_path: Option<String>,
        explicit_domain: Option<String>,
    ) -> Result<PacketRouteOutcome, String> {
        match direction {
            PacketDirection::Outbound => self.process_outbound_packet(
                now_epoch_seconds,
                packet,
                process_id,
                exe_path,
                explicit_domain,
            ),
            PacketDirection::Inbound => self.process_inbound_packet(now_epoch_seconds, packet),
        }
    }

    fn process_outbound_packet(
        &mut self,
        now_epoch_seconds: u64,
        packet: &mut [u8],
        process_id: u32,
        exe_path: Option<String>,
        explicit_domain: Option<String>,
    ) -> Result<PacketRouteOutcome, String> {
        let parsed = parse_ip_packet(packet).map_err(|error| error.to_string())?;
        let key = parsed.flow_key(PacketDirection::Outbound);
        let decision = self.decide_and_record_flow(
            now_epoch_seconds,
            key.clone(),
            process_id,
            exe_path,
            explicit_domain,
        );

        match decision.action {
            RouteAction::UseSystemDefault => {
                Ok(PacketRouteOutcome::Forwarded { decision: Some(decision) })
            }
            RouteAction::Block => Ok(PacketRouteOutcome::Dropped {
                decision,
                reason: String::from("routing policy blocked packet"),
            }),
            RouteAction::UseOutlet(outlet) => {
                let Some(source_addr) = self.adapters.source_address_for(
                    outlet,
                    &self.adapter_selection,
                    parsed.destination_addr,
                ) else {
                    return Ok(PacketRouteOutcome::Dropped {
                        decision,
                        reason: format!(
                            "no usable {} source address for {}",
                            outlet.as_str(),
                            parsed.destination_addr
                        ),
                    });
                };

                let source_port = parsed.source_port.unwrap_or(0);
                rewrite_packet_source(
                    packet,
                    source_addr,
                    transport_port(parsed.protocol, source_port),
                )
                .map_err(|error| error.to_string())?;
                let mapping = self.nat.upsert_outbound(
                    now_epoch_seconds,
                    key,
                    source_addr,
                    source_port,
                    outlet,
                    decision,
                );
                Ok(PacketRouteOutcome::Rewritten { direction: PacketDirection::Outbound, mapping })
            }
        }
    }

    fn process_inbound_packet(
        &mut self,
        now_epoch_seconds: u64,
        packet: &mut [u8],
    ) -> Result<PacketRouteOutcome, String> {
        let parsed = parse_ip_packet(packet).map_err(|error| error.to_string())?;
        let translated_key = parsed.flow_key(PacketDirection::Inbound);
        let Some(mapping) = self.nat.get_by_translated(&translated_key).cloned() else {
            return Ok(PacketRouteOutcome::Forwarded { decision: None });
        };

        rewrite_packet_destination(
            packet,
            mapping.original_key.local_addr,
            mapping.destination_restore_port(),
        )
        .map_err(|error| error.to_string())?;
        self.nat.touch_translated(&translated_key, now_epoch_seconds);
        self.flows.touch(&mapping.original_key, now_epoch_seconds);
        let mapping = self.nat.get_by_original(&mapping.original_key).cloned().unwrap_or(mapping);
        Ok(PacketRouteOutcome::Rewritten { direction: PacketDirection::Inbound, mapping })
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

    pub fn snapshot(
        &self,
        service_version: impl Into<String>,
        windivert_ready: bool,
    ) -> ServiceSnapshot {
        let status = self.backend.status();
        ServiceSnapshot {
            protocol_version: linkroute_core::ipc::IPC_PROTOCOL_VERSION,
            service_version: service_version.into(),
            enabled: self.engine.config().enabled,
            backend_name: status.name.to_string(),
            backend_running: status.running,
            active_flows: self.flows.len(),
            link_state: self.links,
            adapters: self
                .adapters
                .adapters()
                .iter()
                .map(|adapter| AdapterSnapshot {
                    id: adapter.id.clone(),
                    name: adapter.name.clone(),
                    kind: adapter.kind,
                    outlet: if adapter.is_candidate_for(Outlet::Ethernet) {
                        Some(Outlet::Ethernet)
                    } else if adapter.is_candidate_for(Outlet::Wifi) {
                        Some(Outlet::Wifi)
                    } else {
                        None
                    },
                    interface_index: adapter.interface_index,
                    up: adapter.up,
                    physical: adapter.physical,
                })
                .collect(),
            windivert_ready,
        }
    }

    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }

    pub fn nat_count(&self) -> usize {
        self.nat.len()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use crate::backend::{BackendStatus, CaptureBackend};
    use linkroute_core::{
        parse_ip_packet, AdapterInventory, AdapterKind, AdapterSelection, ApplicationRule,
        DecisionSource, DomainMatchMode, DomainRule, FlowDescriptor, FlowKey, LinkState,
        NetworkAdapter, Outlet, PacketDirection, Protocol, RouteAction, RouteConfig, RouteDecision,
    };

    use super::{PacketRouteOutcome, ServiceController};

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

    #[test]
    fn raw_packet_decision_records_flow() {
        let mut config = RouteConfig::enabled();
        config.application_rules.push(
            ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", Outlet::Ethernet)
                .unwrap(),
        );
        let mut controller = ServiceController::new(config, Box::new(FakeBackend::default()));
        controller.start().unwrap();
        let packet = sample_ipv4_tcp_packet();

        let decision = controller
            .decide_packet(
                100,
                &packet,
                PacketDirection::Outbound,
                1000,
                Some(String::from(r"C:\Apps\browser.exe")),
                None,
            )
            .unwrap();

        assert_eq!(decision.action, RouteAction::UseOutlet(Outlet::Ethernet));
        assert_eq!(controller.flow_count(), 1);
        assert_eq!(controller.backend_status().active_flows, 1);
    }

    #[test]
    fn outbound_packet_to_managed_outlet_rewrites_source_and_records_nat() {
        let mut controller = controller_with_app_rule(Outlet::Ethernet);
        let (adapters, selection) =
            sample_adapters(vec![IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9))]);
        controller.set_adapters(adapters, selection);
        controller.start().unwrap();
        let mut packet = sample_ipv4_tcp_packet();

        let outcome = controller
            .process_packet(
                100,
                &mut packet,
                PacketDirection::Outbound,
                1000,
                Some(String::from(r"C:\Apps\browser.exe")),
                None,
            )
            .unwrap();

        let PacketRouteOutcome::Rewritten { direction, mapping } = outcome else {
            panic!("expected rewritten packet");
        };
        assert_eq!(direction, PacketDirection::Outbound);
        assert_eq!(mapping.outlet, Outlet::Ethernet);
        assert_eq!(mapping.original_key.local_addr, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
        assert_eq!(mapping.translated_key.local_addr, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)));
        assert_eq!(controller.flow_count(), 1);
        assert_eq!(controller.nat_count(), 1);

        let parsed = parse_ip_packet(&packet).unwrap();
        assert_eq!(parsed.source_addr, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)));
        assert_eq!(parsed.source_port, Some(50000));
    }

    #[test]
    fn inbound_packet_for_nat_mapping_restores_original_destination() {
        let mut controller = controller_with_app_rule(Outlet::Ethernet);
        let (adapters, selection) =
            sample_adapters(vec![IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9))]);
        controller.set_adapters(adapters, selection);
        controller.start().unwrap();
        let mut outbound = sample_ipv4_tcp_packet();
        controller
            .process_packet(
                100,
                &mut outbound,
                PacketDirection::Outbound,
                1000,
                Some(String::from(r"C:\Apps\browser.exe")),
                None,
            )
            .unwrap();

        let mut inbound = ipv4_tcp_packet(
            Ipv4Addr::new(203, 0, 113, 20),
            443,
            Ipv4Addr::new(198, 51, 100, 9),
            50000,
        );
        let outcome = controller
            .process_packet(110, &mut inbound, PacketDirection::Inbound, 0, None, None)
            .unwrap();

        let PacketRouteOutcome::Rewritten { direction, mapping } = outcome else {
            panic!("expected inbound rewrite");
        };
        assert_eq!(direction, PacketDirection::Inbound);
        assert_eq!(mapping.last_seen_epoch_seconds, 110);
        let parsed = parse_ip_packet(&inbound).unwrap();
        assert_eq!(parsed.destination_addr, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
        assert_eq!(parsed.destination_port, Some(50000));
    }

    #[test]
    fn managed_outbound_packet_drops_when_target_source_address_is_missing() {
        let mut controller = controller_with_app_rule(Outlet::Ethernet);
        let (adapters, selection) = sample_adapters(vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]);
        controller.set_adapters(adapters, selection);
        controller.start().unwrap();
        let mut packet = sample_ipv4_tcp_packet();

        let outcome = controller
            .process_packet(
                100,
                &mut packet,
                PacketDirection::Outbound,
                1000,
                Some(String::from(r"C:\Apps\browser.exe")),
                None,
            )
            .unwrap();

        let PacketRouteOutcome::Dropped { decision, reason } = outcome else {
            panic!("expected drop without source address");
        };
        assert_eq!(decision.action, RouteAction::UseOutlet(Outlet::Ethernet));
        assert!(reason.contains("no usable ethernet source address"));
        assert_eq!(controller.nat_count(), 0);
    }

    fn controller_with_app_rule(outlet: Outlet) -> ServiceController {
        let mut config = RouteConfig::enabled();
        config
            .application_rules
            .push(ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", outlet).unwrap());
        ServiceController::new(config, Box::new(FakeBackend::default()))
    }

    fn sample_adapters(ethernet_addresses: Vec<IpAddr>) -> (AdapterInventory, AdapterSelection) {
        let inventory = AdapterInventory::new(vec![
            NetworkAdapter {
                id: String::from("eth"),
                name: String::from("Ethernet"),
                kind: AdapterKind::Ethernet,
                interface_index: 10,
                up: true,
                physical: true,
                addresses: ethernet_addresses,
            },
            NetworkAdapter {
                id: String::from("wifi"),
                name: String::from("Wi-Fi"),
                kind: AdapterKind::Wifi,
                interface_index: 30,
                up: true,
                physical: true,
                addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 55))],
            },
        ]);
        let selection = AdapterSelection {
            ethernet_id: Some(String::from("eth")),
            wifi_id: Some(String::from("wifi")),
        };
        (inventory, selection)
    }

    fn sample_ipv4_tcp_packet() -> Vec<u8> {
        ipv4_tcp_packet(Ipv4Addr::new(192, 0, 2, 10), 50000, Ipv4Addr::new(203, 0, 113, 20), 443)
    }

    fn ipv4_tcp_packet(
        source_addr: Ipv4Addr,
        source_port: u16,
        destination_addr: Ipv4Addr,
        destination_port: u16,
    ) -> Vec<u8> {
        let mut packet = vec![0_u8; 40];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&40_u16.to_be_bytes());
        packet[8] = 64;
        packet[9] = 6;
        packet[12..16].copy_from_slice(&source_addr.octets());
        packet[16..20].copy_from_slice(&destination_addr.octets());
        packet[20..22].copy_from_slice(&source_port.to_be_bytes());
        packet[22..24].copy_from_slice(&destination_port.to_be_bytes());
        packet
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
