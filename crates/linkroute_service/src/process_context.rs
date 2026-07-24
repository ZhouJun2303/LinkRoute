#![allow(dead_code)]

use std::collections::HashMap;

use linkroute_core::{parse_ip_packet, FlowKey, PacketDirection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketProcessContext {
    pub process_id: u32,
    pub exe_path: Option<String>,
    pub explicit_domain: Option<String>,
}

impl PacketProcessContext {
    pub fn unknown() -> Self {
        Self { process_id: 0, exe_path: None, explicit_domain: None }
    }

    pub fn new(process_id: u32, exe_path: Option<String>, explicit_domain: Option<String>) -> Self {
        Self { process_id, exe_path, explicit_domain }
    }
}

pub trait PacketProcessResolver {
    fn resolve_packet(&mut self, packet: &[u8], direction: PacketDirection)
        -> PacketProcessContext;
}

#[derive(Debug, Default)]
pub struct NoopProcessResolver;

impl PacketProcessResolver for NoopProcessResolver {
    fn resolve_packet(
        &mut self,
        _packet: &[u8],
        _direction: PacketDirection,
    ) -> PacketProcessContext {
        PacketProcessContext::unknown()
    }
}

#[derive(Debug, Default)]
pub struct FlowContextCache {
    contexts: HashMap<FlowKey, PacketProcessContext>,
}

impl FlowContextCache {
    pub fn insert(&mut self, key: FlowKey, context: PacketProcessContext) {
        self.contexts.insert(key, context);
    }

    pub fn remove(&mut self, key: &FlowKey) -> Option<PacketProcessContext> {
        self.contexts.remove(key)
    }

    pub fn clear(&mut self) {
        self.contexts.clear();
    }

    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    pub fn get_bidirectional(&self, key: &FlowKey) -> Option<&PacketProcessContext> {
        self.contexts.get(key).or_else(|| self.contexts.get(&key.reverse()))
    }
}

impl PacketProcessResolver for FlowContextCache {
    fn resolve_packet(
        &mut self,
        packet: &[u8],
        direction: PacketDirection,
    ) -> PacketProcessContext {
        let Ok(parsed) = parse_ip_packet(packet) else {
            return PacketProcessContext::unknown();
        };
        let key = parsed.flow_key(direction);
        self.get_bidirectional(&key).cloned().unwrap_or_else(PacketProcessContext::unknown)
    }
}

#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsProcessResolver;

#[cfg(windows)]
impl PacketProcessResolver for WindowsProcessResolver {
    fn resolve_packet(
        &mut self,
        packet: &[u8],
        direction: PacketDirection,
    ) -> PacketProcessContext {
        let Ok(parsed) = parse_ip_packet(packet) else {
            return PacketProcessContext::unknown();
        };
        let key = parsed.flow_key(direction);
        let Some(process_id) = windows_process::owner_pid_for_flow(&key) else {
            return PacketProcessContext::unknown();
        };
        let exe_path = windows_process::process_image_path(process_id).ok();
        PacketProcessContext::new(process_id, exe_path, None)
    }
}

#[cfg(windows)]
mod windows_process {
    use std::ffi::c_void;
    use std::mem;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::ptr;

    use linkroute_core::{FlowKey, Protocol};

    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const ERROR_BUFFER_OVERFLOW: u32 = 111;
    const NO_ERROR: u32 = 0;
    const TCP_TABLE_OWNER_PID_ALL: u32 = 5;
    const UDP_TABLE_OWNER_PID: u32 = 1;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    type Handle = isize;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MibTcpRowOwnerPid {
        dw_state: u32,
        dw_local_addr: u32,
        dw_local_port: u32,
        dw_remote_addr: u32,
        dw_remote_port: u32,
        dw_owning_pid: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MibUdpRowOwnerPid {
        dw_local_addr: u32,
        dw_local_port: u32,
        dw_owning_pid: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MibTcp6RowOwnerPid {
        uc_local_addr: [u8; 16],
        dw_local_scope_id: u32,
        dw_local_port: u32,
        uc_remote_addr: [u8; 16],
        dw_remote_scope_id: u32,
        dw_remote_port: u32,
        dw_state: u32,
        dw_owning_pid: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MibUdp6RowOwnerPid {
        uc_local_addr: [u8; 16],
        dw_local_scope_id: u32,
        dw_local_port: u32,
        dw_owning_pid: u32,
    }

    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        fn GetExtendedTcpTable(
            tcp_table: *mut c_void,
            size_pointer: *mut u32,
            order: i32,
            ip_version: u32,
            table_class: u32,
            reserved: u32,
        ) -> u32;

        fn GetExtendedUdpTable(
            udp_table: *mut c_void,
            size_pointer: *mut u32,
            order: i32,
            ip_version: u32,
            table_class: u32,
            reserved: u32,
        ) -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            exe_name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(object: Handle) -> i32;
    }

    pub(super) fn owner_pid_for_flow(key: &FlowKey) -> Option<u32> {
        match (key.protocol, key.local_addr, key.remote_addr) {
            (Protocol::Tcp, IpAddr::V4(_), IpAddr::V4(_)) => owner_pid_from_tcp4(key),
            (Protocol::Tcp, IpAddr::V6(_), IpAddr::V6(_)) => owner_pid_from_tcp6(key),
            (Protocol::Udp, IpAddr::V4(_), _) => owner_pid_from_udp4(key),
            (Protocol::Udp, IpAddr::V6(_), _) => owner_pid_from_udp6(key),
            _ => None,
        }
    }

    pub(super) fn process_image_path(process_id: u32) -> Result<String, String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle == 0 {
            return Err(format!(
                "OpenProcess({process_id}) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut buffer = vec![0_u16; 32768];
        let mut size = buffer.len() as u32;
        let result =
            unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
        unsafe {
            CloseHandle(handle);
        }

        if result == 0 {
            return Err(format!(
                "QueryFullProcessImageNameW({process_id}) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(String::from_utf16_lossy(&buffer[..size as usize]))
    }

    fn owner_pid_from_tcp4(key: &FlowKey) -> Option<u32> {
        tcp4_rows().ok()?.into_iter().find_map(|row| {
            let row_key = tcp4_flow_key(row)?;
            (row_key == *key).then_some(row.dw_owning_pid)
        })
    }

    fn owner_pid_from_tcp6(key: &FlowKey) -> Option<u32> {
        tcp6_rows().ok()?.into_iter().find_map(|row| {
            let row_key = tcp6_flow_key(row)?;
            (row_key == *key).then_some(row.dw_owning_pid)
        })
    }

    fn owner_pid_from_udp4(key: &FlowKey) -> Option<u32> {
        udp4_rows()
            .ok()?
            .into_iter()
            .find_map(|row| udp4_row_matches(row, key).then_some(row.dw_owning_pid))
    }

    fn owner_pid_from_udp6(key: &FlowKey) -> Option<u32> {
        udp6_rows()
            .ok()?
            .into_iter()
            .find_map(|row| udp6_row_matches(row, key).then_some(row.dw_owning_pid))
    }

    fn tcp4_rows() -> Result<Vec<MibTcpRowOwnerPid>, String> {
        read_owner_table(|buffer, size| unsafe {
            GetExtendedTcpTable(buffer, size, 0, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0)
        })
    }

    fn tcp6_rows() -> Result<Vec<MibTcp6RowOwnerPid>, String> {
        read_owner_table(|buffer, size| unsafe {
            GetExtendedTcpTable(buffer, size, 0, AF_INET6, TCP_TABLE_OWNER_PID_ALL, 0)
        })
    }

    fn udp4_rows() -> Result<Vec<MibUdpRowOwnerPid>, String> {
        read_owner_table(|buffer, size| unsafe {
            GetExtendedUdpTable(buffer, size, 0, AF_INET, UDP_TABLE_OWNER_PID, 0)
        })
    }

    fn udp6_rows() -> Result<Vec<MibUdp6RowOwnerPid>, String> {
        read_owner_table(|buffer, size| unsafe {
            GetExtendedUdpTable(buffer, size, 0, AF_INET6, UDP_TABLE_OWNER_PID, 0)
        })
    }

    fn read_owner_table<T>(
        mut query: impl FnMut(*mut c_void, *mut u32) -> u32,
    ) -> Result<Vec<T>, String>
    where
        T: Copy,
    {
        let mut size = 0_u32;
        let first = query(ptr::null_mut(), &mut size);
        if !matches!(first, ERROR_INSUFFICIENT_BUFFER | ERROR_BUFFER_OVERFLOW) && first != NO_ERROR
        {
            return Err(format!(
                "owner table size query failed: {}",
                std::io::Error::from_raw_os_error(first as i32)
            ));
        }
        if size < mem::size_of::<u32>() as u32 {
            size = mem::size_of::<u32>() as u32;
        }

        let mut buffer = vec![0_u8; size as usize];
        let result = query(buffer.as_mut_ptr().cast(), &mut size);
        if result != NO_ERROR {
            return Err(format!(
                "owner table query failed: {}",
                std::io::Error::from_raw_os_error(result as i32)
            ));
        }

        table_rows::<T>(&buffer)
    }

    fn table_rows<T>(buffer: &[u8]) -> Result<Vec<T>, String>
    where
        T: Copy,
    {
        if buffer.len() < mem::size_of::<u32>() {
            return Err(String::from("owner table is too short"));
        }
        let count =
            u32::from_ne_bytes(buffer[..4].try_into().expect("slice length checked")) as usize;
        let row_size = mem::size_of::<T>();
        let needed = mem::size_of::<u32>() + count.saturating_mul(row_size);
        if buffer.len() < needed {
            return Err(format!("owner table is too short for {count} rows"));
        }

        let mut rows = Vec::with_capacity(count);
        let mut offset = mem::size_of::<u32>();
        for _ in 0..count {
            let row = unsafe { ptr::read_unaligned(buffer.as_ptr().add(offset).cast::<T>()) };
            rows.push(row);
            offset += row_size;
        }
        Ok(rows)
    }

    fn tcp4_flow_key(row: MibTcpRowOwnerPid) -> Option<FlowKey> {
        Some(FlowKey {
            protocol: Protocol::Tcp,
            local_addr: IpAddr::V4(ipv4_from_dword(row.dw_local_addr)),
            local_port: port_from_dword(row.dw_local_port),
            remote_addr: IpAddr::V4(ipv4_from_dword(row.dw_remote_addr)),
            remote_port: port_from_dword(row.dw_remote_port),
        })
    }

    fn tcp6_flow_key(row: MibTcp6RowOwnerPid) -> Option<FlowKey> {
        Some(FlowKey {
            protocol: Protocol::Tcp,
            local_addr: IpAddr::V6(Ipv6Addr::from(row.uc_local_addr)),
            local_port: port_from_dword(row.dw_local_port),
            remote_addr: IpAddr::V6(Ipv6Addr::from(row.uc_remote_addr)),
            remote_port: port_from_dword(row.dw_remote_port),
        })
    }

    fn udp4_row_matches(row: MibUdpRowOwnerPid, key: &FlowKey) -> bool {
        key.protocol == Protocol::Udp
            && local_addr_matches(IpAddr::V4(ipv4_from_dword(row.dw_local_addr)), key.local_addr)
            && port_from_dword(row.dw_local_port) == key.local_port
    }

    fn udp6_row_matches(row: MibUdp6RowOwnerPid, key: &FlowKey) -> bool {
        key.protocol == Protocol::Udp
            && local_addr_matches(IpAddr::V6(Ipv6Addr::from(row.uc_local_addr)), key.local_addr)
            && port_from_dword(row.dw_local_port) == key.local_port
    }

    fn local_addr_matches(owner_addr: IpAddr, packet_addr: IpAddr) -> bool {
        owner_addr == packet_addr
            || matches!(
                (owner_addr, packet_addr),
                (IpAddr::V4(addr), IpAddr::V4(_)) if addr.is_unspecified()
            )
            || matches!(
                (owner_addr, packet_addr),
                (IpAddr::V6(addr), IpAddr::V6(_)) if addr.is_unspecified()
            )
    }

    fn ipv4_from_dword(value: u32) -> Ipv4Addr {
        let [a, b, c, d] = value.to_ne_bytes();
        Ipv4Addr::new(a, b, c, d)
    }

    fn ipv4_to_dword(value: Ipv4Addr) -> u32 {
        u32::from_ne_bytes(value.octets())
    }

    fn port_from_dword(value: u32) -> u16 {
        u16::from_be((value & 0xffff) as u16)
    }

    fn port_to_dword(value: u16) -> u32 {
        value.to_be() as u32
    }

    #[cfg(test)]
    mod tests {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

        use linkroute_core::{FlowKey, Protocol};

        use super::{
            ipv4_to_dword, port_to_dword, tcp4_flow_key, tcp6_flow_key, udp4_row_matches,
            udp6_row_matches, MibTcp6RowOwnerPid, MibTcpRowOwnerPid, MibUdp6RowOwnerPid,
            MibUdpRowOwnerPid,
        };

        #[test]
        fn tcp4_owner_row_maps_to_flow_key() {
            let row = MibTcpRowOwnerPid {
                dw_state: 5,
                dw_local_addr: ipv4_to_dword(Ipv4Addr::new(192, 0, 2, 10)),
                dw_local_port: port_to_dword(50000),
                dw_remote_addr: ipv4_to_dword(Ipv4Addr::new(203, 0, 113, 20)),
                dw_remote_port: port_to_dword(443),
                dw_owning_pid: 4242,
            };

            let key = tcp4_flow_key(row).unwrap();

            assert_eq!(key.local_addr, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
            assert_eq!(key.local_port, 50000);
            assert_eq!(key.remote_addr, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)));
            assert_eq!(key.remote_port, 443);
        }

        #[test]
        fn tcp6_owner_row_maps_to_flow_key() {
            let local = Ipv6Addr::LOCALHOST;
            let remote = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
            let row = MibTcp6RowOwnerPid {
                uc_local_addr: local.octets(),
                dw_local_scope_id: 0,
                dw_local_port: port_to_dword(50000),
                uc_remote_addr: remote.octets(),
                dw_remote_scope_id: 0,
                dw_remote_port: port_to_dword(443),
                dw_state: 5,
                dw_owning_pid: 4242,
            };

            let key = tcp6_flow_key(row).unwrap();

            assert_eq!(key.local_addr, IpAddr::V6(local));
            assert_eq!(key.remote_addr, IpAddr::V6(remote));
            assert_eq!(key.local_port, 50000);
            assert_eq!(key.remote_port, 443);
        }

        #[test]
        fn udp_owner_rows_match_by_local_endpoint() {
            let key = FlowKey {
                protocol: Protocol::Udp,
                local_addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
                local_port: 5353,
                remote_addr: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)),
                remote_port: 443,
            };
            let row = MibUdpRowOwnerPid {
                dw_local_addr: ipv4_to_dword(Ipv4Addr::UNSPECIFIED),
                dw_local_port: port_to_dword(5353),
                dw_owning_pid: 4242,
            };

            assert!(udp4_row_matches(row, &key));
        }

        #[test]
        fn udp6_owner_rows_match_by_local_endpoint() {
            let local = Ipv6Addr::LOCALHOST;
            let key = FlowKey {
                protocol: Protocol::Udp,
                local_addr: IpAddr::V6(local),
                local_port: 5353,
                remote_addr: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
                remote_port: 443,
            };
            let row = MibUdp6RowOwnerPid {
                uc_local_addr: Ipv6Addr::UNSPECIFIED.octets(),
                dw_local_scope_id: 0,
                dw_local_port: port_to_dword(5353),
                dw_owning_pid: 4242,
            };

            assert!(udp6_row_matches(row, &key));
        }
    }
}
#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use linkroute_core::{FlowKey, PacketDirection, Protocol};

    use super::{FlowContextCache, PacketProcessContext, PacketProcessResolver};

    #[test]
    fn cache_resolves_outbound_packet_context() {
        let mut cache = FlowContextCache::default();
        cache.insert(
            sample_key(),
            PacketProcessContext::new(4242, Some(String::from(r"C:\Apps\browser.exe")), None),
        );

        let context = cache.resolve_packet(&sample_ipv4_tcp_packet(), PacketDirection::Outbound);

        assert_eq!(context.process_id, 4242);
        assert_eq!(context.exe_path.as_deref(), Some(r"C:\Apps\browser.exe"));
    }

    #[test]
    fn cache_resolves_reverse_direction_context() {
        let mut cache = FlowContextCache::default();
        cache.insert(
            sample_key(),
            PacketProcessContext::new(4242, Some(String::from(r"C:\Apps\browser.exe")), None),
        );
        let inbound = ipv4_tcp_packet(
            Ipv4Addr::new(203, 0, 113, 20),
            443,
            Ipv4Addr::new(192, 0, 2, 10),
            50000,
        );

        let context = cache.resolve_packet(&inbound, PacketDirection::Inbound);

        assert_eq!(context.process_id, 4242);
        assert_eq!(context.exe_path.as_deref(), Some(r"C:\Apps\browser.exe"));
    }

    #[test]
    fn malformed_packet_resolves_to_unknown_context() {
        let mut cache = FlowContextCache::default();

        let context = cache.resolve_packet(&[0xff], PacketDirection::Outbound);

        assert_eq!(context, PacketProcessContext::unknown());
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
}
