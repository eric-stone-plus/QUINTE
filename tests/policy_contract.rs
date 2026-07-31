use quinte::model::{MULTIMODAL_MODEL, Policy, RoutePolicy, TEXT_MODEL};
use quinte::policy::{default_policy, validate};

#[test]
fn default_policy_binds_the_fixed_roster_and_models() {
    let policy = default_policy();
    validate(&policy).unwrap();

    let parties = policy
        .roster
        .iter()
        .map(|route| (route.party_id.as_str(), route.route_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        parties,
        vec![
            ("Party A", "mimo-a"),
            ("Party B", "mimo-b"),
            ("Party C", "mimo-c"),
            ("Party D", "mimo-d"),
            ("Party E", "mimo-e"),
        ]
    );
    assert!(policy.roster.iter().all(|route| route.required));
    assert!(policy.roster.iter().all(|route| route.adapter == "mimo"));
    assert!(
        policy
            .roster
            .iter()
            .all(|route| !route.perspective.is_empty())
    );
    assert_eq!(policy.seat.family, "mimo");
    assert_eq!(policy.seat.provider, "xiaomi");
    assert_eq!(policy.counterpart_arbiter.party_id, "Counterpart Arbiter");
    assert!(policy.counterpart_arbiter.required);
    assert_eq!(policy.text_model, TEXT_MODEL);
    assert_eq!(policy.multimodal_model, MULTIMODAL_MODEL);
}

#[test]
fn policy_rejects_missing_or_extra_roster_member() {
    let mut missing = default_policy();
    missing.roster.pop();
    assert!(
        validate(&missing)
            .unwrap_err()
            .to_string()
            .contains("exactly five")
    );

    let mut extra = default_policy();
    extra.roster.push(extra.roster[0].clone());
    assert!(
        validate(&extra)
            .unwrap_err()
            .to_string()
            .contains("exactly five")
    );
}

#[test]
fn policy_rejects_reordered_optional_or_mislabeled_parties() {
    let mut reordered = default_policy();
    reordered.roster.swap(0, 1);
    assert!(validate(&reordered).is_err());

    let mut optional = default_policy();
    optional.roster[2].required = false;
    assert!(validate(&optional).is_err());

    let mut mislabeled = default_policy();
    mislabeled.roster[4].party_id = "Party F".into();
    assert!(validate(&mislabeled).is_err());
}

#[test]
fn policy_rejects_model_route_drift() {
    let mut text_drift = default_policy();
    text_drift.text_model = "mimo-v2.5".into();
    let error = validate(&text_drift).unwrap_err().to_string();
    assert!(error.contains("model aliases must match"));

    let mut multimodal_drift = default_policy();
    multimodal_drift.multimodal_model = "mimo-v2.5-pro".into();
    let error = validate(&multimodal_drift).unwrap_err().to_string();
    assert!(error.contains("model aliases must match"));
}

#[test]
fn policy_rejects_invalid_counterpart_arbiter_and_phase_limits() {
    let mut counterpart_arbiter = default_policy();
    counterpart_arbiter.counterpart_arbiter.required = false;
    assert!(validate(&counterpart_arbiter).is_err());

    let mut r1 = default_policy();
    r1.max_parallel_r1 = 4;
    assert!(validate(&r1).is_err());

    let mut r2 = default_policy();
    r2.max_parallel_r2 = 2;
    assert!(validate(&r2).is_err());

    let mut attempts = default_policy();
    attempts.max_attempts = 2;
    assert!(validate(&attempts).is_err());

    let mut no_pacing = default_policy();
    no_pacing.r2_min_interval_seconds = 0;
    assert!(validate(&no_pacing).is_err());

    let mut inverted_backoff = default_policy();
    inverted_backoff.retry_backoff_max_seconds =
        inverted_backoff.retry_backoff_seconds.saturating_sub(1);
    assert!(validate(&inverted_backoff).is_err());

    let mut pacing_drift = default_policy();
    pacing_drift.r2_min_interval_seconds = 11;
    assert!(validate(&pacing_drift).is_err());

    let mut output_limit = default_policy();
    output_limit.max_output_bytes = 1024;
    assert!(validate(&output_limit).is_err());
}

#[test]
fn policy_rejects_route_tuple_drift_and_path_unsafe_ids() {
    let mut unsupported = default_policy();
    unsupported.roster[0].adapter = "unknown".into();
    assert!(validate(&unsupported).is_err());

    let mut mixed = default_policy();
    mixed.roster[0].family = "deepseek".into();
    assert!(
        validate(&mixed)
            .unwrap_err()
            .to_string()
            .contains("single-family")
    );

    for route_id in ["../escape", "a/b", r"a\b", ".", "UPPER", "two words", ""] {
        let mut policy = default_policy();
        policy.roster[0].route_id = route_id.into();
        assert!(
            validate(&policy).is_err(),
            "accepted unsafe route_id {route_id:?}"
        );
    }

    let mut duplicate = default_policy();
    duplicate.counterpart_arbiter.route_id = duplicate.roster[0].route_id.clone();
    assert!(validate(&duplicate).is_err());

    let mut primary_duplicate = default_policy();
    primary_duplicate.primary_arbiter.route_id =
        primary_duplicate.counterpart_arbiter.route_id.clone();
    assert!(validate(&primary_duplicate).is_err());
}

fn routes_mut(policy: &mut Policy) -> Vec<&mut RoutePolicy> {
    policy
        .roster
        .iter_mut()
        .chain(std::iter::once(&mut policy.counterpart_arbiter))
        .chain(std::iter::once(&mut policy.primary_arbiter))
        .collect()
}

#[test]
fn every_role_must_match_all_four_single_family_binding_axes() {
    for route_index in 0..7 {
        for axis in ["family", "provider", "text_model", "multimodal_model"] {
            let mut policy = default_policy();
            let party = {
                let mut routes = routes_mut(&mut policy);
                let route = &mut routes[route_index];
                match axis {
                    "family" => route.family = "other-family".into(),
                    "provider" => route.provider = "other-provider".into(),
                    "text_model" => route.text_model = "other-text-model".into(),
                    "multimodal_model" => route.multimodal_model = "other-multimodal-model".into(),
                    _ => unreachable!(),
                }
                route.party_id.clone()
            };
            let error = validate(&policy).unwrap_err().to_string();
            assert!(
                error.contains("single-family seat invariant"),
                "accepted {axis} drift for {party}: {error}"
            );
        }
    }
}

#[test]
fn both_arbiters_are_required_and_have_fixed_identities() {
    for mutate in [
        |policy: &mut Policy| policy.counterpart_arbiter.required = false,
        |policy: &mut Policy| policy.primary_arbiter.required = false,
        |policy: &mut Policy| policy.counterpart_arbiter.party_id = "Other".into(),
        |policy: &mut Policy| policy.primary_arbiter.party_id = "Other".into(),
    ] {
        let mut policy = default_policy();
        mutate(&mut policy);
        assert!(validate(&policy).is_err());
    }
}

#[test]
fn binding_identifiers_reject_config_and_endpoint_injection_syntax() {
    for value in [
        "deepseek\nbase_url=evil",
        "provider/name",
        "provider:route",
        "quoted\"value",
        "模型",
        "",
    ] {
        let mut policy = default_policy();
        policy.seat.provider = value.into();
        for route in routes_mut(&mut policy) {
            route.provider = value.into();
        }
        assert!(
            validate(&policy).is_err(),
            "accepted unsafe provider {value:?}"
        );
    }
}

fn production_policy(family: &str, provider: &str, adapter: &str) -> Policy {
    let mut policy = default_policy();
    policy.seat.seat_id = format!("seat-{family}");
    policy.seat.family = family.into();
    policy.seat.provider = provider.into();
    policy.text_model = format!("{family}-text-model");
    policy.multimodal_model = format!("{family}-multimodal-model");
    policy.seat.text_model = policy.text_model.clone();
    policy.seat.multimodal_model = policy.multimodal_model.clone();
    for route in routes_mut(&mut policy) {
        route.family = family.into();
        route.provider = provider.into();
        route.text_model = format!("{family}-text-model");
        route.multimodal_model = format!("{family}-multimodal-model");
        route.adapter = adapter.into();
        route.executable = adapter.into();
    }
    policy
}

#[test]
fn spoofed_legacy_seat_id_cannot_bypass_the_production_capability_matrix() {
    let mut policy = default_policy();
    policy.seat.seat_id = "legacy-mimo".into();
    policy.seat.provider = "xiaomi-token-plan-cn".into();
    for route in routes_mut(&mut policy) {
        route.provider = "xiaomi-token-plan-cn".into();
        route.adapter = "omp".into();
        route.executable = "omp".into();
    }
    let error = validate(&policy).unwrap_err().to_string();
    assert!(
        error.contains("requires provider xiaomi") || error.contains("unsupported adapter"),
        "spoofed legacy seat was not rejected: {error}"
    );
}

#[test]
fn production_capability_matrix_requires_a_proven_isolated_adapter() {
    for (family, provider, adapter) in [
        ("mimo", "xiaomi", "mimo"),
        ("deepseek", "deepseek", "reasonix"),
        ("openai", "openai-api", "codex"),
    ] {
        validate(&production_policy(family, provider, adapter)).unwrap();
    }

    for (family, provider, wrong_adapter) in [
        ("mimo", "xiaomi", "reasonix"),
        ("mimo", "xiaomi", "omp"),
        ("deepseek", "deepseek", "mimo"),
        ("openai", "openai-api", "reasonix"),
    ] {
        let error = validate(&production_policy(family, provider, wrong_adapter))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("proven stateless binding") || error.contains("unsupported adapter"),
            "accepted {wrong_adapter} for {family}: {error}"
        );
    }
}
