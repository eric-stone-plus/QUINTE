use quinte::model::{MULTIMODAL_MODEL, TEXT_MODEL};
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
            ("Party A", "codewhale"),
            ("Party B", "opencode"),
            ("Party C", "kilo"),
            ("Party D", "mimo"),
            ("Party E", "omp"),
        ]
    );
    assert!(policy.roster.iter().all(|route| route.required));
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
    assert!(error.contains("model routing is fixed"));

    let mut multimodal_drift = default_policy();
    multimodal_drift.multimodal_model = "mimo-v2.5-pro".into();
    let error = validate(&multimodal_drift).unwrap_err().to_string();
    assert!(error.contains("model routing is fixed"));
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
