use crate::config::RouteConfig;
use crate::model::{
    ApplicationRule, DecisionSource, DomainMatchMode, DomainRule, FlowDescriptor, LinkState,
    Outlet, RouteDecision,
};
use crate::normalize::{domain_matches, exe_paths_equal, normalize_domain};

#[derive(Debug, Clone)]
pub struct RouteEngine {
    config: RouteConfig,
}

impl RouteEngine {
    pub fn new(config: RouteConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RouteConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut RouteConfig {
        &mut self.config
    }

    pub fn decide(&self, flow: &FlowDescriptor, link_state: LinkState) -> RouteDecision {
        if !self.config.enabled {
            return RouteDecision::system_default(DecisionSource::Disabled);
        }
        if self.config.block_when_vpn_active && link_state.vpn_active {
            return RouteDecision::blocked(DecisionSource::VpnBlocked, None);
        }

        if let Some(rule) = self.best_domain_rule(flow.domain.as_deref()) {
            return self.decision_for_target(
                DecisionSource::DomainRule,
                rule.id.clone(),
                rule.target,
                link_state,
            );
        }

        if let Some(rule) = self.best_application_rule(flow.exe_path.as_deref()) {
            return self.decision_for_target(
                DecisionSource::ApplicationRule,
                rule.id.clone(),
                rule.target,
                link_state,
            );
        }

        RouteDecision::system_default(DecisionSource::SystemDefault)
    }

    fn decision_for_target(
        &self,
        source: DecisionSource,
        rule_id: String,
        outlet: Outlet,
        link_state: LinkState,
    ) -> RouteDecision {
        if self.config.fail_closed && !link_state.is_outlet_up(outlet) {
            return RouteDecision::blocked(DecisionSource::TargetUnavailable, Some(rule_id));
        }
        RouteDecision::outlet(source, rule_id, outlet)
    }

    fn best_domain_rule(&self, domain: Option<&str>) -> Option<&DomainRule> {
        let domain = normalize_domain(domain?).ok()?;

        self.config
            .domain_rules
            .iter()
            .filter(|rule| rule.enabled)
            .filter(|rule| {
                domain_matches(&rule.domain, &domain, matches!(rule.mode, DomainMatchMode::Suffix))
            })
            .max_by_key(|rule| domain_specificity(rule))
    }

    fn best_application_rule(&self, exe_path: Option<&str>) -> Option<&ApplicationRule> {
        let exe_path = exe_path?;
        self.config
            .application_rules
            .iter()
            .find(|rule| rule.enabled && exe_paths_equal(&rule.exe_path, exe_path))
    }
}

fn domain_specificity(rule: &DomainRule) -> (usize, usize) {
    let exact_bonus = match rule.mode {
        DomainMatchMode::Exact => 1,
        DomainMatchMode::Suffix => 0,
    };
    (rule.domain.len(), exact_bonus)
}
