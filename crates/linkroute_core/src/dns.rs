use std::collections::HashMap;
use std::net::IpAddr;

use crate::normalize::{normalize_domain, NormalizeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    pub domain: String,
    pub address: IpAddr,
    pub expires_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DnsCache {
    by_address: HashMap<IpAddr, Vec<DnsRecord>>,
}

impl DnsCache {
    pub fn observe_answer(
        &mut self,
        now_epoch_seconds: u64,
        queried_domain: &str,
        canonical_names: &[String],
        addresses: &[IpAddr],
        ttl_seconds: u32,
    ) -> Result<(), NormalizeError> {
        let mut domains = Vec::with_capacity(canonical_names.len() + 1);
        domains.push(normalize_domain(queried_domain)?);
        for canonical in canonical_names {
            let normalized = normalize_domain(canonical)?;
            if !domains.contains(&normalized) {
                domains.push(normalized);
            }
        }

        let ttl = u64::from(ttl_seconds.max(1));
        let expires_at = now_epoch_seconds.saturating_add(ttl);

        for address in addresses {
            let records = self.by_address.entry(*address).or_default();
            records.retain(|record| record.expires_at_epoch_seconds > now_epoch_seconds);

            for domain in &domains {
                if let Some(existing) = records.iter_mut().find(|record| record.domain == *domain) {
                    existing.expires_at_epoch_seconds = expires_at;
                } else {
                    records.push(DnsRecord {
                        domain: domain.clone(),
                        address: *address,
                        expires_at_epoch_seconds: expires_at,
                    });
                }
            }
        }

        Ok(())
    }

    pub fn domains_for_ip(&self, now_epoch_seconds: u64, address: IpAddr) -> Vec<String> {
        self.by_address
            .get(&address)
            .map(|records| {
                records
                    .iter()
                    .filter(|record| record.expires_at_epoch_seconds > now_epoch_seconds)
                    .map(|record| record.domain.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn best_domain_for_ip(&self, now_epoch_seconds: u64, address: IpAddr) -> Option<String> {
        self.domains_for_ip(now_epoch_seconds, address)
            .into_iter()
            .max_by_key(|domain| domain.len())
    }

    pub fn purge_expired(&mut self, now_epoch_seconds: u64) {
        self.by_address.retain(|_, records| {
            records.retain(|record| record.expires_at_epoch_seconds > now_epoch_seconds);
            !records.is_empty()
        });
    }

    pub fn len(&self) -> usize {
        self.by_address.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::DnsCache;

    #[test]
    fn observes_a_and_cname_records() {
        let mut cache = DnsCache::default();
        let address = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));

        cache
            .observe_answer(
                100,
                "Api.Example.Com.",
                &[String::from("edge.example.net")],
                &[address],
                60,
            )
            .unwrap();

        let domains = cache.domains_for_ip(120, address);
        assert!(domains.contains(&String::from("api.example.com")));
        assert!(domains.contains(&String::from("edge.example.net")));
    }

    #[test]
    fn expires_records_by_ttl() {
        let mut cache = DnsCache::default();
        let address = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));

        cache.observe_answer(100, "example.com", &[], &[address], 10).unwrap();

        assert_eq!(cache.best_domain_for_ip(109, address), Some(String::from("example.com")));
        assert_eq!(cache.best_domain_for_ip(110, address), None);
    }

    #[test]
    fn purge_removes_expired_records() {
        let mut cache = DnsCache::default();
        let address = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));

        cache.observe_answer(100, "example.com", &[], &[address], 1).unwrap();
        cache.purge_expired(101);

        assert!(cache.is_empty());
    }
}
