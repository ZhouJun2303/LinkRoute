use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{ApplicationRule, DomainMatchMode, DomainRule};
use crate::normalize::exe_paths_equal;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteConfig {
    pub version: u32,
    pub enabled: bool,
    pub fail_closed: bool,
    pub block_when_vpn_active: bool,
    pub application_rules: Vec<ApplicationRule>,
    pub domain_rules: Vec<DomainRule>,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            fail_closed: true,
            block_when_vpn_active: true,
            application_rules: Vec::new(),
            domain_rules: Vec::new(),
        }
    }
}

impl RouteConfig {
    pub fn enabled() -> Self {
        Self { enabled: true, ..Self::default() }
    }

    pub fn swap_outlets(&mut self) {
        for rule in &mut self.application_rules {
            rule.target = rule.target.swapped();
        }
        for rule in &mut self.domain_rules {
            rule.target = rule.target.swapped();
        }
    }

    pub fn validate(&self) -> Vec<ConfigIssue> {
        let mut issues = Vec::new();
        let mut ids = HashSet::new();

        for rule in &self.application_rules {
            if !ids.insert(rule.id.clone()) {
                issues.push(ConfigIssue::DuplicateRuleId(rule.id.clone()));
            }
        }
        for rule in &self.domain_rules {
            if !ids.insert(rule.id.clone()) {
                issues.push(ConfigIssue::DuplicateRuleId(rule.id.clone()));
            }
        }

        for (index, left) in self.application_rules.iter().enumerate() {
            for right in self.application_rules.iter().skip(index + 1) {
                if exe_paths_equal(&left.exe_path, &right.exe_path) {
                    issues.push(ConfigIssue::DuplicateApplicationPath(left.exe_path.clone()));
                }
            }
        }

        for (index, left) in self.domain_rules.iter().enumerate() {
            for right in self.domain_rules.iter().skip(index + 1) {
                if left.domain == right.domain && left.mode == right.mode {
                    issues.push(ConfigIssue::DuplicateDomainRule {
                        domain: left.domain.clone(),
                        mode: left.mode,
                    });
                }
            }
        }

        issues
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigIssue {
    DuplicateRuleId(String),
    DuplicateApplicationPath(String),
    DuplicateDomainRule { domain: String, mode: DomainMatchMode },
}
