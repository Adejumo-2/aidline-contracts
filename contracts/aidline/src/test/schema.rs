// ─── Issue #118: schema versioning ───────────────────────────────────────────

use super::*;
use crate::SCHEMA_VERSION;
use crate::types::DataKey;

/// Rewrites the stored state as an older build (before versioning) left it.
fn downgrade_to_unversioned(s: &Setup) {
    s.env.as_contract(&s.client.address, || {
        s.env.storage().instance().remove(&DataKey::SchemaVersion);
    });
}

#[test]
fn fresh_deploys_start_at_the_current_version() {
    let s = Setup::new();
    assert_eq!(s.client.schema_version(), SCHEMA_VERSION);
    let id = s.campaign();
    let donor = s.donor(100);
    s.client.donate(&donor, &id, &100);
}

#[test]
fn calls_fail_while_a_migration_is_pending_and_migrate_fixes_it() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &100);

    downgrade_to_unversioned(&s);
    assert_eq!(s.client.schema_version(), 0);

    // State-changing entry points refuse to run, reads still work.
    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::MigrationPending))
    );
    assert_eq!(
        s.client.try_create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://pending"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
        ),
        Err(Ok(Error::MigrationPending))
    );
    assert_eq!(s.client.get_campaign(&id).raised, 100);

    // The old data survives the migration untouched.
    assert_eq!(s.client.migrate(), SCHEMA_VERSION);
    assert!(!s.env.events().all().events().is_empty(), "Migrated event");
    assert_eq!(s.client.schema_version(), SCHEMA_VERSION);
    assert_eq!(s.client.get_campaign(&id).raised, 100);
    assert_eq!(s.client.donate(&donor, &id, &100), 100);
    assert_eq!(s.client.get_campaign(&id).raised, 200);
}

#[test]
fn migrate_is_idempotent_and_admin_only() {
    let s = Setup::new();
    let id = s.campaign();
    let before = s.client.get_campaign(&id);

    assert_eq!(s.client.migrate(), SCHEMA_VERSION);
    assert_eq!(s.client.migrate(), SCHEMA_VERSION);
    assert!(!s.env.events().all().events().is_empty(), "Migrated event");
    assert_eq!(s.client.get_campaign(&id), before);

    downgrade_to_unversioned(&s);
    assert!(s.client.mock_auths(&[]).try_migrate().is_err());
    assert_eq!(s.client.schema_version(), 0);
}
