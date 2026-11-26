use super::*;

// ─── Sponsor matching pools (issue #27) ───────────────────────────────────────

#[test]
fn pool_config_is_validated() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);

    for (ratio, cap, amount) in [
        (0, 1_000_000, 1_000_000),
        (10_001, 1_000_000, 1_000_000),
        (5_000, 0, 0),
        (5_000, 1_000_000, 999_999),
        (5_000, -5, -5),
        (5_000, 10, 10),
        (5_000, 999_999, 999_999),
    ] {
        assert_eq!(
            s.client
                .try_deposit_sponsor_pool(&sponsor, &id, &ratio, &cap, &amount),
            Err(Ok(Error::InvalidMatchingConfig)),
            "ratio {ratio}, cap {cap}, amount {amount}"
        );
    }
    assert_eq!(s.token.balance(&sponsor), 10_000_000);
}

#[test]
fn sponsor_pool_below_minimum_is_rejected_and_exact_minimum_succeeds() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(2_000_000);

    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &10, &10),
        Err(Ok(Error::InvalidMatchingConfig))
    );
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &999_999, &999_999),
        Err(Ok(Error::InvalidMatchingConfig))
    );

    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
    assert_eq!(pool.deposited, 1_000_000);
    assert_eq!(pool.remaining, 1_000_000);
    assert_eq!(s.token.balance(&sponsor), 1_000_000);
}

#[test]
fn pool_needs_an_open_campaign() {
    let s = Setup::new();
    let sponsor = s.donor(10_000_000);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &9, &5_000, &1_000_000, &1_000_000),
        Err(Ok(Error::CampaignNotFound))
    );

    let id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn a_sponsor_can_only_open_one_pool_per_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000),
        Err(Ok(Error::SponsorPoolExists))
    );
    assert_eq!(s.token.balance(&sponsor), 9_000_000);
}

#[test]
fn matching_credits_the_campaign_at_the_pool_ratio() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);

    s.client.donate(&donor, &id, &200);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &200), 100);

    assert_eq!(s.client.get_campaign(&id).raised, 300);
    let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
    assert_eq!(pool.matched, 100);
    assert_eq!(pool.remaining, 999_900);
}

#[test]
fn matching_ignores_donors_who_never_donated() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let stranger = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000_000, &1_000_000);

    assert_eq!(s.client.apply_matching(&sponsor, &id, &stranger, &400), 0);
    let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
    assert_eq!(pool.remaining, 1_000_000);
    assert_eq!(s.client.get_campaign(&id).raised, 0);
}

#[test]
fn matching_is_capped_to_the_actual_donation_and_cannot_repeat() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000_000, &1_000_000);

    s.client.donate(&donor, &id, &100);
    // Claims 400 but only 100 was donated.
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 100);
    // The same donation cannot be matched twice.
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 0);
    assert_eq!(
        s.client.get_sponsor_pool(&id, &sponsor).unwrap().matched,
        100
    );
}

#[test]
fn matching_stops_at_the_pool_cap() {
    let s = Setup::new();
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://cap-test"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 3_000_000],
    );
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(2_000_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000_000, &1_000_000);

    s.client.donate(&donor, &id, &1_500_000);
    assert_eq!(
        s.client.apply_matching(&sponsor, &id, &donor, &1_500_000),
        1_000_000
    );
    assert_eq!(
        s.client.apply_matching(&sponsor, &id, &donor, &1_500_000),
        0
    );
}

#[test]
fn matching_never_pushes_a_campaign_past_its_goal() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(2_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000_000, &1_000_000);

    s.client.donate(&donor, &id, &900);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &900), 100);
    assert_eq!(s.client.get_campaign(&id).raised, 1_000);
}

#[test]
fn matching_a_tiny_donation_rounds_to_nothing() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(100);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &1, &1_000_000, &1_000_000);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &1), 0);
}

#[test]
fn matching_validates_amount_and_pool() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(100);

    assert_eq!(
        s.client.try_apply_matching(&sponsor, &id, &donor, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_apply_matching(&sponsor, &id, &donor, &10),
        Err(Ok(Error::SponsorPoolNotFound))
    );
}

#[test]
fn pool_cannot_be_returned_while_the_campaign_is_open() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::PoolNotReturnable))
    );
}

#[test]
fn unmatched_pool_balance_returns_to_the_sponsor_after_the_deadline() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    s.client.donate(&donor, &id, &200);
    s.client.apply_matching(&sponsor, &id, &donor, &200);

    s.pass_deadline();
    assert_eq!(s.client.return_sponsor_pool(&sponsor, &id), 999_900);
    assert_eq!(s.token.balance(&sponsor), 9_999_900);
    assert!(s.client.get_sponsor_pool(&id, &sponsor).is_none());
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::SponsorPoolNotFound))
    );
}
