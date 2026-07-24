use linkroute_core::{AdapterInventory, AdapterSelection};

pub fn enumerate_adapters() -> Result<(AdapterInventory, AdapterSelection), String> {
    platform::enumerate_adapters()
}

#[cfg(windows)]
mod platform {
    use std::ffi::{c_char, c_void, CStr};
    use std::mem;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::ptr;

    use linkroute_core::{AdapterInventory, AdapterKind, AdapterSelection, NetworkAdapter, Outlet};

    const AF_UNSPEC: u32 = 0;
    const AF_INET: u16 = 2;
    const AF_INET6: u16 = 23;
    const ERROR_BUFFER_OVERFLOW: u32 = 111;
    const NO_ERROR: u32 = 0;
    const GAA_FLAG_SKIP_ANYCAST: u32 = 0x0002;
    const GAA_FLAG_SKIP_MULTICAST: u32 = 0x0004;
    const GAA_FLAG_SKIP_DNS_SERVER: u32 = 0x0008;
    const GET_ADAPTERS_FLAGS: u32 =
        GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;

    const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
    const IF_TYPE_PPP: u32 = 23;
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    const IF_TYPE_IEEE80211: u32 = 71;
    const IF_TYPE_TUNNEL: u32 = 131;
    const IF_OPER_STATUS_UP: u32 = 1;
    const MAX_ADAPTER_ADDRESS_LENGTH: usize = 8;

    #[repr(C)]
    struct IpAdapterAddresses {
        length: u32,
        if_index: u32,
        next: *mut IpAdapterAddresses,
        adapter_name: *mut c_char,
        first_unicast_address: *mut IpAdapterUnicastAddress,
        first_anycast_address: *mut c_void,
        first_multicast_address: *mut c_void,
        first_dns_server_address: *mut c_void,
        dns_suffix: *mut u16,
        description: *mut u16,
        friendly_name: *mut u16,
        physical_address: [u8; MAX_ADAPTER_ADDRESS_LENGTH],
        physical_address_length: u32,
        flags: u32,
        mtu: u32,
        if_type: u32,
        oper_status: u32,
        ipv6_if_index: u32,
        zone_indices: [u32; 16],
        first_prefix: *mut c_void,
    }

    #[repr(C)]
    struct IpAdapterUnicastAddress {
        length: u32,
        flags: u32,
        next: *mut IpAdapterUnicastAddress,
        address: SocketAddress,
    }

    #[repr(C)]
    struct SocketAddress {
        lp_sockaddr: *const Sockaddr,
        i_sockaddr_length: i32,
    }

    #[repr(C)]
    struct Sockaddr {
        sa_family: u16,
        sa_data: [u8; 14],
    }

    #[repr(C)]
    struct SockaddrIn {
        sin_family: u16,
        sin_port: u16,
        sin_addr: InAddr,
        sin_zero: [u8; 8],
    }

    #[repr(C)]
    struct InAddr {
        s_addr: u32,
    }

    #[repr(C)]
    struct SockaddrIn6 {
        sin6_family: u16,
        sin6_port: u16,
        sin6_flowinfo: u32,
        sin6_addr: In6Addr,
        sin6_scope_id: u32,
    }

    #[repr(C)]
    struct In6Addr {
        bytes: [u8; 16],
    }

    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetAdaptersAddresses(
            family: u32,
            flags: u32,
            reserved: *mut c_void,
            adapter_addresses: *mut IpAdapterAddresses,
            size_pointer: *mut u32,
        ) -> u32;
    }

    pub fn enumerate_adapters() -> Result<(AdapterInventory, AdapterSelection), String> {
        let mut buffer_len = 15_000_u32;
        for _ in 0..4 {
            let mut buffer = vec![0_u8; buffer_len as usize];
            let result = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC,
                    GET_ADAPTERS_FLAGS,
                    ptr::null_mut(),
                    buffer.as_mut_ptr().cast::<IpAdapterAddresses>(),
                    &mut buffer_len,
                )
            };

            match result {
                NO_ERROR => return parse_buffer(buffer.as_mut_ptr().cast::<IpAdapterAddresses>()),
                ERROR_BUFFER_OVERFLOW => continue,
                error => {
                    return Err(format!(
                        "GetAdaptersAddresses failed: {}",
                        std::io::Error::from_raw_os_error(error as i32)
                    ));
                }
            }
        }

        Err(String::from("GetAdaptersAddresses buffer kept growing"))
    }

    fn parse_buffer(
        head: *mut IpAdapterAddresses,
    ) -> Result<(AdapterInventory, AdapterSelection), String> {
        let mut adapters = Vec::new();
        let mut current = head;

        while !current.is_null() {
            let adapter = unsafe { &*current };
            let id = c_string(adapter.adapter_name)
                .unwrap_or_else(|| format!("if-{}", adapter.if_index));
            let friendly_name = utf16_string(adapter.friendly_name);
            let description = utf16_string(adapter.description);
            let name = if friendly_name.is_empty() { id.clone() } else { friendly_name };
            let kind = adapter_kind(adapter.if_type, &name, &description);
            let up = adapter.oper_status == IF_OPER_STATUS_UP;
            let physical = adapter.physical_address_length > 0
                && matches!(kind, AdapterKind::Ethernet | AdapterKind::Wifi)
                && !looks_virtual_adapter(&name, &description);
            let addresses = unicast_addresses(adapter.first_unicast_address);

            adapters.push(NetworkAdapter {
                id,
                name,
                kind,
                interface_index: adapter.if_index,
                up,
                physical,
                addresses,
            });

            current = adapter.next;
        }

        let inventory = AdapterInventory::new(adapters);
        let selection = AdapterSelection {
            ethernet_id: inventory.best_for(Outlet::Ethernet).map(|adapter| adapter.id.clone()),
            wifi_id: inventory.best_for(Outlet::Wifi).map(|adapter| adapter.id.clone()),
        };
        Ok((inventory, selection))
    }

    fn unicast_addresses(mut current: *mut IpAdapterUnicastAddress) -> Vec<IpAddr> {
        let mut addresses = Vec::new();
        while !current.is_null() {
            let unicast = unsafe { &*current };
            if let Some(address) = socket_address_to_ip(&unicast.address) {
                addresses.push(address);
            }
            current = unicast.next;
        }
        addresses
    }

    fn socket_address_to_ip(address: &SocketAddress) -> Option<IpAddr> {
        if address.lp_sockaddr.is_null() || address.i_sockaddr_length <= 0 {
            return None;
        }

        let family = unsafe { (*address.lp_sockaddr).sa_family };
        match family {
            AF_INET if address.i_sockaddr_length as usize >= mem::size_of::<SockaddrIn>() => {
                let sockaddr = unsafe { &*(address.lp_sockaddr.cast::<SockaddrIn>()) };
                let octets = sockaddr.sin_addr.s_addr.to_ne_bytes();
                Some(IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])))
            }
            AF_INET6 if address.i_sockaddr_length as usize >= mem::size_of::<SockaddrIn6>() => {
                let sockaddr = unsafe { &*(address.lp_sockaddr.cast::<SockaddrIn6>()) };
                Some(IpAddr::V6(Ipv6Addr::from(sockaddr.sin6_addr.bytes)))
            }
            _ => None,
        }
    }

    fn adapter_kind(if_type: u32, name: &str, description: &str) -> AdapterKind {
        match if_type {
            IF_TYPE_ETHERNET_CSMACD => AdapterKind::Ethernet,
            IF_TYPE_IEEE80211 => AdapterKind::Wifi,
            IF_TYPE_SOFTWARE_LOOPBACK => AdapterKind::Loopback,
            IF_TYPE_TUNNEL | IF_TYPE_PPP => AdapterKind::Tunnel,
            _ => adapter_kind_from_text(name, description),
        }
    }

    fn adapter_kind_from_text(name: &str, description: &str) -> AdapterKind {
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        if ["vpn", "wireguard", "openvpn", "tailscale", "zerotier", "tap", "tun", "tunnel"]
            .iter()
            .any(|needle| haystack.contains(needle))
        {
            AdapterKind::Tunnel
        } else if looks_virtual_adapter(name, description) {
            AdapterKind::Other
        } else if haystack.contains("wi-fi")
            || haystack.contains("wifi")
            || haystack.contains("wireless")
        {
            AdapterKind::Wifi
        } else if haystack.contains("ethernet") || haystack.contains("lan") {
            AdapterKind::Ethernet
        } else if haystack.contains("loopback") {
            AdapterKind::Loopback
        } else {
            AdapterKind::Other
        }
    }

    fn looks_virtual_adapter(name: &str, description: &str) -> bool {
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        [
            "vethernet",
            "hyper-v",
            "wsl",
            "virtual",
            "vmware",
            "virtualbox",
            "docker",
            "npcap",
            "loopback",
        ]
        .iter()
        .any(|needle| haystack.contains(needle))
    }

    fn c_string(ptr: *const c_char) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        let value = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    fn utf16_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }

        let mut len = 0_usize;
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    #[cfg(test)]
    mod tests {
        use linkroute_core::AdapterKind;

        use super::{
            adapter_kind, adapter_kind_from_text, looks_virtual_adapter, IF_TYPE_ETHERNET_CSMACD,
            IF_TYPE_IEEE80211,
        };

        #[test]
        fn maps_windows_interface_types_to_adapter_kinds() {
            assert_eq!(adapter_kind(IF_TYPE_ETHERNET_CSMACD, "", ""), AdapterKind::Ethernet);
            assert_eq!(adapter_kind(IF_TYPE_IEEE80211, "", ""), AdapterKind::Wifi);
        }

        #[test]
        fn text_fallback_classifies_common_adapter_names() {
            assert_eq!(adapter_kind_from_text("Wi-Fi", ""), AdapterKind::Wifi);
            assert_eq!(adapter_kind_from_text("Ethernet 2", ""), AdapterKind::Ethernet);
            assert_eq!(adapter_kind_from_text("WireGuard Tunnel", ""), AdapterKind::Tunnel);
            assert_eq!(adapter_kind_from_text("vEthernet (WSL)", ""), AdapterKind::Other);
        }

        #[test]
        fn virtual_adapter_names_are_not_physical_candidates() {
            assert!(looks_virtual_adapter("vEthernet (WSL (Hyper-V firewall))", ""));
            assert!(looks_virtual_adapter("DockerNAT", ""));
            assert!(!looks_virtual_adapter("Ethernet", "Intel Controller"));
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use linkroute_core::{AdapterInventory, AdapterSelection};

    pub fn enumerate_adapters() -> Result<(AdapterInventory, AdapterSelection), String> {
        Ok((AdapterInventory::default(), AdapterSelection::default()))
    }
}
