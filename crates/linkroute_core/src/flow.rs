use std::collections::HashMap;
use std::net::IpAddr;

use crate::model::{Protocol, RouteAction, RouteDecision};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub protocol: Protocol,
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: IpAddr,
    pub remote_port: u16,
}

impl FlowKey {
    pub fn reverse(&self) -> Self {
        Self {
            protocol: self.protocol,
            local_addr: self.remote_addr,
            local_port: self.remote_port,
            remote_addr: self.local_addr,
            remote_port: self.local_port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowRecord {
    pub key: FlowKey,
    pub process_id: u32,
    pub exe_path: Option<String>,
    pub domain: Option<String>,
    pub decision: RouteDecision,
    pub created_at_epoch_seconds: u64,
    pub last_seen_epoch_seconds: u64,
}

impl FlowRecord {
    pub fn is_managed(&self) -> bool {
        !matches!(self.decision.action, RouteAction::UseSystemDefault)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlowTable {
    records: HashMap<FlowKey, FlowRecord>,
}

impl FlowTable {
    pub fn insert(&mut self, record: FlowRecord) {
        self.records.insert(record.key.clone(), record);
    }

    pub fn get(&self, key: &FlowKey) -> Option<&FlowRecord> {
        self.records.get(key)
    }

    pub fn get_bidirectional(&self, key: &FlowKey) -> Option<&FlowRecord> {
        self.get(key).or_else(|| self.get(&key.reverse()))
    }

    pub fn touch(&mut self, key: &FlowKey, now_epoch_seconds: u64) -> bool {
        if let Some(record) = self.records.get_mut(key) {
            record.last_seen_epoch_seconds = now_epoch_seconds;
            return true;
        }
        false
    }

    pub fn clear_managed(&mut self) -> usize {
        let before = self.records.len();
        self.records.retain(|_, record| !record.is_managed());
        before - self.records.len()
    }

    pub fn purge_idle(&mut self, now_epoch_seconds: u64, idle_seconds: u64) -> usize {
        let cutoff = now_epoch_seconds.saturating_sub(idle_seconds);
        let before = self.records.len();
        self.records.retain(|_, record| record.last_seen_epoch_seconds >= cutoff);
        before - self.records.len()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::model::{DecisionSource, Outlet, Protocol, RouteDecision};

    use super::{FlowKey, FlowRecord, FlowTable};

    #[test]
    fn reverse_lookup_finds_existing_flow() {
        let mut table = FlowTable::default();
        let key = sample_key();
        table.insert(sample_record(
            key.clone(),
            RouteDecision::system_default(DecisionSource::SystemDefault),
        ));

        assert!(table.get_bidirectional(&key.reverse()).is_some());
    }

    #[test]
    fn clear_managed_keeps_system_default_flows() {
        let mut table = FlowTable::default();
        let key = sample_key();
        table.insert(sample_record(
            key.clone(),
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Ethernet),
        ));
        table.insert(sample_record(
            FlowKey { local_port: 50001, ..key },
            RouteDecision::system_default(DecisionSource::SystemDefault),
        ));

        assert_eq!(table.clear_managed(), 1);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn purges_idle_flows() {
        let mut table = FlowTable::default();
        table.insert(sample_record(
            sample_key(),
            RouteDecision::outlet(DecisionSource::ApplicationRule, "app", Outlet::Wifi),
        ));

        assert_eq!(table.purge_idle(200, 60), 1);
        assert!(table.is_empty());
    }

    fn sample_key() -> FlowKey {
        FlowKey {
            protocol: Protocol::Tcp,
            local_addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
            local_port: 50000,
            remote_addr: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)),
            remote_port: 443,
        }
    }

    fn sample_record(key: FlowKey, decision: RouteDecision) -> FlowRecord {
        FlowRecord {
            key,
            process_id: 42,
            exe_path: Some(String::from(r"C:\Apps\browser.exe")),
            domain: Some(String::from("example.com")),
            decision,
            created_at_epoch_seconds: 100,
            last_seen_epoch_seconds: 100,
        }
    }
}
