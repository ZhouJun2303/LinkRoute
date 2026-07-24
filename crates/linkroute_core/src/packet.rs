use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::{FlowKey, Protocol};

const IPV4_MIN_HEADER_LEN: usize = 20;
const IPV6_HEADER_LEN: usize = 40;
const TCP_PROTOCOL: u8 = 6;
const UDP_PROTOCOL: u8 = 17;
const ICMP_PROTOCOL: u8 = 1;
const ICMPV6_PROTOCOL: u8 = 58;
const IPV6_HOP_BY_HOP: u8 = 0;
const IPV6_ROUTING: u8 = 43;
const IPV6_FRAGMENT: u8 = 44;
const IPV6_AUTHENTICATION: u8 = 51;
const IPV6_DESTINATION_OPTIONS: u8 = 60;
const IPV6_NO_NEXT_HEADER: u8 = 59;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    Outbound,
    Inbound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPacket {
    pub source_addr: IpAddr,
    pub destination_addr: IpAddr,
    pub protocol: Protocol,
    pub source_port: Option<u16>,
    pub destination_port: Option<u16>,
    pub transport_offset: usize,
}

impl ParsedPacket {
    pub fn flow_key(&self, direction: PacketDirection) -> FlowKey {
        let source_port = self.source_port.unwrap_or(0);
        let destination_port = self.destination_port.unwrap_or(0);
        match direction {
            PacketDirection::Outbound => FlowKey {
                protocol: self.protocol,
                local_addr: self.source_addr,
                local_port: source_port,
                remote_addr: self.destination_addr,
                remote_port: destination_port,
            },
            PacketDirection::Inbound => FlowKey {
                protocol: self.protocol,
                local_addr: self.destination_addr,
                local_port: destination_port,
                remote_addr: self.source_addr,
                remote_port: source_port,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketParseError {
    Empty,
    TooShort { needed: usize, actual: usize },
    UnsupportedVersion(u8),
    InvalidIpv4HeaderLength(u8),
    InvalidTotalLength { total_length: usize, header_length: usize },
    FragmentedIpv4,
    FragmentedIpv6,
    MalformedIpv6Extension,
    NoNextHeader,
}

impl fmt::Display for PacketParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "packet is empty"),
            Self::TooShort { needed, actual } => {
                write!(formatter, "packet is too short: need {needed} bytes, got {actual}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported IP version {version}")
            }
            Self::InvalidIpv4HeaderLength(length) => {
                write!(formatter, "invalid IPv4 header length {length}")
            }
            Self::InvalidTotalLength { total_length, header_length } => write!(
                formatter,
                "invalid IPv4 total length {total_length} for header length {header_length}"
            ),
            Self::FragmentedIpv4 => {
                write!(formatter, "fragmented IPv4 packet cannot be routed yet")
            }
            Self::FragmentedIpv6 => {
                write!(formatter, "fragmented IPv6 packet cannot be routed yet")
            }
            Self::MalformedIpv6Extension => write!(formatter, "malformed IPv6 extension header"),
            Self::NoNextHeader => write!(formatter, "IPv6 packet has no next header"),
        }
    }
}

impl std::error::Error for PacketParseError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketRewriteError {
    Parse(PacketParseError),
    AddressFamilyMismatch { packet_version: u8, address: IpAddr },
    TransportPortsUnavailable(Protocol),
}

impl fmt::Display for PacketRewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::AddressFamilyMismatch { packet_version, address } => {
                write!(formatter, "cannot write address {address} into IPv{packet_version} packet")
            }
            Self::TransportPortsUnavailable(protocol) => {
                write!(formatter, "cannot rewrite ports for protocol {protocol:?}")
            }
        }
    }
}

impl std::error::Error for PacketRewriteError {}

impl From<PacketParseError> for PacketRewriteError {
    fn from(error: PacketParseError) -> Self {
        Self::Parse(error)
    }
}

pub fn parse_ip_packet(packet: &[u8]) -> Result<ParsedPacket, PacketParseError> {
    let Some(first) = packet.first() else {
        return Err(PacketParseError::Empty);
    };

    match first >> 4 {
        4 => parse_ipv4_packet(packet),
        6 => parse_ipv6_packet(packet),
        version => Err(PacketParseError::UnsupportedVersion(version)),
    }
}

fn parse_ipv4_packet(packet: &[u8]) -> Result<ParsedPacket, PacketParseError> {
    require_len(packet, IPV4_MIN_HEADER_LEN)?;
    let header_length = usize::from(packet[0] & 0x0f) * 4;
    if header_length < IPV4_MIN_HEADER_LEN || header_length > usize::from(u8::MAX) {
        return Err(PacketParseError::InvalidIpv4HeaderLength(packet[0] & 0x0f));
    }
    require_len(packet, header_length)?;

    let total_length = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    if total_length < header_length {
        return Err(PacketParseError::InvalidTotalLength { total_length, header_length });
    }
    require_len(packet, total_length)?;

    let fragment_flags = u16::from_be_bytes([packet[6], packet[7]]);
    let more_fragments = (fragment_flags & 0x2000) != 0;
    let fragment_offset = fragment_flags & 0x1fff;
    if more_fragments || fragment_offset != 0 {
        return Err(PacketParseError::FragmentedIpv4);
    }

    let protocol = packet[9];
    let source_addr = IpAddr::V4(Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]));
    let destination_addr =
        IpAddr::V4(Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]));
    let (protocol, source_port, destination_port) =
        parse_transport(packet, protocol, header_length)?;

    Ok(ParsedPacket {
        source_addr,
        destination_addr,
        protocol,
        source_port,
        destination_port,
        transport_offset: header_length,
    })
}

fn parse_ipv6_packet(packet: &[u8]) -> Result<ParsedPacket, PacketParseError> {
    require_len(packet, IPV6_HEADER_LEN)?;
    let payload_length = usize::from(u16::from_be_bytes([packet[4], packet[5]]));
    if payload_length != 0 {
        require_len(packet, IPV6_HEADER_LEN + payload_length)?;
    }

    let source_addr = IpAddr::V6(Ipv6Addr::from(read_array::<16>(packet, 8)?));
    let destination_addr = IpAddr::V6(Ipv6Addr::from(read_array::<16>(packet, 24)?));
    let (next_header, transport_offset) = find_ipv6_transport(packet, packet[6], IPV6_HEADER_LEN)?;
    let (protocol, source_port, destination_port) =
        parse_transport(packet, next_header, transport_offset)?;

    Ok(ParsedPacket {
        source_addr,
        destination_addr,
        protocol,
        source_port,
        destination_port,
        transport_offset,
    })
}

fn find_ipv6_transport(
    packet: &[u8],
    mut next_header: u8,
    mut offset: usize,
) -> Result<(u8, usize), PacketParseError> {
    loop {
        match next_header {
            TCP_PROTOCOL | UDP_PROTOCOL | ICMP_PROTOCOL | ICMPV6_PROTOCOL => {
                return Ok((next_header, offset));
            }
            IPV6_NO_NEXT_HEADER => return Err(PacketParseError::NoNextHeader),
            IPV6_HOP_BY_HOP | IPV6_ROUTING | IPV6_DESTINATION_OPTIONS => {
                require_len(packet, offset + 2)?;
                let header_len = (usize::from(packet[offset + 1]) + 1) * 8;
                if header_len < 8 {
                    return Err(PacketParseError::MalformedIpv6Extension);
                }
                next_header = packet[offset];
                offset = offset
                    .checked_add(header_len)
                    .ok_or(PacketParseError::MalformedIpv6Extension)?;
                require_len(packet, offset)?;
            }
            IPV6_FRAGMENT => {
                require_len(packet, offset + 8)?;
                let fragment = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
                let fragment_offset = (fragment & 0xfff8) >> 3;
                let more_fragments = (fragment & 0x0001) != 0;
                if fragment_offset != 0 || more_fragments {
                    return Err(PacketParseError::FragmentedIpv6);
                }
                next_header = packet[offset];
                offset += 8;
            }
            IPV6_AUTHENTICATION => {
                require_len(packet, offset + 2)?;
                let header_len = (usize::from(packet[offset + 1]) + 2) * 4;
                next_header = packet[offset];
                offset = offset
                    .checked_add(header_len)
                    .ok_or(PacketParseError::MalformedIpv6Extension)?;
                require_len(packet, offset)?;
            }
            other => return Ok((other, offset)),
        }
    }
}

fn parse_transport(
    packet: &[u8],
    protocol: u8,
    offset: usize,
) -> Result<(Protocol, Option<u16>, Option<u16>), PacketParseError> {
    let route_protocol = match protocol {
        TCP_PROTOCOL => Protocol::Tcp,
        UDP_PROTOCOL => Protocol::Udp,
        ICMP_PROTOCOL | ICMPV6_PROTOCOL => Protocol::Icmp,
        other => Protocol::Other(other),
    };

    match route_protocol {
        Protocol::Tcp | Protocol::Udp => {
            require_len(packet, offset + 4)?;
            Ok((
                route_protocol,
                Some(u16::from_be_bytes([packet[offset], packet[offset + 1]])),
                Some(u16::from_be_bytes([packet[offset + 2], packet[offset + 3]])),
            ))
        }
        Protocol::Icmp | Protocol::Other(_) => Ok((route_protocol, None, None)),
    }
}

pub fn rewrite_packet_source(
    packet: &mut [u8],
    address: IpAddr,
    port: Option<u16>,
) -> Result<ParsedPacket, PacketRewriteError> {
    rewrite_packet_endpoint(packet, EndpointSide::Source, address, port)
}

pub fn rewrite_packet_destination(
    packet: &mut [u8],
    address: IpAddr,
    port: Option<u16>,
) -> Result<ParsedPacket, PacketRewriteError> {
    rewrite_packet_endpoint(packet, EndpointSide::Destination, address, port)
}

fn rewrite_packet_endpoint(
    packet: &mut [u8],
    side: EndpointSide,
    address: IpAddr,
    port: Option<u16>,
) -> Result<ParsedPacket, PacketRewriteError> {
    let parsed = parse_ip_packet(packet)?;
    rewrite_address(packet, side, address)?;
    if let Some(port) = port {
        rewrite_port(packet, side, &parsed, port)?;
    }
    clear_checksum_fields(packet, &parsed)?;
    parse_ip_packet(packet).map_err(PacketRewriteError::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointSide {
    Source,
    Destination,
}

fn rewrite_address(
    packet: &mut [u8],
    side: EndpointSide,
    address: IpAddr,
) -> Result<(), PacketRewriteError> {
    match (packet[0] >> 4, address) {
        (4, IpAddr::V4(address)) => {
            let offset = match side {
                EndpointSide::Source => 12,
                EndpointSide::Destination => 16,
            };
            packet[offset..offset + 4].copy_from_slice(&address.octets());
            Ok(())
        }
        (6, IpAddr::V6(address)) => {
            let offset = match side {
                EndpointSide::Source => 8,
                EndpointSide::Destination => 24,
            };
            packet[offset..offset + 16].copy_from_slice(&address.octets());
            Ok(())
        }
        (packet_version, address) => {
            Err(PacketRewriteError::AddressFamilyMismatch { packet_version, address })
        }
    }
}

fn rewrite_port(
    packet: &mut [u8],
    side: EndpointSide,
    parsed: &ParsedPacket,
    port: u16,
) -> Result<(), PacketRewriteError> {
    match parsed.protocol {
        Protocol::Tcp | Protocol::Udp => {
            let offset = match side {
                EndpointSide::Source => parsed.transport_offset,
                EndpointSide::Destination => parsed.transport_offset + 2,
            };
            require_len(packet, offset + 2)?;
            packet[offset..offset + 2].copy_from_slice(&port.to_be_bytes());
            Ok(())
        }
        protocol => Err(PacketRewriteError::TransportPortsUnavailable(protocol)),
    }
}

fn clear_checksum_fields(
    packet: &mut [u8],
    parsed: &ParsedPacket,
) -> Result<(), PacketRewriteError> {
    if packet[0] >> 4 == 4 {
        require_len(packet, 12)?;
        packet[10..12].fill(0);
    }

    match parsed.protocol {
        Protocol::Tcp => {
            require_len(packet, parsed.transport_offset + 18)?;
            packet[parsed.transport_offset + 16..parsed.transport_offset + 18].fill(0);
        }
        Protocol::Udp => {
            require_len(packet, parsed.transport_offset + 8)?;
            packet[parsed.transport_offset + 6..parsed.transport_offset + 8].fill(0);
        }
        Protocol::Icmp => {
            require_len(packet, parsed.transport_offset + 4)?;
            packet[parsed.transport_offset + 2..parsed.transport_offset + 4].fill(0);
        }
        Protocol::Other(_) => {}
    }
    Ok(())
}
fn read_array<const N: usize>(packet: &[u8], offset: usize) -> Result<[u8; N], PacketParseError> {
    require_len(packet, offset + N)?;
    let mut data = [0_u8; N];
    data.copy_from_slice(&packet[offset..offset + N]);
    Ok(data)
}

fn require_len(packet: &[u8], needed: usize) -> Result<(), PacketParseError> {
    if packet.len() < needed {
        return Err(PacketParseError::TooShort { needed, actual: packet.len() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use crate::{PacketDirection, PacketParseError, Protocol};

    use super::parse_ip_packet;

    #[test]
    fn parses_ipv4_tcp_packet() {
        let packet = ipv4_packet(6, &[0xc3, 0x50, 0x01, 0xbb, 0, 0, 0, 0]);

        let parsed = parse_ip_packet(&packet).unwrap();

        assert_eq!(parsed.source_addr, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
        assert_eq!(parsed.destination_addr, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)));
        assert_eq!(parsed.protocol, Protocol::Tcp);
        assert_eq!(parsed.source_port, Some(50000));
        assert_eq!(parsed.destination_port, Some(443));
        assert_eq!(parsed.transport_offset, 20);
    }

    #[test]
    fn maps_outbound_and_inbound_flow_keys() {
        let packet = ipv4_packet(17, &[0xc3, 0x50, 0x00, 0x35, 0, 8, 0, 0]);
        let parsed = parse_ip_packet(&packet).unwrap();

        let outbound = parsed.flow_key(PacketDirection::Outbound);
        let inbound = parsed.flow_key(PacketDirection::Inbound);

        assert_eq!(outbound.local_addr, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
        assert_eq!(outbound.local_port, 50000);
        assert_eq!(outbound.remote_addr, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)));
        assert_eq!(outbound.remote_port, 53);
        assert_eq!(inbound.local_addr, outbound.remote_addr);
        assert_eq!(inbound.local_port, outbound.remote_port);
        assert_eq!(inbound.remote_addr, outbound.local_addr);
    }

    #[test]
    fn rejects_fragmented_ipv4_packet() {
        let mut packet = ipv4_packet(17, &[0, 1, 0, 2, 0, 8, 0, 0]);
        packet[6] = 0x20;

        let error = parse_ip_packet(&packet).unwrap_err();

        assert_eq!(error, PacketParseError::FragmentedIpv4);
    }

    #[test]
    fn parses_ipv6_tcp_packet() {
        let packet = ipv6_packet(6, &[0xc3, 0x50, 0x01, 0xbb, 0, 0, 0, 0]);

        let parsed = parse_ip_packet(&packet).unwrap();

        assert_eq!(parsed.source_addr, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(
            parsed.destination_addr,
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))
        );
        assert_eq!(parsed.protocol, Protocol::Tcp);
        assert_eq!(parsed.source_port, Some(50000));
        assert_eq!(parsed.destination_port, Some(443));
        assert_eq!(parsed.transport_offset, 40);
    }

    #[test]
    fn parses_ipv6_udp_after_hop_by_hop_extension() {
        let mut payload = vec![17, 0, 0, 0, 0, 0, 0, 0];
        payload.extend_from_slice(&[0xc3, 0x50, 0x00, 0x35, 0, 8, 0, 0]);
        let packet = ipv6_packet(0, &payload);

        let parsed = parse_ip_packet(&packet).unwrap();

        assert_eq!(parsed.protocol, Protocol::Udp);
        assert_eq!(parsed.source_port, Some(50000));
        assert_eq!(parsed.destination_port, Some(53));
        assert_eq!(parsed.transport_offset, 48);
    }

    #[test]
    fn rewrites_ipv4_tcp_source_and_clears_checksums() {
        let mut payload = vec![0_u8; 20];
        payload[0..2].copy_from_slice(&50000_u16.to_be_bytes());
        payload[2..4].copy_from_slice(&443_u16.to_be_bytes());
        payload[16..18].copy_from_slice(&0xaabb_u16.to_be_bytes());
        let mut packet = ipv4_packet(6, &payload);
        packet[10..12].copy_from_slice(&0x1234_u16.to_be_bytes());

        let parsed = super::rewrite_packet_source(
            &mut packet,
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)),
            Some(40000),
        )
        .unwrap();

        assert_eq!(parsed.source_addr, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)));
        assert_eq!(parsed.source_port, Some(40000));
        assert_eq!(&packet[12..16], &[198, 51, 100, 9]);
        assert_eq!(&packet[20..22], &40000_u16.to_be_bytes());
        assert_eq!(&packet[10..12], &[0, 0]);
        assert_eq!(&packet[36..38], &[0, 0]);
    }

    #[test]
    fn rewrites_ipv6_udp_destination_and_clears_transport_checksum() {
        let mut payload = vec![0_u8; 8];
        payload[0..2].copy_from_slice(&50000_u16.to_be_bytes());
        payload[2..4].copy_from_slice(&53_u16.to_be_bytes());
        payload[6..8].copy_from_slice(&0xaabb_u16.to_be_bytes());
        let mut packet = ipv6_packet(17, &payload);
        let new_addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2);

        let parsed =
            super::rewrite_packet_destination(&mut packet, IpAddr::V6(new_addr), Some(5353))
                .unwrap();

        assert_eq!(parsed.destination_addr, IpAddr::V6(new_addr));
        assert_eq!(parsed.destination_port, Some(5353));
        assert_eq!(&packet[24..40], &new_addr.octets());
        assert_eq!(&packet[42..44], &5353_u16.to_be_bytes());
        assert_eq!(&packet[46..48], &[0, 0]);
    }

    #[test]
    fn rejects_rewrite_address_family_mismatch() {
        let mut packet = ipv4_packet(17, &[0xc3, 0x50, 0x00, 0x35, 0, 8, 0, 0]);

        let error =
            super::rewrite_packet_source(&mut packet, IpAddr::V6(Ipv6Addr::LOCALHOST), None)
                .unwrap_err();

        assert!(matches!(error, super::PacketRewriteError::AddressFamilyMismatch { .. }));
    }
    #[test]
    fn rejects_too_short_transport_header() {
        let packet = ipv4_packet(6, &[0xc3, 0x50, 0x01]);

        let error = parse_ip_packet(&packet).unwrap_err();

        assert!(matches!(error, PacketParseError::TooShort { .. }));
    }

    fn ipv4_packet(protocol: u8, payload: &[u8]) -> Vec<u8> {
        let total_len = 20 + payload.len();
        let mut packet = vec![0_u8; 20];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
        packet[8] = 64;
        packet[9] = protocol;
        packet[12..16].copy_from_slice(&[192, 0, 2, 10]);
        packet[16..20].copy_from_slice(&[203, 0, 113, 20]);
        packet.extend_from_slice(payload);
        packet
    }

    fn ipv6_packet(next_header: u8, payload: &[u8]) -> Vec<u8> {
        let mut packet = vec![0_u8; 40];
        packet[0] = 0x60;
        packet[4..6].copy_from_slice(&(payload.len() as u16).to_be_bytes());
        packet[6] = next_header;
        packet[7] = 64;
        packet[8..24].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        packet[24..40].copy_from_slice(&Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).octets());
        packet.extend_from_slice(payload);
        packet
    }
}
