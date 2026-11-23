use super::*;

// ─── Upgrade tests ────────────────────────────────────────────────────────────

#[contract]
pub struct MockUpgradedAidline;

#[contractimpl]
impl MockUpgradedAidline {
    pub fn version() -> u32 {
        2
    }

    pub fn get_campaign(env: Env, campaign_id: u64) -> crate::Campaign {
        crate::storage::campaign(&env, campaign_id).unwrap()
    }
}

#[test]
fn admin_can_upgrade_contract_and_preserves_state() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &300);

    // Upload the new contract code
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    // Admin upgrades the contract
    s.client.upgrade(&new_wasm_hash);

    // Verify upgrade took effect at the same contract address
    let upgraded_client = MockUpgradedAidlineClient::new(&s.env, &s.client.address);
    assert_eq!(upgraded_client.version(), 2);

    // State (campaign) is preserved across upgrade
    let c = upgraded_client.get_campaign(&id);
    assert_eq!(c.id, id);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.raised, 300);
}

#[test]
fn non_admin_cannot_upgrade_contract() {
    let s = Setup::new();
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    let res = s.client.mock_auths(&[]).try_upgrade(&new_wasm_hash);
    assert!(res.is_err());
}
