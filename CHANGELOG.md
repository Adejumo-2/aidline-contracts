# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Per-campaign token support with an admin-managed allowlist, enabling campaigns to raise in USDC or other approved tokens
- Multisig verification for milestones with m-of-n verifier approval thresholds
- Tests for verifier bonds, sponsor matching pools and pledges, covering every error those features can return (34 new tests)
- Dependency audit job in CI (`cargo audit`)
- Emergency fast track: a partial first tranche for Emergency campaigns (#23, @oladayo2222)
- Per milestone due dates with partial refunds when a milestone is overdue (#26, @oladayo2222)
- Sponsor matching pools that match donations up to a cap (#27, @Feyisara2108)
- Fuzz testing harness, run for 30 seconds in CI (#28, @oladayo2222)
- Verifier bonds with slashing and a withdrawal delay (#29, @oladayo2222)
- Pledges that fund future milestones on approval (#30, @oladayo2222)
- Admin can reassign a campaign's verifier (#4, @RaymondAbiola)
- `AdminChanged` and `VerifierReassigned` events (#2, @RaymondAbiola)
- Permissionless `bump_campaign_ttl` keeper call that renews a campaign, its due dates and campaign-wide entries, and the instance entry (#185)
- Doc comments on every public function (#1, @RaymondAbiola), a resource cost table and exact event assertions in tests (#24, #25, @oladayo2222)

### Fixed

- Every state-changing entry point extends the instance entry, so a deployment that stops creating campaigns no longer lets its admin, token and counters expire (#183)
- Milestone due dates stay in storage until past the campaign deadline, so overdue milestones keep opening refunds on campaigns longer than 120 days (#184)
- Fuzz harness now builds and the CI fuzz job runs from the repository root

### Changed

- Fewer storage host calls: a campaign loaded and written back in one call has its TTL extended once, refunds skip the due-date read on ended campaigns and the stream scan on cancelled ones, and milestone approval reads the treasury only when a fee is set (#186)
- **Migration note**: The `Campaign` struct now stores its own `token: Address` field. Campaign creation entry points (`create_campaign`, `create_multisig_campaign`, `create_campaign_with_due_dates`, `create_multisig_with_due_dates`) now take `token: Address`, which must be approved on the admin token allowlist. All campaign donations, milestone releases, sponsor pools, pledges, and refunds now operate in the campaign's specified token.

## [0.1.0] - 2026-10-04

First public testnet release.

### Added

- Milestone based escrow contract with `Emergency` and `Climate` campaign types
- Verifier registry managed by an admin
- Donations capped at the campaign goal
- Milestone release by the assigned verifier, with a proof URI emitted on chain
- Cancellation by the creator or admin
- Pro rata refunds of unreleased funds after cancellation or expiry
- Events for every state change, for indexers
- Unit tests, CI, testnet deploy script and architecture docs
