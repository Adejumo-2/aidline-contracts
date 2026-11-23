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
fn slashing_moves_the_bond_into_the_campaign_for_donors() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    s.client.slash_verifier(&v, &id, &150);

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 250);
    assert_eq!(bond.status, BondStatus::Active);
    assert_eq!(s.client.get_campaign(&id).raised, 150);
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
    assert_eq!(s.client.get_campaign(&id_cancelled).raised, 100);
}
