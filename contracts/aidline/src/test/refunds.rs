use super::*;

// ─── refund_for tests ────────────────────────────────────────────────────────

#[test]
fn refund_for_pays_donor_never_caller() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    let relayer = Address::generate(&s.env);

    s.client.donate(&donor, &id, &600);
    s.client.cancel_campaign(&s.creator, &id);

    // Initial relayer balance is 0
    assert_eq!(s.token.balance(&relayer), 0);
    assert_eq!(s.token.balance(&donor), 400);

    // Relayer triggers refund on behalf of donor without donor authorization
    let auth_relayer = MockAuth {
        address: &relayer,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "refund_for",
            args: (donor.clone(), id).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let refunded = s.client.mock_auths(&[auth_relayer]).refund_for(&donor, &id);
    assert_eq!(refunded, 600);

    // Tokens were paid to donor, NOT relayer
    assert_eq!(s.token.balance(&donor), 1000);
    assert_eq!(s.token.balance(&relayer), 0);
}

#[test]
fn refund_for_prevents_double_refund() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &500);
    s.client.cancel_campaign(&s.creator, &id);

    // First refund succeeds
    let refunded = s.client.refund_for(&donor, &id);
    assert_eq!(refunded, 500);

    // Second refund via refund_for fails with NothingToRefund
    let second_res = s.client.try_refund_for(&donor, &id);
    assert_eq!(second_res, Err(Ok(Error::NothingToRefund)));

    // Second refund via refund fails with NothingToRefund
    let direct_refund_res = s.client.try_refund(&donor, &id);
    assert_eq!(direct_refund_res, Err(Ok(Error::NothingToRefund)));
}

#[test]
fn refund_for_matches_refund_errors_on_non_refundable_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &300);

    // Campaign is active and on schedule: RefundNotAvailable
    let res_active = s.client.try_refund_for(&donor, &id);
    assert_eq!(res_active, Err(Ok(Error::RefundNotAvailable)));

    // Unknown campaign: CampaignNotFound
    let res_not_found = s.client.try_refund_for(&donor, &999_999);
    assert_eq!(res_not_found, Err(Ok(Error::CampaignNotFound)));

    // Cancel campaign, but test non-contributor
    s.client.cancel_campaign(&s.creator, &id);
    let non_contributor = Address::generate(&s.env);
    let res_nothing = s.client.try_refund_for(&non_contributor, &id);
    assert_eq!(res_nothing, Err(Ok(Error::NothingToRefund)));
}

#[test]
fn refund_for_requires_no_donor_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &400);
    s.client.cancel_campaign(&s.creator, &id);

    // Invocation with empty mock_auths (no auth whatsoever provided)
    let refunded = s.client.mock_auths(&[]).refund_for(&donor, &id);
    assert_eq!(refunded, 400);
    assert_eq!(s.token.balance(&donor), 1000);
}

#[test]
fn is_refundable_covers_all_statuses() {
    let s = Setup::new();
    let id = s.campaign();

    // 1. Active campaign within deadline is not refundable
    assert!(!s.client.is_refundable(&id));

    // 2. Cancelled campaign is refundable
    let cancel_id = s.campaign();
    s.client.cancel_campaign(&s.creator, &cancel_id);
    assert_eq!(
        s.client.get_campaign(&cancel_id).status,
        CampaignStatus::Cancelled
    );
    assert!(s.client.is_refundable(&cancel_id));

    // 3. Expired active campaign is refundable
    let expired_id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client.get_campaign(&expired_id).status,
        CampaignStatus::Active
    );
    assert!(s.client.is_refundable(&expired_id));

    // 4. Overdue milestone in active campaign is refundable
    let s2 = Setup::new();
    let now = s2.env.ledger().timestamp();
    let due_dates = vec![&s2.env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![&s2.env, 300, 700];
    let overdue_id = s2.client.create_campaign_with_due_dates(
        &s2.creator,
        &s2.beneficiary,
        &s2.verifier,
        &s2.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s2.env, "ipfs://overdue"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );
    assert!(!s2.client.is_refundable(&overdue_id));
    // Advance past milestone 0 due date but before campaign deadline
    s2.env.ledger().set_timestamp(now + 11 * DAY);
    assert!(s2.client.is_refundable(&overdue_id));

    // 5. Completed campaign is not refundable
    let s3 = Setup::new();
    let completed_id = s3.campaign();
    let donor = s3.donor(1000);
    s3.client.donate(&donor, &completed_id, &1000);
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 300
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 300
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 400
    assert_eq!(
        s3.client.get_campaign(&completed_id).status,
        CampaignStatus::Completed
    );
    assert!(!s3.client.is_refundable(&completed_id));

    // 6. Non-existent campaign is not refundable
    assert!(!s3.client.is_refundable(&999_999));
}

#[test]
fn only_creator_or_admin_can_cancel() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_cancel_campaign(&stranger, &id),
        Err(Ok(Error::Unauthorized))
    );
    s.client.cancel_campaign(&s.admin, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
}

#[test]
fn missing_campaign_errors() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_get_campaign(&42),
        Err(Ok(Error::CampaignNotFound))
    );
}
