use super::*;

// ─── Issue #26 Tests: Per-milestone due dates ────────────────────────────────

#[test]
fn due_date_refunds_do_not_cancel_campaign() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://due-dates"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(500);
    let donor_b = s.donor(500);

    s.client.donate(&donor_a, &id, &500);
    s.client.donate(&donor_b, &id, &500);

    s.client.approve_milestone(&id, &s.proof()); // 300 released

    // Advance past first milestone due date, into second milestone window.
    env.ledger().set_timestamp(now + 21 * DAY);

    // Milestone 1 is now overdue, refunds become available.
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 350); // 500 * 700 / 1000

    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Cancelled);
    assert_eq!(c.raised, 1000);
    assert_eq!(c.released, 300);

    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 350);

    let c2 = s.client.get_campaign(&id);
    assert_eq!(c2.status, CampaignStatus::Cancelled);
    assert_eq!(c2.raised, 1000);
    assert_eq!(c2.released, 300);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn overdue_refunds_path_independent_with_three_donors() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://asymm-refunds"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(200);
    let donor_b = s.donor(300);
    let donor_c = s.donor(500);

    s.client.donate(&donor_a, &id, &200);
    s.client.donate(&donor_b, &id, &300);
    s.client.donate(&donor_c, &id, &500);

    s.client.approve_milestone(&id, &s.proof()); // 300 released

    // Advance time past milestone 1 due date
    env.ledger().set_timestamp(now + 21 * DAY);

    // Donor C refunds first
    let refund_c = s.client.refund(&donor_c, &id);
    assert_eq!(refund_c, 350); // 500 * 700 / 1000

    // Campaign transitioned to Cancelled and totals remain immutable
    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Cancelled);
    assert_eq!(c.raised, 1000);
    assert_eq!(c.released, 300);

    // Donor A refunds second
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 140); // 200 * 700 / 1000

    // Donor B refunds third
    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 210); // 300 * 700 / 1000

    // Sum of all refunds equals raised - released
    assert_eq!(refund_a + refund_b + refund_c, 700);
    assert_eq!(refund_a + refund_b + refund_c + c.released, c.raised);

    // approve_milestone is rejected on cancelled campaign
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn overdue_milestone_blocks_new_donations_sponsor_pools_and_pledges() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://due-dates-overdue"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(500);
    let donor_b = s.donor(500);
    let sponsor = s.donor(1_000);
    let pledger = s.donor(500);

    // Initial donation succeeds before milestone is overdue
    s.client.donate(&donor_a, &id, &200);

    // Advance time past milestone 0 due date
    env.ledger().set_timestamp(now + 11 * DAY);

    // New donation fails with MilestoneOverdue
    assert_eq!(
        s.client.try_donate(&donor_b, &id, &200),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Deposit sponsor pool fails with MilestoneOverdue
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &1_000, &500, &500),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Create pledge fails with MilestoneOverdue
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&pledger, &s.client.address, &500, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&pledger, &id, &500),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Existing donor can still refund
    let refunded = s.client.refund(&donor_a, &id);
    assert_eq!(refunded, 200);
}

#[test]
fn create_campaign_with_due_dates_rejects_due_dates_exceeding_deadline() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let milestones = vec![&s.env, 300, 700];

    let count_before = s.client.campaign_count();

    // 1. Due date exceeding deadline (deadline = now + 10 * DAY, due_date = now + 15 * DAY) fails
    let late_dates = vec![&s.env, now + 5 * DAY, now + 15 * DAY];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://late-due"),
            &(now + 10 * DAY),
            &milestones,
            &late_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 2. Due date = deadline + 1 fails
    let deadline = now + 10 * DAY;
    let over_by_one = vec![&s.env, now + 5 * DAY, deadline + 1];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://over-by-one"),
            &deadline,
            &milestones,
            &over_by_one,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 3. Due date equal to deadline succeeds
    let exact_deadline_dates = vec![&s.env, now + 5 * DAY, deadline];
    let res = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://exact-deadline-due"),
        &deadline,
        &milestones,
        &exact_deadline_dates,
    );
    assert_eq!(s.client.campaign_count(), count_before + 1);
    assert_eq!(s.client.milestone_due_dates(&res), exact_deadline_dates);
}
