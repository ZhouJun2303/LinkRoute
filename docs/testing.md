# Testing Plan

## Automated

- Rule precedence and longest domain match.
- DNS A/AAAA/CNAME cache TTL handling.
- Flow table reverse lookup, idle purge, and managed-flow clearing.
- Physical adapter filtering and VPN/tunnel detection.
- Exact versus suffix domain behavior.
- Disabled rules and disabled global routing.
- Adapter unavailable fail-closed behavior.
- Swap behavior.
- Duplicate rule detection.
- ConfigStore missing-file defaulting, TOML roundtrip, invalid-file backup, and validation errors.
- Update manifest parsing and signature verification once crypto is added.

## Real Hardware

A self-hosted Windows 10/11 x64 machine with active Ethernet and Wi-Fi adapters
is required for release gates:

- IPv4 and IPv6 TCP routing.
- UDP and QUIC routing.
- DNS A, AAAA, and CNAME observation.
- Adapter unplug and reconnect.
- Sleep and resume.
- DHCP renewal.
- Windows Firewall and Microsoft Defender compatibility.
- VPN/tunnel detection.
- 24 hour stress run.

## Performance Gates

- Total idle memory: no more than 35 MB.
- Idle CPU: below 0.5%.
- 1 Gbps throughput: at least 90% of baseline.
- Added median latency: below 1 ms.




