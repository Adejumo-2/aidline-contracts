use super::*;

#[test]
fn creates_campaign_with_goal_from_milestones() {
    let s = Setup::new();
    let id = s.campaign();

    let c = s.client.get_campaign(&id);
    assert_eq!(id, 0);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(c.kind, CampaignKind::Emergency);
    assert_eq!(s.client.campaign_count(), 1);
}

#[test]
fn rejects_unknown_verifier() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &stranger,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://trees"),
        &(s.env.ledger().timestamp() + DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::NotVerifier)));
}

#[test]
fn rejects_conflict_of_interest() {
    let s = Setup::new();
    let future = s.env.ledger().timestamp() + DAY;
    let uri = String::from_str(&s.env, "ipfs://trees");

    // Verifier is also creator
    let res_creator = s.client.try_create_campaign(
        &s.verifier,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_creator, Err(Ok(Error::ConflictOfInterest)));

    // Verifier is also beneficiary
    let res_beneficiary = s.client.try_create_campaign(
        &s.creator,
        &s.verifier,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_beneficiary, Err(Ok(Error::ConflictOfInterest)));
}

#[test]
fn rejects_bad_milestones_and_deadline() {
    let s = Setup::new();
    let uri = String::from_str(&s.env, "ipfs://x");
    let future = s.env.ledger().timestamp() + DAY;

    let empty = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env],
    );
    assert_eq!(empty, Err(Ok(Error::InvalidMilestones)));

    let negative = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100, -5],
    );
    assert_eq!(negative, Err(Ok(Error::InvalidMilestones)));

    let past = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &s.env.ledger().timestamp(),
        &vec![&s.env, 100],
    );
    assert_eq!(past, Err(Ok(Error::DeadlineInPast)));
}

#[test]
fn rejects_long_metadata_uri() {
    let s = Setup::new();
    let future = s.env.ledger().timestamp() + DAY;

    // 257 bytes should fail with UriTooLong
    let long_str = "a".repeat(257);
    let long_uri = String::from_str(&s.env, &long_str);
    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &long_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::UriTooLong)));

    // Exact limit 256 bytes should succeed
    let max_str = "a".repeat(256);
    let max_uri = String::from_str(&s.env, &max_str);
    let ok_res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &max_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert!(ok_res.is_ok());
}

#[test]
fn rejects_long_proof_uri() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &300);

    // 257 bytes should fail with UriTooLong
    let long_str = "p".repeat(257);
    let long_proof = String::from_str(&s.env, &long_str);
    let res = s.client.try_approve_milestone(&id, &long_proof);
    assert_eq!(res, Err(Ok(Error::UriTooLong)));

    // Exact limit 256 bytes should succeed
    let max_str = "p".repeat(256);
    let max_proof = String::from_str(&s.env, &max_str);
    let ok_res = s.client.try_approve_milestone(&id, &max_proof);
    assert_eq!(ok_res, Ok(Ok(300)));
}

#[test]
fn donations_are_held_in_escrow() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);

    s.client.donate(&donor, &id, &200);
    s.client.donate(&donor, &id, &100);

    assert_eq!(s.token.balance(&donor), 200);
    assert_eq!(s.token.balance(&s.client.address), 300);
    assert_eq!(s.client.contribution_of(&id, &donor), 300);
    assert_eq!(s.client.get_campaign(&id).raised, 300);
}

#[test]
fn donation_accepts_partial_amount_when_passing_goal() {
    let s = Setup::new();
    let id = s.campaign(); // goal 1000
    let donor = s.donor(2000);

    // Donating 1500 to a 1000-goal campaign accepts 1000, transfers only 1000, returns 1000
    let accepted = s.client.donate(&donor, &id, &1500);
    assert_eq!(accepted, 1000);
    assert_eq!(s.token.balance(&donor), 1000);
    assert_eq!(s.token.balance(&s.client.address), 1000);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
    assert_eq!(s.client.contribution_of(&id, &donor), 1000);

    // Fully funded campaign now rejects subsequent donations
    assert_eq!(
        s.client.try_donate(&donor, &id, &10),
        Err(Ok(Error::GoalExceeded))
    );
    assert_eq!(
        s.client.try_donate(&donor, &id, &0),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn donation_partial_acceptance_with_existing_raised() {
    let s = Setup::new();
    let id = s.campaign(); // goal 1000
    let donor_a = s.donor(1000);
    let donor_b = s.donor(1000);

    // Donor A donates 700 (exact)
    let accepted_a = s.client.donate(&donor_a, &id, &700);
    assert_eq!(accepted_a, 700);
    assert_eq!(s.token.balance(&donor_a), 300);

    // Donor B offers 500 when only 300 is needed
    let accepted_b = s.client.donate(&donor_b, &id, &500);
    assert_eq!(accepted_b, 300);
    assert_eq!(s.token.balance(&donor_b), 700); // only 300 deducted
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
    assert_eq!(s.client.contribution_of(&id, &donor_b), 300);

    // Now campaign is fully funded, rejects any further donations
    let donor_c = s.donor(100);
    assert_eq!(
        s.client.try_donate(&donor_c, &id, &50),
        Err(Ok(Error::GoalExceeded))
    );
}

#[test]
fn milestones_release_in_order_and_complete() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 400);

    let c = s.client.get_campaign(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 1000);
    assert_eq!(c.released, 1000);
    assert_eq!(c.status, CampaignStatus::Completed);
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn milestone_needs_enough_funds() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &299);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::MilestoneNotFunded))
    );
}

#[test]
fn removed_verifier_cannot_approve() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);
    s.client.remove_verifier(&s.verifier);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn cancelled_campaign_refunds_unreleased_pro_rata() {
    let s = Setup::new();
    let id = s.campaign();
    let alice = s.donor(1000);
    let bob = s.donor(1000);
    s.client.donate(&alice, &id, &600);
    s.client.donate(&bob, &id, &400);
    s.client.approve_milestone(&id, &s.proof()); // 300 released, 700 left

    s.client.cancel_campaign(&s.creator, &id);

    assert_eq!(s.client.refund(&alice, &id), 420);
    assert_eq!(s.client.refund(&bob, &id), 280);
    assert_eq!(s.token.balance(&s.client.address), 0);
    assert_eq!(
        s.client.try_refund(&alice, &id),
        Err(Ok(Error::NothingToRefund))
    );
}

#[test]
fn expired_campaign_allows_refund_and_blocks_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    assert_eq!(
        s.client.try_refund(&donor, &id),
        Err(Ok(Error::RefundNotAvailable))
    );

    s.pass_deadline();
    assert_eq!(
        s.client.try_donate(&donor, &id, &10),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(s.client.refund(&donor, &id), 500);
    assert_eq!(s.token.balance(&donor), 1000);
}

#[test]
fn refund_pro_rata_overflow_returns_invalid_amount() {
    let s = Setup::new();
    let half_max = i128::MAX / 2;
    let milestones = vec![&s.env, half_max, half_max];
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://overflow-refund"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &milestones,
    );

    let donor1 = s.donor(half_max);
    let donor2 = s.donor(1);

    s.client.donate(&donor1, &id, &half_max);
    s.client.donate(&donor2, &id, &1);

    s.client.cancel_campaign(&s.creator, &id);

    // donor1: contributed = half_max, unreleased = half_max + 1.
    // contributed * unreleased overflows i128::MAX -> returns Err(Ok(Error::InvalidAmount))
    assert_eq!(
        s.client.try_refund(&donor1, &id),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_refund_for(&donor1, &id),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(s.client.refundable_amount(&id, &donor1), 0);

    // donor2: contributed = 1, unreleased = half_max + 1.
    // 1 * (half_max + 1) does not overflow -> succeeds
    let refunded2 = s.client.refund(&donor2, &id);
    assert_eq!(refunded2, 1);
}
