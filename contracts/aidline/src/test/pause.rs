use super::*;

// ─── Emergency pause switch tests ────────────────────────────────────────────

#[test]
fn admin_can_pause_and_unpause() {
    let s = Setup::new();
    assert!(!s.client.is_paused());

    s.client.pause();
    assert!(s.client.is_paused());

    s.client.unpause();
    assert!(!s.client.is_paused());
}

#[test]
fn pause_blocks_create_campaign_donate_and_approve_milestone() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    let sponsor = s.donor(2_000_000);

    // Initial donation works
    let accepted = s.client.donate(&donor, &id, &300);
    assert_eq!(accepted, 300);

    // Pause contract
    s.client.pause();

    // Create campaign blocked
    let res_create = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://new-campaign"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res_create, Err(Ok(Error::Paused)));

    // Create campaign with due dates blocked
    let res_create_dates = s.client.try_create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://new-dates"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100],
        &vec![&s.env, s.env.ledger().timestamp() + 10 * DAY],
    );
    assert_eq!(res_create_dates, Err(Ok(Error::Paused)));

    // Donate blocked
    let res_donate = s.client.try_donate(&donor, &id, &100);
    assert_eq!(res_donate, Err(Ok(Error::Paused)));

    // Approve milestone blocked
    let res_approve = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res_approve, Err(Ok(Error::Paused)));

    // Emergency fast track blocked
    let res_fast_track = s.client.try_emergency_fast_track(&id);
    assert_eq!(res_fast_track, Err(Ok(Error::Paused)));

    // Deposit sponsor pool blocked
    let res_sponsor_pool = s
        .client
        .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    assert_eq!(res_sponsor_pool, Err(Ok(Error::Paused)));

    // Unpause restores functionality
    s.client.unpause();

    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);

    let accepted2 = s.client.donate(&donor, &id, &100);
    assert_eq!(accepted2, 100);

    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
}

#[test]
fn non_admin_cannot_pause_or_unpause() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );

    let pause_auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "pause",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res_pause = s.client.mock_auths(&[pause_auth]).try_pause();
    assert_eq!(res_pause.unwrap_err().unwrap(), expected_auth_err);
    assert!(!s.client.is_paused());

    s.client.pause();
    assert!(s.client.is_paused());

    let unpause_auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "unpause",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res_unpause = s.client.mock_auths(&[unpause_auth]).try_unpause();
    assert_eq!(res_unpause.unwrap_err().unwrap(), expected_auth_err);
    assert!(s.client.is_paused());
}

#[test]
fn refunds_work_while_paused() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &300);

    s.client.cancel_campaign(&s.creator, &id);

    // Pause contract
    s.client.pause();

    // Refund succeeds while paused
    let refunded = s.client.refund(&donor, &id);
    assert_eq!(refunded, 300);
    assert_eq!(s.token.balance(&donor), 500);
}
