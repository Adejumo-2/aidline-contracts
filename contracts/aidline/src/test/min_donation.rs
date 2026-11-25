use super::*;

// ─── Optional minimum donation ────────────────────────────────────────────────

#[test]
fn donations_below_minimum_fail() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor = s.donor(1000);

    let res = s.client.try_donate(&donor, &id, &50);
    assert_eq!(res, Err(Ok(Error::DonationBelowMinimum)));
}

#[test]
fn donations_at_or_above_minimum_succeed() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor = s.donor(1000);

    let accepted_exact = s.client.donate(&donor, &id, &100);
    assert_eq!(accepted_exact, 100);

    let accepted_above = s.client.donate(&donor, &id, &150);
    assert_eq!(accepted_above, 150);
    assert_eq!(s.client.get_campaign(&id).raised, 250);
}

#[test]
fn final_smaller_donation_reaching_goal_accepted() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 950 first (above minimum of 100).
    s.client.donate(&donor1, &id, &950);
    assert_eq!(s.client.get_campaign(&id).raised, 950);

    // Remaining needed is 50. Minimum donation is 100.
    // A donation of 50 reaches the goal exactly and must be accepted.
    let accepted = s.client.donate(&donor2, &id, &50);
    assert_eq!(accepted, 50);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
}

#[test]
fn final_smaller_donation_exceeding_remaining_goal_accepted() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 960 first.
    s.client.donate(&donor1, &id, &960);
    assert_eq!(s.client.get_campaign(&id).raised, 960);

    // Remaining is 40. Donor offers 60 (below minimum 100, but exceeds remaining).
    let accepted = s.client.donate(&donor2, &id, &60);
    assert_eq!(accepted, 40);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
}

#[test]
fn final_donation_below_minimum_that_does_not_reach_goal_fails() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 950 first. Remaining is 50.
    s.client.donate(&donor1, &id, &950);

    // Donor offers 30 (below minimum 100 and does NOT reach remaining 50).
    let res = s.client.try_donate(&donor2, &id, &30);
    assert_eq!(res, Err(Ok(Error::DonationBelowMinimum)));
}

#[test]
fn minimum_donation_of_zero_behaves_as_normal() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(0);
    let donor = s.donor(100);

    // Any positive donation is accepted when min_donation is 0.
    let accepted = s.client.donate(&donor, &id, &1);
    assert_eq!(accepted, 1);
}

#[test]
fn negative_min_donation_is_rejected() {
    let s = Setup::new();
    let res = s.client.try_create_campaign_min_donation(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://flood-relief"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &-1,
    );
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn min_donation_stored_in_campaign_record() {
    let s = Setup::new();

    // Default create_campaign has min_donation = 0.
    let id_default = s.campaign();
    assert_eq!(s.client.get_campaign(&id_default).min_donation, 0);

    // Configured min_donation is preserved.
    let id_custom = s.campaign_with_min_donation(250);
    assert_eq!(s.client.get_campaign(&id_custom).min_donation, 250);
}
