#![allow(dead_code)]

use std::collections::HashMap;
use std::net::IpAddr;

use linkroute_core::{FlowKey, Outlet, Protocol, RouteDecision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatMapping {
    pub original_key: FlowKey,
    pub translated_key: FlowKey,
    pub outlet: Outlet,
    pub decision: RouteDecision,
    pub created_at_epoch_seconds: u64,
    pub last_seen_epoch_seconds: u64,
}

impl NatMapping {
    pub fn source_rewrite_port(&self) -> Option<u16> {
        transport_port(self.translated_key.protocol, self.translated_key.local_port)
    }

    pub fn destination_restore_port(&self) -> Option<u16> {
        transport_port(self.original_key.protocol, self.original_key.local_port)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NatTable {
    by_original: HashMap<FlowKey, NatMapping>,
    original_by_translated: HashMap<FlowKey, FlowKey>,
}

impl NatTable {
    pub fn upsert_outbound(
        &mut self,
        now_epoch_seconds: u64,
        original_key: FlowKey,
        source_addr: IpAddr,
        source_port: u16,
        outlet: Outlet,
        decision: RouteDecision,
    ) -> NatMapping {
        let translated_key = FlowKey {
            protocol: original_key.protocol,
            local_addr: source_addr,
            local_port: source_port,
            remote_addr: original_key.remote_addr,
            remote_port: original_key.remote_port,
        };

        if let Some(conflicting_original) =
            self.original_by_translated.get(&translated_key).cloned()
        {
            if conflicting_original != original_key {
                self.by_original.remove(&conflicting_original);
            }
        }

        if let Some(previous) = self.by_original.remove(&original_key) {
            self.original_by_translated.remove(&previous.translated_key);
        }

        let mapping = NatMapping {
            original_key: original_key.clone(),
            translated_key: translated_key.clone(),
            outlet,
            decision,
            created_at_epoch_seconds: now_epoch_seconds,
            last_seen_epoch_seconds: now_epoch_seconds,
        };
        self.by_original.insert(original_key.clone(), mapping.clone());
        self.original_by_translated.insert(translated_key, original_key);
        mapping
    }

    pub fn get_by_original(&self, original_key: &FlowKey) -> Option<&NatMapping> {
        self.by_original.get(original_key)
    }

    pub fn get_by_translated(&self, translated_key: &FlowKey) -> Option<&NatMapping> {
        self.original_by_translated
            .get(translated_key)
            .and_then(|original_key| self.by_original.get(original_key))
    }

    pub fn touch_original(&mut self, original_key: &FlowKey, now_epoch_seconds: u64) -> bool {
        if let Some(mapping) = self.by_original.get_mut(original_key) {
            mapping.last_seen_epoch_seconds = now_epoch_seconds;
            return true;
        }
        false
    }

    pub fn touch_translated(&mut self, translated_key: &FlowKey, now_epoch_seconds: u64) -> bool {
        let Some(original_key) = self.original_by_translated.get(translated_key).cloned() else {
            return false;
        };
        self.touch_original(&original_key, now_epoch_seconds)
    }

    pub fn clear(&mut self) -> usize {
        let cleared = self.by_original.len();
        self.by_original.clear();
        self.original_by_translated.clear();
        cleared
    }

    pub fn purge_idle(&mut self, now_epoch_seconds: u64, idle_seconds: u64) -> usize {
        let cutoff = now_epoch_seconds.saturating_sub(idle_seconds);
        let before = self.by_original.len();
        self.by_original.retain(|_, mapping| mapping.last_seen_epoch_seconds >= cutoff);
        self.rebuild_reverse_index();
        before - self.by_original.len()
    }

    pub fn len(&self) -> usize {
        self.by_original.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_original.is_empty()
    }

    fn rebuild_reverse_index(&mut self) {
        self.original_by_translated.clear();
        for (original_key, mapping) in &self.by_original {
            self.original_by_translated
                .insert(mapping.translated_key.clone(), original_key.clone());
        }
    }
}

pub fn transport_port(protocol: Protocol, port: u16) -> Option<u16> {
    match protocol {
        Protocol::Tcp | Protocol::Udp => Some(port),
        Protocol::Icmp | Protocol::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use linkroute_core::{DecisionSource, Outlet, Protocol, RouteDecision};

    use super::NatTable;

    #[test]
    fn outbound_mapping_can_be_found_by_translated_inbound_key() {
        let mut table = NatTable::default();
        let original = sample_key(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 50000);
        let mapping = table.upsert_outbound(
            100,
            original.clone(),
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)),
            50000,
            Outlet::Ethernet,
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Ethernet),
        );

        assert_eq!(table.get_by_original(&original), Some(&mapping));
        assert_eq!(table.get_by_translated(&mapping.translated_key), Some(&mapping));
    }

    #[test]
    fn replacing_original_removes_stale_reverse_index() {
        let mut table = NatTable::default();
        let original = sample_key(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 50000);
        let first = table.upsert_outbound(
            100,
            original.clone(),
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)),
            50000,
            Outlet::Ethernet,
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Ethernet),
        );
        let second = table.upsert_outbound(
            110,
            original,
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 55)),
            50000,
            Outlet::Wifi,
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Wifi),
        );

        assert!(table.get_by_translated(&first.translated_key).is_none());
        assert!(table.get_by_translated(&second.translated_key).is_some());
    }

    #[test]
    fn idle_mappings_are_purged_from_both_indexes() {
        let mut table = NatTable::default();
        let original = sample_key(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 50000);
        let mapping = table.upsert_outbound(
            100,
            original,
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)),
            50000,
            Outlet::Ethernet,
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Ethernet),
        );

        assert_eq!(table.purge_idle(200, 60), 1);
        assert!(table.get_by_translated(&mapping.translated_key).is_none());
        assert!(table.is_empty());
    }

    fn sample_key(local_addr: IpAddr, local_port: u16) -> linkroute_core::FlowKey {
        linkroute_core::FlowKey {
            protocol: Protocol::Tcp,
            local_addr,
            local_port,
            remote_addr: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)),
            remote_port: 443,
        }
    }
}
