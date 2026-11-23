use super::*;

// ─── Refund rollover (issue #121) ────────────────────────────────────────────

#[test]
fn rollover_moves_the_refund_into_another_campaign_without_moving_tokens() {
    let s = Setup::new();
    let from = s.campaign();
    let to = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &from, &500);
    s.client.cancel_campaign(&s.creator, &from);
    let contract_balance = s.token.balance(&s.client.address);

    assert_eq!(s.client.rollover_refund(&donor, &from, &to), 500);

    assert_eq!(s.token.balance(&donor), 0);
    assert_eq!(s.token.balance(&s.client.address), contract_balance);
    assert_eq!(s.client.contribution_of(&from, &donor), 0);
    assert_eq!(s.client.contribution_of(&to, &donor), 500);
    assert_eq!(s.client.get_campaign(&to).raised, 500);
}

#[test]
fn rollover_uses_the_same_rounding_as_refund() {
    let s = Setup::new();
    let from = s.campaign(); // Goal 1000
    let to = s.campaign();
    let donor = s.donor(1_000);
    s.client.donate(&donor, &from, &1_000);
    s.client.approve_milestone(&from, &s.proof()); // 300 released
    s.client.cancel_campaign(&s.creator, &from);

    // 1000 * 700 / 1000
    assert_eq!(s.client.rollover_refund(&donor, &from, &to), 700);
}

#[test]
fn donor_cannot_roll_over_twice_or_refund_after_rolling_over() {
    let s = Setup::new();
    let from = s.campaign();
    let to = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &from, &500);
    s.client.cancel_campaign(&s.creator, &from);
    s.client.rollover_refund(&donor, &from, &to);

    assert_eq!(
        s.client.try_rollover_refund(&donor, &from, &to),
        Err(Ok(Error::NothingToRefund))
    );
    assert_eq!(
        s.client.try_refund(&donor, &from),
        Err(Ok(Error::NothingToRefund))
    );
}

#[test]
fn rollover_rejects_same_campaign_and_inactive_target() {
    let s = Setup::new();
    let from = s.campaign();
    let to = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &from, &500);
    s.client.cancel_campaign(&s.creator, &from);

    assert_eq!(
        s.client.try_rollover_refund(&donor, &from, &from),
        Err(Ok(Error::SameCampaign))
    );

    s.client.cancel_campaign(&s.creator, &to);
    assert_eq!(
        s.client.try_rollover_refund(&donor, &from, &to),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn rollover_fails_when_it_would_pass_the_target_goal() {
    let s = Setup::new();
    let from = s.campaign();
    let to = s.campaign(); // Goal 1000
    let donor = s.donor(500);
    let other = s.donor(900);
    s.client.donate(&donor, &from, &500);
    s.client.donate(&other, &to, &900);
    s.client.cancel_campaign(&s.creator, &from);

    assert_eq!(
        s.client.try_rollover_refund(&donor, &from, &to),
        Err(Ok(Error::GoalExceeded))
    );
    // The donor can still take a normal refund.
    assert_eq!(s.client.refund(&donor, &from), 500);
}

#[test]
fn rollover_requires_a_refundable_source() {
    let s = Setup::new();
    let from = s.campaign();
    let to = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &from, &500);

    assert_eq!(
        s.client.try_rollover_refund(&donor, &from, &to),
        Err(Ok(Error::RefundNotAvailable))
    );
}
