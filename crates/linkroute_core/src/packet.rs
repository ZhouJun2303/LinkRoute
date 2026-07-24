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
