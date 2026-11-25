// ─── Issue #117: streaming release ───────────────────────────────────────────

use super::*;

/// Milestones 300, 300, 400 where only the first streams, over 10 days.
fn streamed(s: &Setup) -> u64 {
    s.client.create_campaign_with_streams(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://stream"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &vec![&s.env, 10, 0, 0],
    )
}

fn jump(s: &Setup, seconds: u64) {
    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + seconds);
}

#[test]
fn vested_amount_follows_time_and_can_be_claimed_repeatedly() {
    let s = Setup::new();
    let id = streamed(&s);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    s.client.approve_milestone(&id, &s.proof());
    // Nothing moves at approval.
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert_eq!(
        s.client.try_claim_streamed(&id),
        Err(Ok(Error::NothingToClaim))
    );

    jump(&s, 5 * DAY);
    assert_eq!(s.client.claim_streamed(&id), 150);
    assert_eq!(s.token.balance(&s.beneficiary), 150);

    // A quarter of the remaining half: 2.5 days is 75.
    jump(&s, DAY * 5 / 2);
    assert_eq!(s.client.claim_streamed(&id), 75);

    // Long after the end the total is capped at the milestone.
    jump(&s, 100 * DAY);
    assert_eq!(s.client.claim_streamed(&id), 75);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(
        s.client.try_claim_streamed(&id),
        Err(Ok(Error::NothingToClaim))
    );
}

#[test]
fn lump_sum_milestones_still_pay_immediately() {
    let s = Setup::new();
    let id = streamed(&s);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());

    // Milestone 1 has no stream days.
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
}

#[test]
fn cancel_keeps_the_vested_part_and_refunds_the_unvested_part() {
    let s = Setup::new();
    let id = streamed(&s);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());

    // 3 of 10 days: 90 of 300 has vested.
    jump(&s, 3 * DAY);
    s.client.cancel_campaign(&s.creator, &id);

    // Released shrinks from 300 to the vested 90, so 910 is refundable.
    assert_eq!(s.client.get_campaign(&id).released, 90);
    assert_eq!(s.client.refund(&donor, &id), 910);

    // The beneficiary can still claim what had vested, but nothing more,
    // however long they wait.
    jump(&s, 30 * DAY);
    assert_eq!(s.client.claim_streamed(&id), 90);
    assert_eq!(s.token.balance(&s.beneficiary), 90);
    assert_eq!(s.token.balance(&s.client.address), 0);
}

#[test]
fn an_expired_campaign_settles_its_streams_on_the_first_refund() {
    let s = Setup::new();
    let id = streamed(&s);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());

    jump(&s, 2 * DAY);
    s.pass_deadline();
    // Vested at the deadline check: 31 days is more than the stream, so all
    // 300 has vested and the refund pool is the other 700.
    assert_eq!(s.client.refund(&donor, &id), 700);
    assert_eq!(s.client.claim_streamed(&id), 300);
}

#[test]
fn stream_days_must_match_the_milestones() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_create_campaign_with_streams(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://bad"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
            &vec![&s.env, 10],
        ),
        Err(Ok(Error::InvalidStream))
    );
}
