pub mod runtime;
pub use runtime::WindivertRuntime;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend::{BackendStatus, CaptureBackend};
use crate::control::{PacketRouteOutcome, ServiceController};
use crate::process_context::{NoopProcessResolver, PacketProcessResolver};
use linkroute_core::{
    FlowDescriptor, LinkState, PacketDirection, RouteAction, RouteConfig, RouteDecision,
};
use runtime::{
    WindivertAddress, WindivertLayer, WindivertSession, LINKROUTE_NETWORK_FILTER, MAX_PACKET_SIZE,
};

pub struct WindivertBackend {
    running: bool,
    active_flows: usize,
}

impl WindivertBackend {
    pub fn new() -> Self {
        Self { running: false, active_flows: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindivertProbe {
    pub search_dir: PathBuf,
    pub dll_present: bool,
    pub driver_present: bool,
}

impl WindivertProbe {
    pub fn probe_near_exe() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let search_dir = exe
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| String::from("could not determine executable directory"))?;
        Ok(Self::probe_dir(search_dir))
    }

    pub fn probe_dir(search_dir: PathBuf) -> Self {
        Self {
            dll_present: search_dir.join("WinDivert.dll").is_file(),
            driver_present: search_dir.join("WinDivert64.sys").is_file(),
            search_dir,
        }
    }

    pub fn ready(&self) -> bool {
        self.dll_present && self.driver_present
    }

    pub fn missing_files(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.dll_present {
            missing.push("WinDivert.dll");
        }
        if !self.driver_present {
            missing.push("WinDivert64.sys");
        }
        missing
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDisposition {
    ForwardUnchanged,
    ForwardRecalculateChecksums,
    Drop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PacketPumpStats {
    pub received: usize,
    pub sent: usize,
    pub dropped: usize,
    pub checksummed: usize,
}

pub trait PacketIo {
    fn recv_packet(
        &mut self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<usize, String>;

    fn send_packet(&mut self, packet: &[u8], address: &WindivertAddress) -> Result<usize, String>;

    fn calc_checksums(
        &mut self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<(), String>;
}

pub trait PacketHandler {
    fn handle_packet(
        &mut self,
        packet: &mut [u8],
        direction: PacketDirection,
    ) -> Result<PacketDisposition, String>;
}

pub fn run_packet_pump<I, H, S>(
    io: &mut I,
    handler: &mut H,
    mut should_stop: S,
) -> Result<PacketPumpStats, String>
where
    I: PacketIo,
    H: PacketHandler,
    S: FnMut(&PacketPumpStats) -> bool,
{
    let mut stats = PacketPumpStats::default();
    let mut packet = vec![0_u8; MAX_PACKET_SIZE];
    let mut address = WindivertAddress::default();

    while !should_stop(&stats) {
        let packet_len = io.recv_packet(&mut packet, &mut address)?;
        stats.received = stats.received.saturating_add(1);
        let packet = &mut packet[..packet_len];

        match handler.handle_packet(packet, address.direction())? {
            PacketDisposition::Drop => {
                stats.dropped = stats.dropped.saturating_add(1);
            }
            PacketDisposition::ForwardUnchanged => {
                send_complete_packet(io, packet, &address)?;
                stats.sent = stats.sent.saturating_add(1);
            }
            PacketDisposition::ForwardRecalculateChecksums => {
                io.calc_checksums(packet, &mut address)?;
                stats.checksummed = stats.checksummed.saturating_add(1);
                send_complete_packet(io, packet, &address)?;
                stats.sent = stats.sent.saturating_add(1);
            }
        }
    }

    Ok(stats)
}

#[allow(dead_code)]
pub fn run_network_packet_pump<H, S>(
    runtime: &WindivertRuntime,
    handler: &mut H,
    should_stop: S,
) -> Result<PacketPumpStats, String>
where
    H: PacketHandler,
    S: FnMut(&PacketPumpStats) -> bool,
{
    let mut session = runtime.open(LINKROUTE_NETWORK_FILTER, WindivertLayer::Network, 0, 0)?;
    run_packet_pump(&mut session, handler, should_stop)
}

#[allow(dead_code)]
#[cfg(windows)]
pub fn run_controller_network_packet_pump<S>(
    runtime: &WindivertRuntime,
    controller: &mut ServiceController,
    should_stop: S,
) -> Result<PacketPumpStats, String>
where
    S: FnMut(&PacketPumpStats) -> bool,
{
    let mut handler = ControllerPacketHandler::with_process_resolver(
        controller,
        current_epoch_seconds,
        crate::process_context::WindowsProcessResolver,
    );
    run_network_packet_pump(runtime, &mut handler, should_stop)
}

#[allow(dead_code)]
pub struct ControllerPacketHandler<'controller, F, R = NoopProcessResolver> {
    controller: &'controller mut ServiceController,
    now_epoch_seconds: F,
    process_resolver: R,
}

#[allow(dead_code)]
impl<'controller, F> ControllerPacketHandler<'controller, F, NoopProcessResolver> {
    pub fn new(controller: &'controller mut ServiceController, now_epoch_seconds: F) -> Self {
        Self::with_process_resolver(controller, now_epoch_seconds, NoopProcessResolver)
    }
}

#[allow(dead_code)]
impl<'controller, F, R> ControllerPacketHandler<'controller, F, R> {
    pub fn with_process_resolver(
        controller: &'controller mut ServiceController,
        now_epoch_seconds: F,
        process_resolver: R,
    ) -> Self {
        Self { controller, now_epoch_seconds, process_resolver }
    }
}

impl<F, R> PacketHandler for ControllerPacketHandler<'_, F, R>
where
    F: FnMut() -> u64,
    R: PacketProcessResolver,
{
    fn handle_packet(
        &mut self,
        packet: &mut [u8],
        direction: PacketDirection,
    ) -> Result<PacketDisposition, String> {
        let now_epoch_seconds = (self.now_epoch_seconds)();
        let process_context = self.process_resolver.resolve_packet(packet, direction);
        let outcome = self.controller.process_packet(
            now_epoch_seconds,
            packet,
            direction,
            process_context.process_id,
            process_context.exe_path,
            process_context.explicit_domain,
        )?;

        Ok(packet_disposition(outcome))
    }
}

#[allow(dead_code)]
pub struct SharedControllerPacketHandler<F, R = NoopProcessResolver> {
    controller: Arc<Mutex<ServiceController>>,
    now_epoch_seconds: F,
    process_resolver: R,
}

#[allow(dead_code)]
impl<F> SharedControllerPacketHandler<F, NoopProcessResolver> {
    pub fn new(controller: Arc<Mutex<ServiceController>>, now_epoch_seconds: F) -> Self {
        Self::with_process_resolver(controller, now_epoch_seconds, NoopProcessResolver)
    }
}

#[allow(dead_code)]
impl<F, R> SharedControllerPacketHandler<F, R> {
    pub fn with_process_resolver(
        controller: Arc<Mutex<ServiceController>>,
        now_epoch_seconds: F,
        process_resolver: R,
    ) -> Self {
        Self { controller, now_epoch_seconds, process_resolver }
    }
}

impl<F, R> PacketHandler for SharedControllerPacketHandler<F, R>
where
    F: FnMut() -> u64,
    R: PacketProcessResolver,
{
    fn handle_packet(
        &mut self,
        packet: &mut [u8],
        direction: PacketDirection,
    ) -> Result<PacketDisposition, String> {
        let now_epoch_seconds = (self.now_epoch_seconds)();
        let process_context = self.process_resolver.resolve_packet(packet, direction);
        let mut controller = self
            .controller
            .lock()
            .map_err(|_| String::from("service controller lock is poisoned"))?;
        let outcome = controller.process_packet(
            now_epoch_seconds,
            packet,
            direction,
            process_context.process_id,
            process_context.exe_path,
            process_context.explicit_domain,
        )?;

        Ok(packet_disposition(outcome))
    }
}

#[allow(dead_code)]
fn packet_disposition(outcome: PacketRouteOutcome) -> PacketDisposition {
    match outcome {
        PacketRouteOutcome::Forwarded { .. } => PacketDisposition::ForwardUnchanged,
        PacketRouteOutcome::Dropped { .. } => PacketDisposition::Drop,
        PacketRouteOutcome::Rewritten { .. } => PacketDisposition::ForwardRecalculateChecksums,
    }
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0)
}

fn send_complete_packet<I: PacketIo>(
    io: &mut I,
    packet: &[u8],
    address: &WindivertAddress,
) -> Result<(), String> {
    let sent = io.send_packet(packet, address)?;
    if sent != packet.len() {
        return Err(format!("WinDivertSend wrote {sent} bytes for {} byte packet", packet.len()));
    }
    Ok(())
}

impl PacketIo for WindivertSession<'_> {
    fn recv_packet(
        &mut self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<usize, String> {
        WindivertSession::recv_packet(self, packet, address)
    }

    fn send_packet(&mut self, packet: &[u8], address: &WindivertAddress) -> Result<usize, String> {
        WindivertSession::send_packet(self, packet, address)
    }

    fn calc_checksums(
        &mut self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<(), String> {
        WindivertSession::calc_checksums(self, packet, address)
    }
}

impl CaptureBackend for WindivertBackend {
    fn name(&self) -> &'static str {
        "windivert"
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
        BackendStatus { name: self.name(), running: self.running, active_flows: self.active_flows }
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
        if self.running && !matches!(decision.action, RouteAction::UseSystemDefault) {
            self.active_flows = self.active_flows.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    use crate::backend::{BackendStatus, CaptureBackend};
    use crate::process_context::{FlowContextCache, PacketProcessContext};
    use linkroute_core::{
        parse_ip_packet, AdapterInventory, AdapterKind, AdapterSelection, ApplicationRule,
        FlowDescriptor, FlowKey, LinkState, NetworkAdapter, Outlet, PacketDirection, Protocol,
        RouteAction, RouteConfig, RouteDecision,
    };

    use super::{
        run_packet_pump, ControllerPacketHandler, PacketDisposition, PacketHandler, PacketIo,
        SharedControllerPacketHandler, WindivertAddress, WindivertProbe,
    };

    #[test]
    fn probe_reports_missing_files() {
        let dir =
            std::env::temp_dir().join(format!("linkroute_probe_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let probe = WindivertProbe::probe_dir(dir.clone());

        assert!(!probe.ready());
        assert_eq!(probe.missing_files(), vec!["WinDivert.dll", "WinDivert64.sys"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_accepts_required_files() {
        let dir =
            std::env::temp_dir().join(format!("linkroute_probe_ready_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("WinDivert.dll"), b"").unwrap();
        fs::write(dir.join("WinDivert64.sys"), b"").unwrap();

        let probe = WindivertProbe::probe_dir(dir.clone());

        assert!(probe.ready());
        assert!(probe.missing_files().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn packet_pump_sends_forwarded_packets() {
        let mut io = FakePacketIo::new(vec![vec![1, 2, 3]]);
        let mut handler = StaticHandler::new(PacketDisposition::ForwardUnchanged);

        let stats = run_packet_pump(&mut io, &mut handler, |stats| stats.received >= 1).unwrap();

        assert_eq!(stats.received, 1);
        assert_eq!(stats.sent, 1);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.checksummed, 0);
        assert_eq!(io.sent_packets, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn packet_pump_recalculates_checksums_for_rewritten_packets() {
        let mut io = FakePacketIo::new(vec![vec![1, 2, 3]]);
        let mut handler = MutatingHandler;

        let stats = run_packet_pump(&mut io, &mut handler, |stats| stats.received >= 1).unwrap();

        assert_eq!(stats.sent, 1);
        assert_eq!(stats.checksummed, 1);
        assert_eq!(io.checksum_calls, 1);
        assert_eq!(io.sent_packets, vec![vec![9, 2, 3]]);
    }

    #[test]
    fn packet_pump_drops_packets_without_reinjection() {
        let mut io = FakePacketIo::new(vec![vec![1, 2, 3]]);
        let mut handler = StaticHandler::new(PacketDisposition::Drop);

        let stats = run_packet_pump(&mut io, &mut handler, |stats| stats.received >= 1).unwrap();

        assert_eq!(stats.received, 1);
        assert_eq!(stats.sent, 0);
        assert_eq!(stats.dropped, 1);
        assert!(io.sent_packets.is_empty());
    }

    #[test]
    fn controller_packet_handler_uses_resolved_process_context_for_app_rule() {
        let mut controller = controller_with_app_rule(Outlet::Ethernet);
        controller.set_adapters(sample_adapters(), sample_selection());
        controller.start().unwrap();

        let mut process_cache = FlowContextCache::default();
        process_cache.insert(
            sample_flow_key(),
            PacketProcessContext::new(4242, Some(String::from(r"C:\Apps\browser.exe")), None),
        );
        let mut handler =
            ControllerPacketHandler::with_process_resolver(&mut controller, || 100, process_cache);
        let mut packet = sample_ipv4_tcp_packet();

        let disposition = handler.handle_packet(&mut packet, PacketDirection::Outbound).unwrap();

        assert_eq!(disposition, PacketDisposition::ForwardRecalculateChecksums);
        let parsed = parse_ip_packet(&packet).unwrap();
        assert_eq!(parsed.source_addr, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)));
    }

    #[test]
    fn shared_controller_packet_handler_locks_controller_for_packet_processing() {
        let mut controller = controller_with_app_rule(Outlet::Ethernet);
        controller.set_adapters(sample_adapters(), sample_selection());
        controller.start().unwrap();
        let controller = Arc::new(Mutex::new(controller));

        let mut process_cache = FlowContextCache::default();
        process_cache.insert(
            sample_flow_key(),
            PacketProcessContext::new(4242, Some(String::from(r"C:\Apps\browser.exe")), None),
        );
        let mut handler = SharedControllerPacketHandler::with_process_resolver(
            Arc::clone(&controller),
            || 100,
            process_cache,
        );
        let mut packet = sample_ipv4_tcp_packet();

        let disposition = handler.handle_packet(&mut packet, PacketDirection::Outbound).unwrap();

        assert_eq!(disposition, PacketDisposition::ForwardRecalculateChecksums);
        assert_eq!(controller.lock().unwrap().flow_count(), 1);
    }
    fn controller_with_app_rule(outlet: Outlet) -> crate::control::ServiceController {
        let mut config = RouteConfig::enabled();
        config
            .application_rules
            .push(ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", outlet).unwrap());
        crate::control::ServiceController::new(config, Box::new(FakeBackend::default()))
    }

    fn sample_adapters() -> AdapterInventory {
        AdapterInventory::new(vec![NetworkAdapter {
            id: String::from("eth"),
            name: String::from("Ethernet"),
            kind: AdapterKind::Ethernet,
            interface_index: 10,
            up: true,
            physical: true,
            addresses: vec![IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9))],
        }])
    }

    fn sample_selection() -> AdapterSelection {
        AdapterSelection { ethernet_id: Some(String::from("eth")), wifi_id: None }
    }

    fn sample_flow_key() -> FlowKey {
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

    struct FakePacketIo {
        recv_packets: VecDeque<Vec<u8>>,
        sent_packets: Vec<Vec<u8>>,
        checksum_calls: usize,
    }

    impl FakePacketIo {
        fn new(packets: Vec<Vec<u8>>) -> Self {
            Self {
                recv_packets: VecDeque::from(packets),
                sent_packets: Vec::new(),
                checksum_calls: 0,
            }
        }
    }

    impl PacketIo for FakePacketIo {
        fn recv_packet(
            &mut self,
            packet: &mut [u8],
            _address: &mut WindivertAddress,
        ) -> Result<usize, String> {
            let next = self
                .recv_packets
                .pop_front()
                .ok_or_else(|| String::from("no test packet queued"))?;
            packet[..next.len()].copy_from_slice(&next);
            Ok(next.len())
        }

        fn send_packet(
            &mut self,
            packet: &[u8],
            _address: &WindivertAddress,
        ) -> Result<usize, String> {
            self.sent_packets.push(packet.to_vec());
            Ok(packet.len())
        }

        fn calc_checksums(
            &mut self,
            _packet: &mut [u8],
            _address: &mut WindivertAddress,
        ) -> Result<(), String> {
            self.checksum_calls += 1;
            Ok(())
        }
    }

    struct StaticHandler {
        disposition: PacketDisposition,
    }

    impl StaticHandler {
        fn new(disposition: PacketDisposition) -> Self {
            Self { disposition }
        }
    }

    impl PacketHandler for StaticHandler {
        fn handle_packet(
            &mut self,
            _packet: &mut [u8],
            direction: PacketDirection,
        ) -> Result<PacketDisposition, String> {
            assert_eq!(direction, PacketDirection::Inbound);
            Ok(self.disposition)
        }
    }

    struct MutatingHandler;

    impl PacketHandler for MutatingHandler {
        fn handle_packet(
            &mut self,
            packet: &mut [u8],
            _direction: PacketDirection,
        ) -> Result<PacketDisposition, String> {
            packet[0] = 9;
            Ok(PacketDisposition::ForwardRecalculateChecksums)
        }
    }
}
