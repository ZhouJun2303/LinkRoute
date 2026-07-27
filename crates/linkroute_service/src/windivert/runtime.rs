#![allow(dead_code)]

use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::Library;
use linkroute_core::PacketDirection;

pub type WindivertHandle = isize;

pub const WINDIVERT_ADDRESS_SIZE: usize = 80;
pub const MAX_PACKET_SIZE: usize = 0xffff;
pub const LINKROUTE_NETWORK_FILTER: &str = "(ip or ipv6) and (tcp or udp or icmp or icmpv6)";

const OUTBOUND_FLAG: u32 = 1 << 17;
const IPV6_FLAG: u32 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindivertAddress {
    bytes: [u8; WINDIVERT_ADDRESS_SIZE],
}

impl WindivertAddress {
    pub fn new() -> Self {
        Self { bytes: [0; WINDIVERT_ADDRESS_SIZE] }
    }

    pub fn direction(&self) -> PacketDirection {
        if self.flags() & OUTBOUND_FLAG != 0 {
            PacketDirection::Outbound
        } else {
            PacketDirection::Inbound
        }
    }

    pub fn is_ipv6(&self) -> bool {
        self.flags() & IPV6_FLAG != 0
    }

    pub fn as_bytes(&self) -> &[u8; WINDIVERT_ADDRESS_SIZE] {
        &self.bytes
    }

    fn as_ptr(&self) -> *const c_void {
        self.bytes.as_ptr().cast()
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr().cast()
    }

    fn flags(&self) -> u32 {
        u32::from_ne_bytes([self.bytes[8], self.bytes[9], self.bytes[10], self.bytes[11]])
    }

    #[cfg(test)]
    fn with_flags(flags: u32) -> Self {
        let mut address = Self::new();
        address.bytes[8..12].copy_from_slice(&flags.to_ne_bytes());
        address
    }
}

impl Default for WindivertAddress {
    fn default() -> Self {
        Self::new()
    }
}

type RawOpen = unsafe extern "system" fn(*const c_char, u32, i16, u64) -> WindivertHandle;
type RawClose = unsafe extern "system" fn(WindivertHandle) -> i32;
type RawShutdown = unsafe extern "system" fn(WindivertHandle, u32) -> i32;
type RawRecv =
    unsafe extern "system" fn(WindivertHandle, *mut c_void, u32, *mut u32, *mut c_void) -> i32;
type RawSend =
    unsafe extern "system" fn(WindivertHandle, *const c_void, u32, *mut u32, *const c_void) -> i32;
type RawCalcChecksums = unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u64) -> i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindivertLayer {
    Network = 0,
    NetworkForward = 1,
    Flow = 2,
    Socket = 3,
    Reflect = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindivertShutdown {
    Recv = 1,
    Send = 2,
    Both = 3,
}

const INVALID_HANDLE_VALUE: WindivertHandle = -1;

pub struct WindivertRuntime {
    dll_path: PathBuf,
    _library: Library,
    open: RawOpen,
    close: RawClose,
    shutdown: RawShutdown,
    recv: RawRecv,
    send: RawSend,
    calc_checksums: RawCalcChecksums,
}

impl WindivertRuntime {
    pub fn load_from_dir(dir: &Path) -> Result<Self, String> {
        let dll_path = dir.join("WinDivert.dll");
        if !dll_path.is_file() {
            return Err(format!("missing {}", dll_path.display()));
        }

        // Loading a trusted, hash-checked WinDivert DLL from the application directory.
        let library = unsafe { Library::new(&dll_path) }.map_err(|error| error.to_string())?;
        // Symbols are copied as function pointers while the owning Library is kept alive.
        let open = *unsafe { library.get::<RawOpen>(b"WinDivertOpen\0") }
            .map_err(|error| error.to_string())?;
        let close = *unsafe { library.get::<RawClose>(b"WinDivertClose\0") }
            .map_err(|error| error.to_string())?;
        let shutdown = *unsafe { library.get::<RawShutdown>(b"WinDivertShutdown\0") }
            .map_err(|error| error.to_string())?;
        let recv = *unsafe { library.get::<RawRecv>(b"WinDivertRecv\0") }
            .map_err(|error| error.to_string())?;
        let send = *unsafe { library.get::<RawSend>(b"WinDivertSend\0") }
            .map_err(|error| error.to_string())?;
        let calc_checksums =
            *unsafe { library.get::<RawCalcChecksums>(b"WinDivertHelperCalcChecksums\0") }
                .map_err(|error| error.to_string())?;

        Ok(Self { dll_path, _library: library, open, close, shutdown, recv, send, calc_checksums })
    }

    pub fn dll_path(&self) -> &Path {
        &self.dll_path
    }

    pub fn open(
        &self,
        filter: &str,
        layer: WindivertLayer,
        priority: i16,
        flags: u64,
    ) -> Result<WindivertSession<'_>, String> {
        Ok(WindivertSession {
            runtime: self,
            handle: self.open_handle(filter, layer, priority, flags)?,
        })
    }

    pub fn open_owned(
        runtime: Arc<Self>,
        filter: &str,
        layer: WindivertLayer,
        priority: i16,
        flags: u64,
    ) -> Result<WindivertOwnedSession, String> {
        let handle = runtime.open_handle(filter, layer, priority, flags)?;
        Ok(WindivertOwnedSession { runtime, handle })
    }

    fn open_handle(
        &self,
        filter: &str,
        layer: WindivertLayer,
        priority: i16,
        flags: u64,
    ) -> Result<WindivertHandle, String> {
        let filter = CString::new(filter).map_err(|_| String::from("filter contains NUL byte"))?;
        let handle = unsafe { (self.open)(filter.as_ptr(), layer as u32, priority, flags) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!("WinDivertOpen failed: {}", std::io::Error::last_os_error()));
        }
        Ok(handle)
    }

    pub fn shutdown_handle(
        &self,
        handle: WindivertHandle,
        how: WindivertShutdown,
    ) -> Result<(), String> {
        if unsafe { (self.shutdown)(handle, how as u32) } == 0 {
            return Err(format!("WinDivertShutdown failed: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }

    pub fn calc_checksums(
        &self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<(), String> {
        if unsafe {
            (self.calc_checksums)(
                packet.as_mut_ptr().cast(),
                packet.len() as u32,
                address.as_mut_ptr(),
                0,
            )
        } == 0
        {
            return Err(format!(
                "WinDivertHelperCalcChecksums failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    pub fn resolved_symbol_count(&self) -> usize {
        6
    }
}

pub struct WindivertSession<'runtime> {
    runtime: &'runtime WindivertRuntime,
    handle: WindivertHandle,
}

impl WindivertSession<'_> {
    pub fn handle(&self) -> WindivertHandle {
        self.handle
    }

    pub fn shutdown(&self, how: WindivertShutdown) -> Result<(), String> {
        self.runtime.shutdown_handle(self.handle, how)
    }

    pub fn recv_packet(
        &self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<usize, String> {
        recv_packet(self.runtime.recv, self.handle, packet, address)
    }

    pub fn send_packet(&self, packet: &[u8], address: &WindivertAddress) -> Result<usize, String> {
        send_packet(self.runtime.send, self.handle, packet, address)
    }

    pub fn calc_checksums(
        &self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<(), String> {
        self.runtime.calc_checksums(packet, address)
    }
}

impl Drop for WindivertSession<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.runtime.close)(self.handle);
        }
    }
}

pub struct WindivertOwnedSession {
    runtime: Arc<WindivertRuntime>,
    handle: WindivertHandle,
}

impl WindivertOwnedSession {
    pub fn handle(&self) -> WindivertHandle {
        self.handle
    }

    pub fn stopper(&self) -> WindivertSessionStopper {
        WindivertSessionStopper { runtime: Arc::clone(&self.runtime), handle: self.handle }
    }

    pub fn shutdown(&self, how: WindivertShutdown) -> Result<(), String> {
        self.runtime.shutdown_handle(self.handle, how)
    }

    pub fn recv_packet(
        &self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<usize, String> {
        recv_packet(self.runtime.recv, self.handle, packet, address)
    }

    pub fn send_packet(&self, packet: &[u8], address: &WindivertAddress) -> Result<usize, String> {
        send_packet(self.runtime.send, self.handle, packet, address)
    }

    pub fn calc_checksums(
        &self,
        packet: &mut [u8],
        address: &mut WindivertAddress,
    ) -> Result<(), String> {
        self.runtime.calc_checksums(packet, address)
    }
}

impl Drop for WindivertOwnedSession {
    fn drop(&mut self) {
        unsafe {
            (self.runtime.close)(self.handle);
        }
    }
}

#[derive(Clone)]
pub struct WindivertSessionStopper {
    runtime: Arc<WindivertRuntime>,
    handle: WindivertHandle,
}

impl WindivertSessionStopper {
    pub fn shutdown_recv(&self) -> Result<(), String> {
        self.runtime.shutdown_handle(self.handle, WindivertShutdown::Recv)
    }

    pub fn shutdown_both(&self) -> Result<(), String> {
        self.runtime.shutdown_handle(self.handle, WindivertShutdown::Both)
    }
}

fn recv_packet(
    recv: RawRecv,
    handle: WindivertHandle,
    packet: &mut [u8],
    address: &mut WindivertAddress,
) -> Result<usize, String> {
    let mut recv_len = 0_u32;
    if unsafe {
        recv(
            handle,
            packet.as_mut_ptr().cast(),
            packet.len() as u32,
            &mut recv_len,
            address.as_mut_ptr(),
        )
    } == 0
    {
        return Err(format!("WinDivertRecv failed: {}", std::io::Error::last_os_error()));
    }
    Ok(recv_len as usize)
}

fn send_packet(
    send: RawSend,
    handle: WindivertHandle,
    packet: &[u8],
    address: &WindivertAddress,
) -> Result<usize, String> {
    let mut send_len = 0_u32;
    if unsafe {
        send(handle, packet.as_ptr().cast(), packet.len() as u32, &mut send_len, address.as_ptr())
    } == 0
    {
        return Err(format!("WinDivertSend failed: {}", std::io::Error::last_os_error()));
    }
    Ok(send_len as usize)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use linkroute_core::PacketDirection;

    use super::{
        WindivertAddress, WindivertRuntime, WindivertShutdown, IPV6_FLAG, OUTBOUND_FLAG,
        WINDIVERT_ADDRESS_SIZE,
    };

    #[test]
    fn missing_dll_returns_error() {
        let dir = std::env::temp_dir()
            .join(format!("linkroute_windivert_runtime_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let error = match WindivertRuntime::load_from_dir(&dir) {
            Ok(_) => panic!("missing DLL should fail"),
            Err(error) => error,
        };

        assert!(error.contains("WinDivert.dll"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn address_buffer_matches_windivert_2_2_layout_size() {
        assert_eq!(WindivertAddress::default().as_bytes().len(), WINDIVERT_ADDRESS_SIZE);
        assert_eq!(WINDIVERT_ADDRESS_SIZE, 80);
    }

    #[test]
    fn address_decodes_direction_and_ip_version_flags() {
        let address = WindivertAddress::with_flags(OUTBOUND_FLAG | IPV6_FLAG);

        assert_eq!(address.direction(), PacketDirection::Outbound);
        assert!(address.is_ipv6());
        assert_eq!(WindivertAddress::with_flags(0).direction(), PacketDirection::Inbound);
    }

    #[test]
    fn shutdown_values_match_windivert_header() {
        assert_eq!(WindivertShutdown::Recv as u32, 1);
        assert_eq!(WindivertShutdown::Send as u32, 2);
        assert_eq!(WindivertShutdown::Both as u32, 3);
    }
}
