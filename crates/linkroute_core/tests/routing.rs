use linkroute_core::{
    ApplicationRule, ConfigIssue, DomainMatchMode, DomainRule, FlowDescriptor, LinkState, Outlet,
    RouteAction, RouteConfig, RouteEngine,
};

#[test]
fn domain_rule_overrides_application_rule() {
    let mut config = RouteConfig::enabled();
    config.application_rules.push(
        ApplicationRule::new("app-browser", "Browser", r"C:\Apps\browser.exe", Outlet::Wifi)
            .unwrap(),
    );
    config.domain_rules.push(
        DomainRule::new("domain-api", "api.example.com", DomainMatchMode::Exact, Outlet::Ethernet)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow =
        FlowDescriptor::tcp().with_exe_path(r"c:\apps\BROWSER.exe").with_domain("api.example.com");

    assert_eq!(
        engine.decide(&flow, LinkState::all_up()).action,
        RouteAction::UseOutlet(Outlet::Ethernet)
    );
}

#[test]
fn longest_suffix_domain_wins() {
    let mut config = RouteConfig::enabled();
    config.domain_rules.push(
        DomainRule::new("domain-example", "example.com", DomainMatchMode::Suffix, Outlet::Wifi)
            .unwrap(),
    );
    config.domain_rules.push(
        DomainRule::new("domain-api", "api.example.com", DomainMatchMode::Suffix, Outlet::Ethernet)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_domain("v1.api.example.com");

    assert_eq!(
        engine.decide(&flow, LinkState::all_up()).action,
        RouteAction::UseOutlet(Outlet::Ethernet)
    );
}

#[test]
fn exact_domain_does_not_match_subdomain() {
    let mut config = RouteConfig::enabled();
    config.domain_rules.push(
        DomainRule::new("domain-api", "api.example.com", DomainMatchMode::Exact, Outlet::Ethernet)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_domain("v1.api.example.com");

    assert_eq!(engine.decide(&flow, LinkState::all_up()).action, RouteAction::UseSystemDefault);
}

#[test]
fn suffix_domain_matches_subdomain() {
    let mut config = RouteConfig::enabled();
    config.domain_rules.push(
        DomainRule::new("domain-api", "api.example.com", DomainMatchMode::Suffix, Outlet::Wifi)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_domain("v1.api.example.com");

    assert_eq!(
        engine.decide(&flow, LinkState::all_up()).action,
        RouteAction::UseOutlet(Outlet::Wifi)
    );
}

#[test]
fn disabled_config_uses_system_default() {
    let mut config = RouteConfig::default();
    config.application_rules.push(
        ApplicationRule::new("app-browser", "Browser", r"C:\Apps\browser.exe", Outlet::Wifi)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_exe_path(r"C:\Apps\browser.exe");

    assert_eq!(engine.decide(&flow, LinkState::all_up()).action, RouteAction::UseSystemDefault);
}

#[test]
fn matched_rule_blocks_when_target_is_down() {
    let mut config = RouteConfig::enabled();
    config.application_rules.push(
        ApplicationRule::new("app-browser", "Browser", r"C:\Apps\browser.exe", Outlet::Ethernet)
            .unwrap(),
    );

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_exe_path(r"C:\Apps\browser.exe");
    let link_state = LinkState { ethernet_up: false, wifi_up: true, vpn_active: false };

    assert_eq!(engine.decide(&flow, link_state).action, RouteAction::Block);
}

#[test]
fn unmatched_flow_is_not_blocked_by_down_adapter() {
    let config = RouteConfig::enabled();
    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_exe_path(r"C:\Apps\other.exe");
    let link_state = LinkState { ethernet_up: false, wifi_up: true, vpn_active: false };

    assert_eq!(engine.decide(&flow, link_state).action, RouteAction::UseSystemDefault);
}

#[test]
fn swap_outlets_changes_rule_targets() {
    let mut config = RouteConfig::enabled();
    config.application_rules.push(
        ApplicationRule::new("app-browser", "Browser", r"C:\Apps\browser.exe", Outlet::Ethernet)
            .unwrap(),
    );
    config.swap_outlets();

    let engine = RouteEngine::new(config);
    let flow = FlowDescriptor::tcp().with_exe_path(r"C:\Apps\browser.exe");

    assert_eq!(
        engine.decide(&flow, LinkState::all_up()).action,
        RouteAction::UseOutlet(Outlet::Wifi)
    );
}

#[test]
fn duplicate_rules_are_reported() {
    let mut config = RouteConfig::enabled();
    config.application_rules.push(
        ApplicationRule::new("same", "Browser", r"C:\Apps\browser.exe", Outlet::Ethernet).unwrap(),
    );
    config.application_rules.push(
        ApplicationRule::new("same", "Browser 2", r"C:\Apps\BROWSER.exe", Outlet::Wifi).unwrap(),
    );

    let issues = config.validate();
    assert!(issues.contains(&ConfigIssue::DuplicateRuleId("same".to_string())));
    assert!(issues.iter().any(|issue| matches!(issue, ConfigIssue::DuplicateApplicationPath(_))));
}
