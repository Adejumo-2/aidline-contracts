// ─── Issue #114: donor vote to cancel ────────────────────────────────────────

use super::*;
use crate::STALL_PERIOD;

/// A campaign of 1000 funded 600 by `a` and 400 by `b`.
fn funded(s: &Setup) -> (u64, Address, Address) {
    let id = s.campaign();
    let (a, b) = (s.donor(600), s.donor(400));
    s.client.donate(&a, &id, &600);
    s.client.donate(&b, &id, &400);
    (id, a, b)
}

fn jump(s: &Setup, seconds: u64) {
    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + seconds);
}

#[test]
fn votes_before_the_stall_period_are_recorded_and_cancel_later() {
    let s = Setup::new();
    let (id, a, _b) = funded(&s);

    // 600 of 1000 is over the threshold, but the campaign is not stalled yet.
    assert!(!s.client.vote_cancel(&a, &id));
    assert_eq!(s.client.cancel_vote_weight(&id), 600);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);
    assert_eq!(
        s.client.try_execute_vote_cancel(&id),
        Err(Ok(Error::NotStalled))
    );

    jump(&s, STALL_PERIOD);
    s.client.execute_vote_cancel(&id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
    assert_eq!(s.client.refund(&a, &id), 600);
}

#[test]
fn a_vote_cast_after_the_stall_period_cancels_immediately() {
    let s = Setup::new();
    let (id, a, _b) = funded(&s);
    jump(&s, STALL_PERIOD);

    assert!(s.client.vote_cancel(&a, &id));
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
}

#[test]
fn votes_are_weighted_by_contribution_and_counted_once() {
    let s = Setup::new();
    let (id, a, b) = funded(&s);
    jump(&s, STALL_PERIOD);

    // 400 of 1000 is below the 50% threshold.
    assert!(!s.client.vote_cancel(&b, &id));
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);
    assert_eq!(
        s.client.try_vote_cancel(&b, &id),
        Err(Ok(Error::AlreadyVoted))
    );
    assert_eq!(s.client.cancel_vote_weight(&id), 400);

    let outsider = s.donor(10);
    assert_eq!(
        s.client.try_vote_cancel(&outsider, &id),
        Err(Ok(Error::NothingToRefund))
    );

    // The second donor pushes it over.
    assert!(s.client.vote_cancel(&a, &id));
}

#[test]
fn a_vote_can_be_withdrawn_and_cast_again() {
    let s = Setup::new();
    let (id, a, _b) = funded(&s);

    s.client.vote_cancel(&a, &id);
    s.client.withdraw_cancel_vote(&a, &id);
    assert_eq!(s.client.cancel_vote_weight(&id), 0);
    assert_eq!(
        s.client.try_withdraw_cancel_vote(&a, &id),
        Err(Ok(Error::NoVote))
    );

    jump(&s, STALL_PERIOD);
    assert_eq!(
        s.client.try_execute_vote_cancel(&id),
        Err(Ok(Error::VoteThresholdNotReached))
    );
    assert!(s.client.vote_cancel(&a, &id));
}

#[test]
fn an_approved_milestone_restarts_the_stall_period() {
    let s = Setup::new();
    let (id, a, _b) = funded(&s);
    jump(&s, STALL_PERIOD - 1);
    s.client.approve_milestone(&id, &s.proof());

    jump(&s, 2);
    // Past the original deadline, but the approval moved the clock.
    assert!(!s.client.vote_cancel(&a, &id));
    assert_eq!(
        s.client.try_execute_vote_cancel(&id),
        Err(Ok(Error::NotStalled))
    );

    jump(&s, STALL_PERIOD);
    s.client.execute_vote_cancel(&id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
}

#[test]
fn cancelling_by_vote_releases_the_verifiers_bond_lock() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &v,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://vote"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
    );
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.deregister_verifier(&v, &v);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::VerifierHasActiveCampaigns))
    );

    jump(&s, STALL_PERIOD);
    assert!(s.client.vote_cancel(&donor, &id));
    assert_eq!(s.client.withdraw_bond(&v), 400);
}
