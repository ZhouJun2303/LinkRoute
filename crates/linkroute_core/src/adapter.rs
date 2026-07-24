use std::net::IpAddr;

use crate::model::{LinkState, Outlet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    Ethernet,
    Wifi,
    Tunnel,
    Loopback,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAdapter {
    pub id: String,
    pub name: String,
    pub kind: AdapterKind,
    pub interface_index: u32,
    pub up: bool,
    pub physical: bool,
    pub addresses: Vec<IpAddr>,
}

impl NetworkAdapter {
    pub fn is_candidate_for(&self, outlet: Outlet) -> bool {
        self.up
            && self.physical
            && matches!(
                (outlet, self.kind),
                (Outlet::Ethernet, AdapterKind::Ethernet) | (Outlet::Wifi, AdapterKind::Wifi)
            )
    }

    pub fn looks_like_vpn_or_tunnel(&self) -> bool {
        if self.up && matches!(self.kind, AdapterKind::Tunnel) {
            return true;
        }

        let name = self.name.to_ascii_lowercase();
        self.up
            && ["vpn", "wireguard", "openvpn", "tailscale", "zerotier", "tap", "tun", "tunnel"]
                .iter()
                .any(|needle| name.contains(needle))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterSelection {
    pub ethernet_id: Option<String>,
    pub wifi_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AdapterInventory {
    adapters: Vec<NetworkAdapter>,
}

impl AdapterInventory {
    pub fn new(adapters: Vec<NetworkAdapter>) -> Self {
        Self { adapters }
    }

    pub fn adapters(&self) -> &[NetworkAdapter] {
        &self.adapters
    }

    pub fn best_for(&self, outlet: Outlet) -> Option<&NetworkAdapter> {
        self.adapters
            .iter()
            .filter(|adapter| adapter.is_candidate_for(outlet))
            .min_by_key(|adapter| adapter.interface_index)
    }

    pub fn vpn_active(&self) -> bool {
        self.adapters.iter().any(NetworkAdapter::looks_like_vpn_or_tunnel)
    }

    pub fn link_state(&self, selection: &AdapterSelection) -> LinkState {
        LinkState {
            ethernet_up: selection
                .ethernet_id
                .as_deref()
                .and_then(|id| self.find(id))
                .map(|adapter| adapter.up)
                .unwrap_or_else(|| self.best_for(Outlet::Ethernet).is_some()),
            wifi_up: selection
                .wifi_id
                .as_deref()
                .and_then(|id| self.find(id))
                .map(|adapter| adapter.up)
                .unwrap_or_else(|| self.best_for(Outlet::Wifi).is_some()),
            vpn_active: self.vpn_active(),
        }
    }

    pub fn find(&self, id: &str) -> Option<&NetworkAdapter> {
        self.adapters.iter().find(|adapter| adapter.id == id)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::model::Outlet;

    use super::{AdapterInventory, AdapterKind, AdapterSelection, NetworkAdapter};

    #[test]
    fn chooses_lowest_index_physical_adapter() {
        let inventory = AdapterInventory::new(vec![
            adapter("eth-high", "Ethernet 2", AdapterKind::Ethernet, 20, true, true),
            adapter("eth-low", "Ethernet", AdapterKind::Ethernet, 10, true, true),
        ]);

        assert_eq!(inventory.best_for(Outlet::Ethernet).unwrap().id, "eth-low");
    }

    #[test]
    fn ignores_virtual_adapter_for_outlet_choice() {
        let inventory = AdapterInventory::new(vec![
            adapter("vpn", "VPN Ethernet", AdapterKind::Ethernet, 1, true, false),
            adapter("eth", "Ethernet", AdapterKind::Ethernet, 2, true, true),
        ]);

        assert_eq!(inventory.best_for(Outlet::Ethernet).unwrap().id, "eth");
    }

    #[test]
    fn detects_vpn_by_kind_or_name() {
        let inventory = AdapterInventory::new(vec![adapter(
            "wg",
            "WireGuard Tunnel",
            AdapterKind::Tunnel,
            50,
            true,
            false,
        )]);

        assert!(inventory.vpn_active());
        assert!(inventory.link_state(&AdapterSelection::default()).vpn_active);
    }

    fn adapter(
        id: &str,
        name: &str,
        kind: AdapterKind,
        interface_index: u32,
        up: bool,
        physical: bool,
    ) -> NetworkAdapter {
        NetworkAdapter {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            interface_index,
            up,
            physical,
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, interface_index as u8))],
        }
    }
}
