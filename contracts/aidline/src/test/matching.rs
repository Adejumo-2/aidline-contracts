use super::*;

// ─── Sponsor matching pools (issue #27) ───────────────────────────────────────

#[test]
fn pool_config_is_validated() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);

    for (ratio, cap, amount) in [
        (0, 500, 500),
        (10_001, 500, 500),
        (5_000, 0, 0),
        (5_000, 500, 400),
        (5_000, -5, -5),
    ] {
        assert_eq!(
            s.client
                .try_deposit_sponsor_pool(&sponsor, &id, &ratio, &cap, &amount),
            Err(Ok(Error::InvalidMatchingConfig)),
            "ratio {ratio}, cap {cap}, amount {amount}"
        );
    }
    assert_eq!(s.token.balance(&sponsor), 10_000);
}

#[test]
fn pool_needs_an_open_campaign() {
    let s = Setup::new();
    let sponsor = s.donor(10_000);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &9, &5_000, &500, &500),
        Err(Ok(Error::CampaignNotFound))
    );

    let id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn a_sponsor_can_only_open_one_pool_per_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::SponsorPoolExists))
    );
    assert_eq!(s.token.balance(&sponsor), 9_500);
}

#[test]
fn matching_credits_the_campaign_at_the_pool_ratio() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);

    s.client.donate(&donor, &id, &200);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &200), 100);

    assert_eq!(s.client.get_campaign(&id).raised, 300);
    let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
    assert_eq!(pool.matched, 100);
    assert_eq!(pool.remaining, 400);
}

#[test]
fn matching_stops_at_the_pool_cap() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &150, &150);

    s.client.donate(&donor, &id, &400);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 150);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 0);
}

#[test]
fn matching_never_pushes_a_campaign_past_its_goal() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(2_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000, &1_000);

    s.client.donate(&donor, &id, &900);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &900), 100);
    assert_eq!(s.client.get_campaign(&id).raised, 1_000);
}

#[test]
fn matching_a_tiny_donation_rounds_to_nothing() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(100);
    s.client.deposit_sponsor_pool(&sponsor, &id, &1, &500, &500);
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
    let sponsor = s.donor(10_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::PoolNotReturnable))
    );
}

#[test]
fn unmatched_pool_balance_returns_to_the_sponsor_after_the_deadline() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    s.client.donate(&donor, &id, &200);
    s.client.apply_matching(&sponsor, &id, &donor, &200);

    s.pass_deadline();
    assert_eq!(s.client.return_sponsor_pool(&sponsor, &id), 400);
    assert_eq!(s.token.balance(&sponsor), 9_900);
    assert!(s.client.get_sponsor_pool(&id, &sponsor).is_none());
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::SponsorPoolNotFound))
    );
}
