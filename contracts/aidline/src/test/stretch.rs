use super::*;

// ─── Stretch milestones (issue #120) ─────────────────────────────────────────

impl Setup<'_> {
    /// Campaign with base milestones 300, 300, 400 and stretch milestones 200, 300 (goal 1500).
    fn stretch_campaign(&self) -> u64 {
        self.client.create_campaign_with_stretch(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://stretch"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
            &vec![&self.env, 200, 300],
        )
    }
}

#[test]
fn campaign_without_stretch_behaves_like_a_plain_campaign() {
    let s = Setup::new();
    let id = s.client.create_campaign_with_stretch(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://plain"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &vec![&s.env],
    );
    assert_eq!(s.client.get_campaign(&id).goal, 1_000);
    assert_eq!(s.client.stretch_milestone_count(&id), 0);

    let donor = s.donor(2_000);
    s.client.donate(&donor, &id, &1_000);
    assert_eq!(
        s.client.try_donate(&donor, &id, &MIN_DONATION_AMOUNT),
        Err(Ok(Error::GoalExceeded))
    );
}

#[test]
fn donations_past_the_base_goal_are_accepted_up_to_the_stretch_total() {
    let s = Setup::new();
    let id = s.stretch_campaign();
    assert_eq!(s.client.stretch_milestone_count(&id), 2);
    assert_eq!(s.client.get_campaign(&id).goal, 1_500);

    let donor = s.donor(2_000);
    assert_eq!(s.client.donate(&donor, &id, &1_200), 1_200);
    // Only the 300 left under the stretch total is accepted.
    assert_eq!(s.client.donate(&donor, &id, &600), 300);
    assert_eq!(s.client.get_campaign(&id).raised, 1_500);
    assert_eq!(
        s.client.try_donate(&donor, &id, &MIN_DONATION_AMOUNT),
        Err(Ok(Error::GoalExceeded))
    );
}

#[test]
fn stretch_milestones_are_approved_in_order_after_the_base_ones() {
    let s = Setup::new();
    let id = s.stretch_campaign();
    let donor = s.donor(1_500);
    s.client.donate(&donor, &id, &1_500);

    for _ in 0..3 {
        s.client.approve_milestone(&id, &s.proof());
    }
    let c = s.client.get_campaign(&id);
    assert_eq!(c.milestones_released, 3);
    assert_eq!(c.status, CampaignStatus::Active);

    // First stretch milestone pays 200.
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 1_200);
}

#[test]
fn refund_uses_the_larger_stretch_pool() {
    let s = Setup::new();
    let id = s.stretch_campaign();
    let donor = s.donor(1_500);
    s.client.donate(&donor, &id, &1_500);
    s.client.approve_milestone(&id, &s.proof()); // 300 released

    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(s.client.refund(&donor, &id), 1_200);
}
