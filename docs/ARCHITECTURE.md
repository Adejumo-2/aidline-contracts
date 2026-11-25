# Architecture

This document explains how the Aidline contract is put together and why. If you are about to change contract behavior, read this first.

## Goals

- **Donors can verify where money went.** Every movement of funds emits an event and is tied to a proof link.
- **Nobody holds funds on trust.** Money only leaves escrow through a verifier approval or a donor refund.
- **Donors are never stuck.** If a campaign stalls, unreleased funds can always be reclaimed.
- **Simple enough to audit.** One contract, one token, no upgradability tricks in the MVP.

## Roles

| Role | Powers |
| --- | --- |
| Admin | Adds and removes verifiers, can cancel any campaign, can hand admin to another address, can slash verifier bonds, can pause/unpause for emergencies, can upgrade contract code |
| Creator | Opens campaigns, can cancel their own campaign |
| Beneficiary | Receives milestone payouts. Often the same as the creator, but can be a separate NGO wallet |
| Verifier | Approves milestones for campaigns assigned to them, while registered by the admin. Must post a bond to register. |
| Donor | Donates, and claims refunds when a campaign is cancelled or expired |
| Sponsor | Deposits a matching pool for a specific campaign |
| Pledgor | Creates a pledge backed by a token allowance; funds are pulled when a milestone is approved |

## Campaign lifecycle

```
              create_campaign
                    │
                    ▼
               ┌─────────┐   approve last milestone   ┌───────────┐
   donate ───▶ │ Active  │ ─────────────────────────▶ │ Completed │
   approve ──▶ │         │                            └───────────┘
               └────┬────┘
        cancel      │      deadline passes
     ┌──────────────┴──────────────┐
     ▼                             ▼
┌───────────┐              Active but expired
│ Cancelled │              (no stored status change)
└─────┬─────┘                      │
      └──────────┬─────────────────┘
                 ▼
          donors call refund
```

Expiry is computed from the ledger timestamp rather than stored, so no one needs to send a transaction to "close" a campaign.

## Emergency Fast Track

For `Emergency` campaigns, the assigned verifier can release an emergency fast-track advance immediately, before milestone one is completed. 
This provides critical funds in the first hours of a disaster without waiting for a full milestone to be funded or proven.

- **Cap:** The advance is hard-capped at `min(20% of the campaign's total goal, milestone 0 allocation)`.
- **Escrow requirements:** The advance cannot exceed the currently available unreleased escrow.
- **Accounting:** The advance is recorded against milestone one (`milestones[0]`). When the verifier later formally approves milestone one, the contract only transfers the remaining balance (if any) of that milestone to the beneficiary. The total funds released for milestone one never exceed its scheduled amount.
- **Security:** Only the registered verifier can trigger the fast track. It is only available for `Emergency` campaigns, and only once per campaign.

## Resource budget tests

`mod budget` in `contracts/aidline/src/test.rs` reads the host budget (CPU instructions and memory bytes) for `create_campaign`, `donate`, `apply_matching`, `approve_milestone` and `refund`, and fails if a call goes over the ceiling in its table, naming the call and the overshoot. The ceilings start at a quarter of the network's per-transaction limits. To tighten them, run `cargo test budget -- --nocapture` to print the measured numbers and set each ceiling about 25 percent above its number.
## Verifier track record

The contract keeps a fixed-size `VerifierStats` entry per verifier, read with `verifier_stats(verifier)`. It never grows with the number of campaigns.

| Counter | Updated when |
|---------|--------------|
| `campaigns_assigned` | a campaign is created with the verifier, and when `reassign_verifier` makes them the campaign's verifier |
| `milestones_approved` | the verifier records an approval (each multisig vote counts for the voter) |
| `campaigns_completed` | the last milestone is released while the verifier is assigned |
| `campaigns_cancelled` | `cancel_campaign`, `cancel_campaign_with_reason` or a missed milestone due date cancels the campaign while the verifier is assigned |
| `total_slashed` | `slash_verifier` takes `slash_amount` from their bond |

Reassignment never moves counters: the previous verifier keeps what happened while they held the campaign, and everything after the handover counts for the new verifier.

## Split payouts

A campaign normally pays one beneficiary. `create_campaign_with_beneficiaries` takes up to `MAX_BENEFICIARIES` (10) recipients with shares in basis points instead.

- **Validation:** shares are positive, sum to exactly 10,000 and name distinct addresses, and the verifier cannot be one of the beneficiaries (`ConflictOfInterest`). Anything else is `InvalidBeneficiaries`.
- **Payout:** `approve_milestone` (after the platform fee) and `emergency_fast_track` give each beneficiary `amount * bps / 10_000`, rounded down. The rounding dust goes to the first beneficiary, so the payments always sum to exactly the amount released.
- **Events:** every recipient that receives funds gets a `paid` event with the campaign id, the recipient and the amount.
- **Single beneficiary:** a one-entry list, or `create_campaign`, behaves exactly as before. `get_beneficiaries` returns the single beneficiary with 10,000 bps for those campaigns.

## Emergency Pause Switch

In case an exploit or critical bug is discovered, the admin can pause state-mutating operations using `pause` and resume them using `unpause`.

- **Blocked functions:** While paused, `donate`, `approve_milestone` (including multisig approvals), `emergency_fast_track`, and `create_campaign` (including multisig and due dates creation) reject immediately with `Error::Paused`.
- **Refunds always allowed:** `refund` is never blocked by the pause switch. Donors can always withdraw unreleased funds if a campaign has been cancelled, expired, or had an overdue milestone.
- **View functions:** View functions (`get_campaign`, `is_paused`, etc.) remain accessible at all times.
- **Events:** `paused` and `unpaused` are emitted with the acting admin address.

## Contract Upgrades

Aidline provides an admin-controlled upgrade path via `upgrade(new_wasm_hash)` so that bugs can be fixed or features added without abandoning existing campaign history and escrow balances.

### Upgrade Procedure

1. **Build and Validate**: Compile and test the new contract code locally (`cargo test`, `stellar contract build`).
2. **Optional Pause**: For high-risk upgrades, the admin may call `pause()` to freeze donations and milestone payouts while preserving refunds.
3. **Install Bytecode**: Install the new Wasm binary onto the network using the Stellar CLI:
   ```sh
   stellar contract install --wasm target/wasm32v1-none/release/aidline.wasm --source admin
   ```
   This stores the bytecode on-chain and outputs its 32-byte Wasm hash (`new_wasm_hash`).
4. **Trigger Upgrade**: The admin calls `upgrade(new_wasm_hash)` on the contract. The contract verifies admin authorization, updates the contract executable via `update_current_contract`, and emits the `upgraded` event.
5. **Post-Upgrade Verification**: Verify that existing campaign state and storage remain intact, then call `unpause()` if the contract was paused.

### Trust Implications and Security

- **Centralized Control**: The upgrade path gives the admin address the authority to replace contract logic. Donors and beneficiaries must trust the admin not to deploy malicious or compromised bytecode.
- **State Preservation**: In Soroban, updating the contract executable preserves all instance and persistent storage entries (campaigns, contributions, bonds, pledges). New contract versions must remain schema-compatible with existing storage keys.
- **Mitigation**: In production deployments, admin control should be transferred to a multisig account, a timelock contract, or a DAO to mitigate single-key compromise risks.

## Platform Fee and Treasury

The admin can configure an optional platform fee and treasury address to sustainably cover platform operational expenses.

- **Capped Rate:** The platform fee is specified in basis points and capped at 200 bps (2.00%). The default is 0 bps (no fee).
- **Treasury Address:** Fees are routed to the stored `treasury` address. If no treasury address is set or the fee is 0 bps, no fee is taken.
- **Collection on Milestone Release:** The fee is calculated and deducted when a milestone is approved and released (`fee = milestone_amount * fee_bps / 10_000`). The net payout is transferred to the beneficiary, and the fee is transferred directly to the treasury address.
- **Transparency:** The exact fee deducted is included in the `milestone_released` event.
- **Refund Math Unaffected:** Because the entire scheduled milestone amount leaves contract escrow on release (`beneficiary_payout + fee == milestone_amount`), the total released accounting reflects all escrow deductions. Donor refund calculations `contribution * (raised - released) / raised` remain unaffected and exact.

## Storage

| Key | Storage type | Value |
| --- | --- | --- |
| `Admin` | instance | `Address` |
| `Token` | instance | `Address` of the Stellar Asset Contract used for all campaigns |
| `CampaignCount` | instance | `u64`, also the next campaign id |
| `Verifier(Address)` | persistent | `true` when registered, removed when unregistered |
| `Campaign(u64)` | persistent | `Campaign` struct |
| `Contribution(u64, Address)` | persistent | `i128` total donated by that donor, removed after refund |
| `SponsorPool(u64, Address)` | persistent | `SponsorPool` struct (Issue #27) |
| `VerifierBond(Address)` | persistent | `VerifierBond` struct (Issue #29) |
| `BondRequirement` | instance | `i128` required bond amount (Issue #29) |
| `BondWithdrawDelay` | instance | `u64` withdrawal delay in seconds (Issue #29) |
| `PledgeCount` | instance | `u64` next pledge id (Issue #30) |
| `Pledge(u64)` | persistent | `Pledge` struct (Issue #30) |
| `MilestoneApprovals(u64, u32)` | persistent | `Vec<Address>` approved verifiers for milestone |
| `Paused` | instance | `bool`, true when contract is paused in emergencies |
| `PlatformFee` | instance | `u32` fee in basis points (capped at 200 bps / 2%) |
| `Treasury` | instance | `Address` treasury address receiving release fees |
| `CampaignDonors(u64)` | persistent | `Vec<Address>` unique donor addresses in order of first donation |

TTLs are extended on every read and write (instance: 30 days, persistent: 120 days).

- **Instance entry:** every state-changing entry point extends it, either directly (admin, verifier, bond and pledge calls) or by loading a campaign, so a deployment that stops creating campaigns keeps its admin, token and counters (#183).
- **Milestone due dates:** they are kept alive until a week past the campaign deadline when that is further away than 120 days, capped at the network's maximum TTL. Every read during overdue checks keeps that guarantee, and `extend_deadline` moves it with the new deadline, so a missed milestone always opens refunds (#184).
- **Keeper call:** for long-running campaigns (such as multi-year climate projects) that may stay quiet for months, anyone (an off-chain keeper, a backend worker, a community member) can call `bump_campaign_ttl(campaign_id)` without authorization. It renews the instance entry, the campaign, its due dates and its other campaign-wide entries (split beneficiaries, stream schedule, donor and pledge lists, start time, slashed recovery, cancel-vote total). Per-donor entries (contributions and cancel votes) are renewed when that donor's entry is read or written. `extend_campaign_ttl` is the same call under its earlier name (#185).
- **Writes after reads:** an entry point that loads a campaign and writes it back extends its TTL once, on the read (#186).

## Refund math

Once a campaign is cancelled or expired, `raised` and `released` can no longer change, because both `donate` and `approve_milestone` require an open campaign. Each donor's refund is:

```
refund = contribution * (raised - released) / raised
```

Integer division rounds down, so a few stroops of dust can remain in the contract per campaign. This is deliberate: rounding in favor of the contract means the last donor can never be short.

Sponsors are also included in the refund pool (their `Contribution` entry tracks the matched amount), so unused sponsor matching funds can be partially reclaimed through the standard refund path in addition to `return_sponsor_pool`.

## Events

| Event | Topics | Data |
| --- | --- | --- |
| `campaign_created` | `campaign_id` | `creator`, `kind`, `goal`, `deadline`, `min_donation` |
| `donated` | `campaign_id`, `donor` | `amount` |
| `emergency_advance_released` | `campaign_id`, `verifier` | `milestone_index`, `amount` |
| `milestone_released` | `campaign_id` | `index`, `amount`, `fee`, `proof_uri` |
| `campaign_cancelled` | `campaign_id` | none |
| `refunded` | `campaign_id`, `donor` | `amount` |
| `verifier_updated` | `verifier` | `active` |
| `admin_changed` | `old_admin` | `new_admin` |
| `sponsor_pool_deposited` | `campaign_id`, `sponsor` | `amount`, `ratio_bps`, `cap` |
| `matching_applied` | `campaign_id`, `sponsor` | `donor`, `donation_amount`, `matched_amount` |
| `sponsor_pool_returned` | `campaign_id`, `sponsor` | `amount` |
| `verifier_bond_posted` | `verifier` | `amount` |
| `verifier_deregistered` | `verifier` | `deregistered_at` |
| `bond_withdrawn` | `verifier` | `amount` |
| `bond_slashed` | `verifier` | `campaign_id`, `slashed_amount` |
| `pledge_created` | `pledge_id`, `campaign_id` | `donor`, `pledged_amount` |
| `pledge_pulled` | `pledge_id`, `campaign_id` | `donor`, `pulled_amount` |
| `pledge_skipped` | `pledge_id`, `campaign_id` | `donor`, `reason` |
| `pledge_settlement` | `campaign_id` | `milestone_index`, `total_pledged_pulled` |
| `verifier_reassigned` | `campaign_id` | `old_verifier`, `new_verifier` |
| `approved` | `campaign_id`, `milestone_index` | `verifier`, `approvals_count`, `threshold` |
| `rejected` | `campaign_id`, `milestone_index` | `reason`, `details_uri` |
| `paused` | `admin` | none |
| `unpaused` | `admin` | none |
| `upgraded` | none | `new_wasm_hash` |
| `meta_upd` | `campaign_id` | `new_metadata_uri` |
| `dl_ext` | `campaign_id` | `old_deadline`, `new_deadline` |
| `fee_upd` | none | `fee_bps` |
| `tres_upd` | none | `treasury` |

The backend indexer reads these to build campaign pages, donor histories and impact reports without scanning contract storage. See [docs/EVENTS.md](EVENTS.md) for the complete reference of all event structs, topic structures, data payloads, and emitting functions.

## Design decisions

**Donations accept up to the remaining goal amount.** A donation exceeding the remaining goal accepts `min(amount, goal - raised)`, transfers only that accepted amount into escrow, and returns it. Once a campaign is fully funded (`raised == goal`), subsequent donations reject with `GoalExceeded`. This keeps milestone math exact without rejecting generous donors.

**Milestones are released strictly in order.** Partial or parallel milestones add complexity that the MVP does not need.

**Proofs live off chain.** Only the proof URI is emitted. Photos and receipts belong on IPFS or the backend, not in contract storage.

**One token per deployment.** Multi asset support is a planned improvement.

**Removing a verifier freezes their campaigns.** Funds are not lost: the creator or admin can cancel, which opens refunds. The admin can also reassign the verifier on an active campaign using `reassign_verifier`.

**Creators can extend deadlines once.** To accommodate unexpected field work delays without prematurely triggering refund windows, campaign creators can extend their deadline once by at most 30 days while the campaign is active.

---

## Multisig Verification: M-of-N Verifiers per Milestone

### Why multisig verification exists

A single verifier represents a single point of failure or compromise. For high-value campaigns, creators can configure multiple independent verifiers and an approval threshold (e.g. 2 of 3, or 3 of 5).

### How it works

1. **Creation**: Creators specify `verifiers: Vec<Address>` and `threshold: u32` via `create_multisig_campaign`. The threshold must satisfy `1 <= threshold <= verifiers.len()`, with no duplicate verifiers, and every verifier must be currently registered in the contract's verifier registry.
2. **Approval**: Verifiers submit approvals via `approve_milestone_multisig(campaign_id, verifier, proof_uri)`.
3. **Threshold Enforcement**:
   - Each approval emits an `approved` event with the verifier, current approval count, and threshold.
   - Duplicate approvals from the same verifier for the same milestone are rejected with `DuplicateApproval`.
   - Milestone funds are only released to the beneficiary once the approval count reaches `threshold`.
4. **Single-verifier compatibility**:
   - Single-verifier campaigns created via `create_campaign` default to `threshold = 1` and `verifiers = [verifier]`.
   - The original `approve_milestone(campaign_id, proof_uri)` entry point remains supported for single-verifier campaigns, preserving existing integrations.

---

## Issue #27: Sponsor Matching Funds

### What it is

Diaspora associations and companies can pledge to match community donations at a configured ratio up to a configured maximum cap. The matched funds come from a pool the sponsor deposits into the contract ahead of time, so they are always real tokens already held in escrow.

### How the ratio works

The sponsor specifies a ratio in **basis points** (bps): 100 bps = 1 %, 5 000 bps = 50 %, 10 000 bps = 100 %. For a donation of `D` tokens the raw match is:

```
raw_match = D * ratio_bps / 10_000
```

Integer division is used, so tiny donations may produce 0 match (not an error).

### How the cap works

The sponsor sets a `cap` at deposit time equal to the deposited `amount`. For the lifetime of the pool, the total matched can never exceed `cap`. After the cap is reached, further calls to `apply_matching` return 0.

### Matching constraints

The effective match for a single call to `apply_matching` is the minimum of:
1. `raw_match` (ratio-based)
2. `pool.cap - pool.matched` (cap headroom)
3. `pool.remaining` (undepleted pool balance)
4. `campaign.goal - campaign.raised` (campaign headroom, cannot overfund)

### When the pool is exhausted

When `pool.remaining == 0`, `apply_matching` returns 0. No error is raised; the caller just receives no match. The sponsor pool record remains until `return_sponsor_pool` is called.

### How unused funds are returned

After the campaign is no longer active (Completed, Cancelled, or past its deadline), the sponsor calls `return_sponsor_pool`. The remaining pool balance is transferred back to the sponsor and the pool record is deleted.

### Why `apply_matching` is separate from `donate`

Soroban persistent storage does not support prefix-scanning (iterating all keys with a given prefix). Because sponsors are not enumerable inside `donate`, the matching must be triggered explicitly. Any account, the donor, the sponsor, or a keeper, may call `apply_matching` at any time while the campaign is active.

### Accounting and security trade-offs

- **Funds are always real.** The pool is funded up front, so `apply_matching` never mints tokens, it only moves accounting entries.
- **Sponsors share in refunds.** The sponsor's matched amount is recorded as a `Contribution` entry, so they receive a pro-rata refund of unmatched/unreleased funds when a campaign is cancelled or expired. Additionally, `return_sponsor_pool` returns any tokens that were deposited but never matched.
- **No overfunding.** The campaign-headroom cap ensures that matching cannot push `raised` above `goal`.
- **One pool per sponsor per campaign.** Prevents accidental double-deposits. A sponsor must call `return_sponsor_pool` to clear a pool before creating a new one.
- **Integer truncation.** All matching uses integer division truncating toward zero. This means dust can accumulate in the pool but no funds are ever created from nothing.

---

## Issue #29: Verifier Bonds and Slashing

### Why verifier bonds exist

Under the baseline design verifiers risk nothing when approving false milestones. A bond makes approval costly for bad actors: if a verifier approves fraudulent work, the admin can slash their bond and distribute the slashed amount to the affected campaign's donors.

### Bond requirement

The admin sets a required bond amount (`set_bond_requirement`) and withdrawal delay (`set_bond_withdraw_delay`) before bonds are enforced. Verifiers must call `register_with_bond` (which replaces `add_verifier` for bonded registrations) and transfer at least the required bond. If the transferred amount is less than the requirement, registration fails.

### How registration works

`register_with_bond(verifier, bond_amount)`:
1. Requires auth from the verifier.
2. Transfers `bond_amount` tokens from the verifier to the contract.
3. Stores a `VerifierBond` record with status `Active`.
4. Marks the verifier as active in the verifier registry.
5. Emits `VerifierBondPosted`.

### How deregistration works

`deregister_verifier(verifier)`:
1. Requires auth from the verifier or the admin.
2. Sets bond status to `PendingWithdrawal` and records `deregistered_at`.
3. Removes the verifier from the active registry.
4. Emits `VerifierDeregistered`.

### Withdrawal delay semantics

After deregistration the verifier must wait at least `bond_withdraw_delay` seconds before calling `withdraw_bond`. This delay gives the admin time to slash if a fraud dispute is raised.

### Bond withdrawal while campaigns are open (#129)

A verifier cannot take their bond back while they are still responsible for a campaign. The contract keeps a count per verifier of campaigns that are assigned to them and not yet completed or cancelled: it goes up when a campaign is created with them or reassigned to them, and down when the campaign completes, is cancelled, or is reassigned away. `withdraw_bond` fails with `VerifierHasActiveCampaigns` while the count is above zero, so the bond stays slashable for the whole life of the campaigns. A campaign past its deadline stays active until it is cancelled (the creator or admin can cancel it), which then releases the verifier. Campaigns created before this change are not counted.

### Donor vote to cancel a stalled campaign (#114)

Donors have a say when a verifier goes silent.

- **Weight:** a donor's vote counts for their contribution at the time they vote, once per donor, and only while the campaign is `Active`.
- **Stall period:** `STALL_PERIOD`, 14 days with no progress, measured from the later of the campaign's start and its last approved milestone. Any approval resets it.
- **Threshold:** the total vote weight must reach `CANCEL_VOTE_BPS` (50%) of `raised`.
- **Trigger:** `vote_cancel` cancels the campaign if both conditions hold when the vote is cast. A vote cast before the stall period ended is recorded, and anyone can call `execute_vote_cancel` once the period has passed. The campaign then becomes `Cancelled` and refunds open.
- **Withdrawing:** `withdraw_cancel_vote` removes the donor's vote while the campaign is still `Active`.
- **Events:** `cvote`, `cvote_wd` and the usual `canceled`.

### Streaming release (#117)

`create_campaign_with_streams` takes a `stream_days` entry per milestone. `0` is a normal lump sum. A positive value makes that milestone's payout, after the platform fee, vest linearly over that many days from the moment it is approved. Nothing is transferred at approval; the beneficiary calls `claim_streamed` to withdraw what has vested so far, as often as they like, and `vested = total * elapsed / duration`, capped at `total`. The funds stay in escrow until claimed, and the milestone still counts as released. See [ADR 0007](adr/0007-streaming-release.md) for what happens on cancel.

### How disputes and slashing work

`slash_verifier(admin, verifier, campaign_id, slash_amount)`:
1. Requires auth from the admin.
2. Checks that `slash_amount <= bond.remaining`.
3. Reduces `bond.remaining` by `slash_amount`.
4. Credits `slash_amount` to the campaign's `SlashedRecovery` pool (`campaign.raised` is left untouched). `refund` pays each donor `contributed * (raised - released + slashed_recovery) / raised`, so slashed funds are distributed pro-rata to contributors and nothing is credited to the contract address.
5. Emits `BondSlashed`.

Preventing double-slash for the same dispute is enforced by tracking slashed amounts; the admin cannot slash more than `bond.remaining`.

### Security and trust assumptions

- Only the admin can slash.
- A verifier cannot withdraw a slashed portion.
- The withdrawal delay ensures time for the admin to act on disputes.
- Slashing distributes through the existing refund formula, no new distribution logic needed.

### Trade-offs

- A single admin can slash, so the design trusts the admin. A future improvement is a multisig or DAO-controlled slash authority.
- The withdrawal delay is a fixed parameter; campaigns that run longer than the delay may not be fully protected.

---

## Issue #30: Pledges That Fund Future Milestones on Approval

### Why pledges exist

Some donors prefer to commit funds only when work is verified, rather than transferring tokens up front. Pledges allow a donor to pre-authorize a token allowance; the contract pulls funds proportionally when each milestone is approved.

### How pledges differ from escrow

With normal donations, tokens move into the contract immediately. With pledges, tokens stay in the donor's wallet until a milestone is approved. The donor must maintain a sufficient token allowance pointing at the contract; if the allowance is insufficient at pull time, the pledge is skipped for that milestone.

### Token allowance semantics

Before creating a pledge the donor must call `approve` on the token contract to grant the Aidline contract an allowance of at least `pledged_amount`. The contract reads the allowance at pull time via `token::Client::allowance`. If the allowance is lower than the donor's remaining pledge, the contract pulls only what is available (or skips the pledge entirely if nothing is available).

### When funds are actually pulled

Funds are pulled inside `approve_milestone` after the verifier's signature is verified and the milestone is confirmed funded (by direct donations). Active pledges for the campaign are iterated and pulled proportionally.

### Proportional allocation

Given a set of active pledges `[p1, p2, …, pN]` and a milestone amount `M`, each pledge's share is:

```
share_i = M * p_i.remaining / total_remaining
```

where `total_remaining = sum of all p_i.remaining` for active pledges.

Integer division truncates toward zero. Any unallocated remainder stays in the milestone's existing funded balance (direct donations). A pledge that cannot cover its full share is pulled for as much as the allowance permits; the rest is skipped without reverting.

### What happens when allowance is unavailable

If a donor's token allowance is 0 or insufficient for their share:
- The pledge is skipped for this milestone.
- A `PledgeSkipped` event is emitted.
- Other pledges are still processed.
- The milestone approval does not revert.

### Rounding and accounting behavior

- All arithmetic uses `i128` with integer truncation.
- No pledge pull can exceed the donor's remaining `pledged_amount`.
- No pledge pull can exceed the donor's current token allowance.
- The total pledged pulled is added to `campaign.raised` and recorded as a `Contribution` so pledgors share in refunds if the campaign is cancelled before completion.

### Donor/user experience

1. Donor approves the token contract: `token.approve(donor, aidline_contract, pledged_amount, expiry)`.
2. Donor calls `create_pledge(donor, campaign_id, pledged_amount)`.
3. When each milestone is approved, the contract automatically pulls the donor's proportional share.
4. If the donor revokes their allowance, the pledge is silently skipped.

### Security and trust trade-offs

- **No custody until pull.** Donor retains control of tokens until milestone approval.
- **Allowance can be revoked.** Donors can back out by revoking the allowance; this is a feature, not a bug.
- **No guaranteed funding.** If all pledges are revoked, milestone approval may still succeed if direct donations cover the milestone (pledges are supplementary, not substitutive).
- **Deterministic allocation.** The proportional formula is deterministic given the same on-chain state; there is no randomness or ordering advantage.

### Advantages versus escrow

| Property | Pledge | Escrow (donate) |
| --- | --- | --- |
| When tokens leave donor wallet | At milestone approval | At donation time |
| Donor retains control until | Milestone approval | Never |
| Guaranteed funding | No (allowance may lapse) | Yes (tokens already in contract) |
| Refund needed if campaign fails | No (tokens never left) | Yes |

---

## Optional Minimum Donation

Creators can configure an optional minimum donation floor when creating a campaign via `create_campaign_min_donation`.

- **Purpose**: Prevents tiny donations from bloating the contribution map in persistent storage and making pro-rata refunds cost more in network transaction fees than the refund amount itself.
- **Enforcement**: `donate` checks the incoming amount against `min_donation`. If `amount < campaign.min_donation` and the donation does not fill the remaining funding goal (`amount < remaining`), the call fails with `Error::DonationBelowMinimum`.
- **Final donation exception**: If the remaining balance to reach the campaign goal is smaller than `min_donation`, a final donation that reaches or exceeds the remaining goal (`amount >= remaining`) is accepted, ensuring campaigns can always reach 100% funding without getting stuck.
- **Backwards compatibility**: Campaigns created through standard entry points (`create_campaign`, `create_multisig_campaign`, `create_campaign_with_due_dates`) default to `min_donation = 0`, which imposes no floor and preserves original behavior.

---

## Paginated Donor Enumeration

Frontends and indexers frequently need to enumerate who donated to a campaign without having to replay every single `Donated` event from ledger inception.

- **Storage layout**: Unique donor addresses for each campaign are indexed in persistent storage under `DataKey::CampaignDonors(campaign_id)` as an ordered `Vec<Address>`.
- **Deduplication**: When donations are received via `donate` (or pledges settled via `pull_pledges`), the contract checks whether the donor address is already present in the campaign's donor vector. An address is appended only on its first donation. Repeat donations from the same donor do not duplicate the entry.
- **Stable ordering**: Donors are stored in insertion order (the order in which their first contribution was accepted).
- **Pagination**: The public view function `donors(campaign_id, start, limit) -> Vec<Address>` provides paginated access. To prevent excessive gas or resource consumption in a single invocation, `limit` is hard-capped at 50 (`MAX_DONORS_PAGE_LIMIT`). Any `limit` requested above 50 is clamped down to 50.

---

## Known limitations and roadmap

- Verifier reassignment for a live campaign
- Overfunding and stretch goals
- Multi token campaigns
- TTL keeper for very long climate campaigns
- Enumerate sponsor pools per campaign (requires off-chain indexer)
- Enumerate pledges per campaign (requires off-chain indexer)
- External security audit before mainnet

## Resource Costs (Issue #24)

To help contributors understand the impact of contract changes, the following table tracks the resource costs (CPU instructions and memory usage) for the main entry points.

| Function | CPU Instructions | Memory | Notes |
| -------- | ---------------: | -----: | ----- |
| `create_campaign` | (Requires Regeneration) | (Requires Regeneration) | 2 milestones |
| `donate` | (Requires Regeneration) | (Requires Regeneration) | Standard donation |
| `emergency_fast_track` | (Requires Regeneration) | (Requires Regeneration) | First-time advance |
| `approve_milestone` | (Requires Regeneration) | (Requires Regeneration) | Milestone 1 |
| `cancel_campaign` | (Requires Regeneration) | (Requires Regeneration) | By creator |
| `refund` | (Requires Regeneration) | (Requires Regeneration) | Single donor |

*Note: The values above currently require regeneration.*

### How to regenerate the table

Maintainers can regenerate these measurements using the built-in test environment. The test `test_measure_resource_costs` in `contracts/aidline/src/test/costs_and_events.rs` executes each entry point and resets the budget.

1. Ensure you have the Soroban CLI and test tools installed.
2. Run the test and capture the output:
   ```bash
   cargo test test_measure_resource_costs -- --nocapture
   ```
3. Read the printed budget costs for each step and update the table above.
