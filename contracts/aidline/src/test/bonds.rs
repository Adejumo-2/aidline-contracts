use super::*;

// ─── Verifier bonds (issue #29) ───────────────────────────────────────────────

#[test]
fn bond_below_requirement_is_rejected() {
    let s = Setup::new();
    s.client.set_bond_requirement(&500);
    let v = s.donor(1_000);
    assert_eq!(
        s.client.try_register_with_bond(&v, &499),
        Err(Ok(Error::BondRequired))
    );
    assert!(!s.client.is_verifier(&v));
    assert_eq!(s.token.balance(&v), 1_000);
}

#[test]
fn zero_bond_is_rejected_even_when_no_bond_is_required() {
    let s = Setup::new();
    let v = s.donor(1_000);
    assert_eq!(
        s.client.try_register_with_bond(&v, &0),
        Err(Ok(Error::BondRequired))
    );
}

#[test]
fn negative_bond_requirement_is_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_set_bond_requirement(&-1),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn posting_a_bond_registers_the_verifier_and_escrows_the_tokens() {
    let s = Setup::new();
    s.client.set_bond_requirement(&500);
    let v = s.bonded_verifier(1_000, 600);

    assert!(s.client.is_verifier(&v));
    assert_eq!(s.token.balance(&v), 400);
    assert_eq!(s.token.balance(&s.client.address), 600);

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.amount, 600);
    assert_eq!(bond.remaining, 600);
    assert_eq!(bond.status, BondStatus::Active);
}

#[test]
fn an_already_registered_verifier_cannot_post_a_bond() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_register_with_bond(&s.verifier, &100),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn only_the_verifier_or_admin_can_deregister() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client.try_deregister_verifier(&stranger, &v),
        Err(Ok(Error::Unauthorized))
    );
    s.client.deregister_verifier(&s.admin, &v);
    assert!(!s.client.is_verifier(&v));
}

#[test]
fn deregistering_an_unregistered_address_fails() {
    let s = Setup::new();
    let nobody = Address::generate(&s.env);
    assert_eq!(
        s.client.try_deregister_verifier(&nobody, &nobody),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn withdrawing_without_a_bond_fails() {
    let s = Setup::new();
    let nobody = Address::generate(&s.env);
    assert_eq!(
        s.client.try_withdraw_bond(&nobody),
        Err(Ok(Error::BondNotFound))
    );
}

#[test]
fn an_active_bond_cannot_be_withdrawn() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn admin_removal_moves_bond_to_pending_withdrawal() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);

    s.client.remove_verifier(&v);

    assert!(!s.client.is_verifier(&v));
    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.status, BondStatus::PendingWithdrawal);
    assert_eq!(bond.deregistered_at, s.env.ledger().timestamp());
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
    assert_eq!(s.token.balance(&v), 1_000);
}

#[test]
fn bond_withdrawal_waits_for_the_delay() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);
    s.client.deregister_verifier(&v, &v);

    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + 7 * DAY - 1);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    s.env.ledger().set_timestamp(now + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
    assert_eq!(s.token.balance(&v), 1_000);
    assert_eq!(s.token.balance(&s.client.address), 0);
    assert_eq!(
        s.client.get_verifier_bond(&v).unwrap().status,
        BondStatus::Withdrawn
    );
}

#[test]
fn bond_withdrawal_delay_saturates_when_timestamp_is_less_than_deregistered_at() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);

    let dereg_time = 100_000;
    s.env.ledger().set_timestamp(dereg_time);
    s.client.deregister_verifier(&v, &v);

    // Ledger timestamp equals deregistered_at: elapsed is 0, delay not met
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    // Ledger timestamp goes backward: saturating subtraction evaluates to 0 without underflow panic
    s.env.ledger().set_timestamp(dereg_time - 100);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    // When time advances past delay: succeeds
    s.env.ledger().set_timestamp(dereg_time + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
}

#[test]
fn a_bond_cannot_be_withdrawn_twice() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    s.client.deregister_verifier(&v, &v);
    s.client.withdraw_bond(&v);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn verifier_reregistration_preserves_historical_bond_record() {
    let s = Setup::new();
    let v = s.bonded_verifier(2_000, 500);

    // Initial bond is active
    let bond0 = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond0.amount, 500);
    assert_eq!(bond0.status, BondStatus::Active);

    // Cannot re-register while active
    assert_eq!(
        s.client.try_register_with_bond(&v, &600),
        Err(Ok(Error::Unauthorized))
    );

    // Deregister
    s.client.deregister_verifier(&v, &v);
    let bond_pending = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_pending.status, BondStatus::PendingWithdrawal);

    // Cannot re-register while pending withdrawal
    assert_eq!(
        s.client.try_register_with_bond(&v, &600),
        Err(Ok(Error::Unauthorized))
    );

    // Withdraw bond
    let withdrawn = s.client.withdraw_bond(&v);
    assert_eq!(withdrawn, 500);
    let bond_withdrawn = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_withdrawn.status, BondStatus::Withdrawn);
    assert_eq!(s.client.get_verifier_bond_nonce(&v), 0);

    // Re-register with new bond amount 600
    s.client.register_with_bond(&v, &600);

    // Archived prior bond preserved
    assert_eq!(s.client.get_verifier_bond_nonce(&v), 1);
    let historical = s.client.get_historical_verifier_bond(&v, &0).unwrap();
    assert_eq!(historical.amount, 500);
    assert_eq!(historical.remaining, 0);
    assert_eq!(historical.status, BondStatus::Withdrawn);

    // New active bond is in place
    let bond1 = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond1.amount, 600);
    assert_eq!(bond1.remaining, 600);
    assert_eq!(bond1.status, BondStatus::Active);
    assert!(s.client.is_verifier(&v));
}

#[test]
fn slashing_moves_the_bond_into_the_recovery_pool_for_donors() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    s.client.slash_verifier(&v, &id, &150);

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 250);
    assert_eq!(bond.status, BondStatus::Active);
    assert_eq!(s.client.get_campaign(&id).raised, 0);
    assert_eq!(s.client.slashed_recovery(&id), 150);
}

#[test]
fn slashed_bond_is_split_pro_rata_among_refunding_donors() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign(); // Goal 1000
    let donor_a = s.donor(600);
    let donor_b = s.donor(400);
    s.client.donate(&donor_a, &id, &600);
    s.client.donate(&donor_b, &id, &400);
    s.client.approve_milestone(&id, &s.proof()); // 300 released

    s.client.slash_verifier(&v, &id, &200);
    s.client.cancel_campaign(&s.creator, &id);

    // Pool = (1000 - 300) + 200 = 900, split 60/40.
    assert_eq!(s.client.refund(&donor_a, &id), 540);
    assert_eq!(s.client.refund(&donor_b, &id), 360);
    // Only the unslashed remainder of the bond is left in the contract.
    assert_eq!(s.token.balance(&s.client.address), 200);
}

#[test]
fn slashing_the_whole_bond_marks_it_slashed() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    s.client.slash_verifier(&v, &id, &400);
    assert_eq!(
        s.client.get_verifier_bond(&v).unwrap().status,
        BondStatus::Slashed
    );
}

#[test]
fn slash_validates_its_arguments() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    let nobody = Address::generate(&s.env);

    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_slash_verifier(&nobody, &id, &10),
        Err(Ok(Error::BondNotFound))
    );
    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &401),
        Err(Ok(Error::SlashExceedsBond))
    );
}

#[test]
fn slash_with_unknown_campaign_fails_and_leaves_bond_untouched() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);

    assert_eq!(
        s.client.try_slash_verifier(&v, &9_999, &150),
        Err(Ok(Error::CampaignNotFound))
    );

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 400);
    assert_eq!(bond.status, BondStatus::Active);
}

#[test]
fn a_withdrawn_bond_cannot_be_slashed() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    s.client.deregister_verifier(&v, &v);
    s.client.withdraw_bond(&v);
    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &10),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn slashing_completed_campaign_fails_and_does_not_deduct_bond() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    let donor = s.donor(1_000);

    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());

    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Completed);

    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &150),
        Err(Ok(Error::CampaignNotActive))
    );

    // Verify bond is not deducted
    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 400);

    // Slashing a cancelled campaign succeeds
    let id_cancelled = s.campaign();
    s.client.cancel_campaign(&s.creator, &id_cancelled);
    assert_eq!(
        s.client.get_campaign(&id_cancelled).status,
        CampaignStatus::Cancelled
    );
    s.client.slash_verifier(&v, &id_cancelled, &100);
    let bond_after = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_after.remaining, 300);
    assert_eq!(s.client.slashed_recovery(&id_cancelled), 100);
}
