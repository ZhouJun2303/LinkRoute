use serde::{Deserialize, Serialize};

use crate::normalize::{normalize_domain, normalize_exe_path, NormalizeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outlet {
    Ethernet,
    Wifi,
}

impl Outlet {
    pub fn swapped(self) -> Self {
        match self {
            Self::Ethernet => Self::Wifi,
            Self::Wifi => Self::Ethernet,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ethernet => "ethernet",
            Self::Wifi => "wifi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DomainMatchMode {
    Exact,
    Suffix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRule {
    pub id: String,
    pub name: String,
    pub exe_path: String,
    pub target: Outlet,
    pub enabled: bool,
}

impl ApplicationRule {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        exe_path: impl AsRef<str>,
        target: Outlet,
    ) -> Result<Self, NormalizeError> {
        Ok(Self {
            id: id.into(),
            name: name.into(),
            exe_path: normalize_exe_path(exe_path.as_ref())?,
            target,
            enabled: true,
        })
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainRule {
    pub id: String,
    pub domain: String,
    pub mode: DomainMatchMode,
    pub target: Outlet,
    pub enabled: bool,
}

impl DomainRule {
    pub fn new(
        id: impl Into<String>,
        domain: impl AsRef<str>,
        mode: DomainMatchMode,
        target: Outlet,
    ) -> Result<Self, NormalizeError> {
        Ok(Self {
            id: id.into(),
            domain: normalize_domain(domain.as_ref())?,
            mode,
            target,
            enabled: true,
        })
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowDescriptor {
    pub protocol: Protocol,
    pub exe_path: Option<String>,
    pub domain: Option<String>,
    pub remote_addr: Option<String>,
}

impl FlowDescriptor {
    pub fn tcp() -> Self {
        Self { protocol: Protocol::Tcp, exe_path: None, domain: None, remote_addr: None }
    }

    pub fn with_exe_path(mut self, exe_path: impl Into<String>) -> Self {
        self.exe_path = Some(exe_path.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkState {
    pub ethernet_up: bool,
    pub wifi_up: bool,
    pub vpn_active: bool,
}

impl LinkState {
    pub fn all_up() -> Self {
        Self { ethernet_up: true, wifi_up: true, vpn_active: false }
    }

    pub fn is_outlet_up(self, outlet: Outlet) -> bool {
        match outlet {
            Outlet::Ethernet => self.ethernet_up,
            Outlet::Wifi => self.wifi_up,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAction {
    UseOutlet(Outlet),
    UseSystemDefault,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionSource {
    Disabled,
    DomainRule,
    ApplicationRule,
    SystemDefault,
    VpnBlocked,
    TargetUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub action: RouteAction,
    pub source: DecisionSource,
    pub matched_rule_id: Option<String>,
}

impl RouteDecision {
    pub fn system_default(source: DecisionSource) -> Self {
        Self { action: RouteAction::UseSystemDefault, source, matched_rule_id: None }
    }

    pub fn outlet(source: DecisionSource, rule_id: impl Into<String>, outlet: Outlet) -> Self {
        Self {
            action: RouteAction::UseOutlet(outlet),
            source,
            matched_rule_id: Some(rule_id.into()),
        }
    }

    pub fn blocked(source: DecisionSource, rule_id: Option<String>) -> Self {
        Self { action: RouteAction::Block, source, matched_rule_id: rule_id }
    }
}
