# Data Plane Notes

## Current Packet Path

`ServiceController::process_packet` is the current service-side packet processing contract.

Outbound flow:

1. Parse the raw IPv4/IPv6 packet with `linkroute_core::parse_ip_packet`.
2. Convert the packet to an outbound `FlowKey`.
3. Resolve a domain from the DNS cache when one was not supplied by process correlation.
4. Run the rule engine.
5. For `UseSystemDefault`, forward unchanged.
6. For `Block`, drop.
7. For `UseOutlet`, choose an Ethernet/Wi-Fi source address with the same IP family as the remote endpoint, rewrite the packet source endpoint, and store the original-to-translated NAT mapping.

Inbound flow:

1. Parse the packet and convert it to an inbound `FlowKey`.
2. Look up that translated key in `NatTable`.
3. If no mapping exists, forward unchanged.
4. If a mapping exists, restore the original destination endpoint and update flow/NAT timestamps.

The packet rewrite helpers clear affected checksum fields. The WinDivert runtime must call `WinDivertHelperCalcChecksums` before reinjecting rewritten packets.

## Adapter Source Selection

The service loads adapters through Windows `GetAdaptersAddresses`. The inventory records adapter ID, friendly name, kind, interface index, link state, physical/virtual classification, and unicast addresses.

Only active physical Ethernet/Wi-Fi adapters are outlet candidates. Virtual adapters such as Hyper-V, WSL, Docker, VMware, VirtualBox, Npcap, loopback, and tunnel/VPN adapters are excluded from outlet selection. VPN/tunnel adapters are still reported so `block_when_vpn_active` can fail closed.

`AdapterInventory::source_address_for` selects a source address matching the packet remote address family. If no source address is available for a matched rule, the packet is dropped to avoid silently falling back to the wrong network.

## WinDivert Packet Pump

`linkroute_service::windivert::run_packet_pump` is the tested loop boundary. It receives packet bytes and `WINDIVERT_ADDRESS` metadata, calls a `PacketHandler`, drops packets with `PacketDisposition::Drop`, recalculates checksums for rewritten packets, and sends complete packets back through WinDivert.

`run_network_packet_pump` opens the WinDivert Network layer with `LINKROUTE_NETWORK_FILTER` and delegates to the generic pump. `ControllerPacketHandler` adapts `ServiceController::process_packet` to the pump by mapping forwarded, dropped, and rewritten route outcomes to packet dispositions.

The remaining service lifecycle work is to run this pump from the SCM-hosted service, stop it cleanly on shutdown/pause/swap/uninstall, and maintain Flow/Socket layer correlation tables so outbound packets receive PID and executable path before rule evaluation.

## Verification

Automated tests cover NAT table indexing, outbound source rewrites, inbound destination restoration, missing-source-address drops, adapter source selection, virtual adapter filtering, packet parser/rewrite edge cases, and packet pump forwarding/drop/checksum behavior.

Manual status check:

```powershell
cargo run -p linkroute_service -- --status-json
```

The JSON should include physical outlet candidates for active Ethernet/Wi-Fi adapters and should not mark `vEthernet`, WSL, Docker, or loopback adapters as outlet candidates.