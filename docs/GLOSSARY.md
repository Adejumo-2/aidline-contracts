# Aidline Domain Glossary

This glossary defines key domain terms used throughout the Aidline smart contracts, documentation, and user interfaces. Terms are listed in alphabetical order with links to relevant contract functions, types, and architectural documentation.

---

### Admin
The privileged address that manages platform configurations. Powers include registering and deregistering verifiers, managing allowed campaign tokens, configuring platform fees and treasury addresses, setting bond requirements and withdrawal delays, slashing fraudulent verifier bonds, executing emergency pauses/unpauses, and upgrading contract code.
- **Related functions**: [`set_admin`](../contracts/aidline/src/lib.rs), [`add_verifier`](../contracts/aidline/src/lib.rs), [`pause`](../contracts/aidline/src/lib.rs), [`slash_bond`](../contracts/aidline/src/lib.rs), [`set_platform_fee`](../contracts/aidline/src/lib.rs), [`set_treasury`](../contracts/aidline/src/lib.rs), [`upgrade`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Beneficiary
The wallet address designated to receive milestone funds upon successful verification. Often the project operator, local partner, or aid NGO carrying out field work.
- **Related functions**: [`create_campaign`](../contracts/aidline/src/lib.rs), [`approve_milestone`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Campaign
An on-chain escrow and milestone tracking entity. A campaign is initialized with a goal, deadline, accepted token, creator, beneficiary, and assigned verifier(s). Lifecycle statuses are `Active`, `Completed`, or `Cancelled`.
- **Related types**: [`Campaign`](../contracts/aidline/src/types.rs), [`CampaignStatus`](../contracts/aidline/src/types.rs), [`CampaignKind`](../contracts/aidline/src/types.rs)
- **Reference**: [Architecture: Campaign lifecycle](ARCHITECTURE.md#campaign-lifecycle)

### Campaign Token
The specific Stellar Asset Contract (SAC) address (e.g. USDC, XLM) in which a campaign raises donations, holds escrow, and distributes milestone payouts. Tokens must be approved on the admin allowlist.
- **Related functions**: [`add_allowed_token`](../contracts/aidline/src/lib.rs), [`is_token_allowed`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Storage](ARCHITECTURE.md#storage)

### Creator
The account that opens and configures a campaign. Creators have exclusive rights to cancel their active campaign or grant a one-time deadline extension of up to 30 days.
- **Related functions**: [`create_campaign`](../contracts/aidline/src/lib.rs), [`cancel_campaign`](../contracts/aidline/src/lib.rs), [`extend_deadline`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Donor
An individual or organization that transfers tokens into a campaign's escrow. Donors are recorded in contract storage and retain a pro-rata entitlement to unreleased escrow if the campaign does not complete.
- **Related functions**: [`donate`](../contracts/aidline/src/lib.rs), [`contribution_of`](../contracts/aidline/src/lib.rs), [`refund`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Emergency Advance (Fast Track)
An early payout mechanism for `CampaignKind::Emergency` campaigns. Allows the assigned verifier to immediately disburse an advance of up to 20% of the total goal to the beneficiary before milestone one is verified, providing immediate disaster relief funding.
- **Related functions**: [`emergency_fast_track`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Emergency Fast Track](ARCHITECTURE.md#emergency-fast-track)

### Emergency Pause Switch
An administrative control that freezes state-mutating functions (`donate`, `approve_milestone`, `create_campaign`, `emergency_fast_track`) if an exploit or critical bug is detected. Donor refunds remain continuously functional while paused.
- **Related functions**: [`pause`](../contracts/aidline/src/lib.rs), [`unpause`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Emergency Pause Switch](ARCHITECTURE.md#emergency-pause-switch)

### Escrow
The locked token balance held directly by the Aidline smart contract instance. Escrowed tokens can only leave the contract via verifier milestone approval to the beneficiary or pro-rata refund claims to donors.
- **Related functions**: [`donate`](../contracts/aidline/src/lib.rs), [`approve_milestone`](../contracts/aidline/src/lib.rs), [`refund`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Goals](ARCHITECTURE.md#goals)

### Matching Pool (Sponsor Pool)
A pre-funded token deposit created by a sponsor that automatically matches incoming community donations at a designated ratio in basis points (e.g. 5,000 bps = 50%) up to a predetermined matching cap.
- **Related functions**: [`deposit_sponsor_pool`](../contracts/aidline/src/lib.rs), [`apply_matching`](../contracts/aidline/src/lib.rs), [`return_sponsor_pool`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Sponsor Matching Funds](ARCHITECTURE.md#issue-27-sponsor-matching-funds)

### Milestone
A discrete delivery stage within a campaign associated with a required funding amount. Milestones are paid out sequentially to the beneficiary only after verifier verification.
- **Related functions**: [`approve_milestone`](../contracts/aidline/src/lib.rs), [`approve_milestone_multisig`](../contracts/aidline/src/lib.rs), [`reject_milestone`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Goals](ARCHITECTURE.md#goals)

### Milestone Approval
The on-chain transaction by which an authorized verifier attests that a milestone's work has been completed according to off-chain evidence (linked via IPFS/HTTPS proof URI), triggering token release.
- **Related functions**: [`approve_milestone`](../contracts/aidline/src/lib.rs), [`approve_milestone_multisig`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Multisig Verification](ARCHITECTURE.md#multisig-verification-m-of-n-verifiers-per-milestone)

### Milestone Due Date
An optional timestamp assigned to individual milestones during campaign creation. If a campaign fails to deliver an approved milestone before its scheduled due date, donors become eligible to claim pro-rata refunds.
- **Related functions**: [`create_campaign_with_due_dates`](../contracts/aidline/src/lib.rs), [`is_refundable`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Storage](ARCHITECTURE.md#storage)

### Minimum Donation
An optional minimum token amount required for individual donations to a campaign, configured at campaign creation time (`min_donation`). Smaller donations are rejected unless they reach or exceed the remaining goal amount.
- **Related functions**: [`create_campaign_min_donation`](../contracts/aidline/src/lib.rs), [`donate`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Campaign lifecycle](ARCHITECTURE.md#campaign-lifecycle)

### Multisig Verification
A security configuration requiring an M-of-N threshold of registered verifiers to independently approve a milestone before funds can be released to the beneficiary, eliminating single-verifier points of failure.
- **Related functions**: [`create_multisig_campaign`](../contracts/aidline/src/lib.rs), [`approve_milestone_multisig`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Multisig Verification](ARCHITECTURE.md#multisig-verification-m-of-n-verifiers-per-milestone)

### Platform Fee
An optional fee in basis points (capped at 200 bps / 2.00%) configured by the admin that is deducted from milestone release payouts and routed to the platform treasury to fund ongoing platform operations.
- **Related functions**: [`set_platform_fee`](../contracts/aidline/src/lib.rs), [`set_treasury`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Platform Fee and Treasury](ARCHITECTURE.md#platform-fee-and-treasury)

### Pledge
A donor commitment backed by a token allowance rather than upfront token transfer. When milestones are approved, the contract automatically pulls funds proportionally from active pledges to help cover the milestone release.
- **Related functions**: [`create_pledge`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Storage](ARCHITECTURE.md#storage)

### Pro-Rata Refund
The fair return of unreleased escrow to donors when a campaign is cancelled, expires past its deadline, or misses a milestone due date. Calculated as `contribution * (raised - released) / raised`.
- **Related functions**: [`refund`](../contracts/aidline/src/lib.rs), [`is_refundable`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Refund math](ARCHITECTURE.md#refund-math)

### Slash
An administrative penalty executed against a bonded verifier who acted fraudulently or approved bad milestones. The slashed tokens are confiscated from the verifier's bond and transferred directly into the affected campaign's escrow for donors.
- **Related functions**: [`slash_bond`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Verifier Bonds and Slashing](ARCHITECTURE.md#issue-29-verifier-bonds-and-slashing)

### Sponsor
An institution, business, or donor group that deposits a matching pool to match community contributions, amplifying grassroots support for causes.
- **Related functions**: [`deposit_sponsor_pool`](../contracts/aidline/src/lib.rs), [`return_sponsor_pool`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Treasury
The account address configured by the contract admin to receive platform fees deducted upon milestone releases.
- **Related functions**: [`set_treasury`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Platform Fee and Treasury](ARCHITECTURE.md#platform-fee-and-treasury)

### TTL Extension
The operation that bumps the Time-To-Live (storage expiration) of persistent Soroban ledger entries (e.g. long-running climate campaigns and donor contributions) to keep them alive on-chain indefinitely without requiring transaction authentication.
- **Related functions**: [`bump_campaign_ttl`](../contracts/aidline/src/lib.rs), [`extend_campaign_ttl`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Storage](ARCHITECTURE.md#storage)

### Verifier
A trusted entity (auditor, engineering firm, community representative) registered by the admin to inspect delivery evidence and approve milestone disbursements. Verifiers cannot be the campaign creator or beneficiary.
- **Related functions**: [`add_verifier`](../contracts/aidline/src/lib.rs), [`register_with_bond`](../contracts/aidline/src/lib.rs), [`approve_milestone`](../contracts/aidline/src/lib.rs), [`reject_milestone`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Roles](ARCHITECTURE.md#roles)

### Verifier Bond
Tokens posted and locked by a verifier upon registration. The bond guarantees accountability: it can be slashed by the admin if fraud occurs, and is subject to a mandatory withdrawal delay when a verifier deregisters.
- **Related functions**: [`register_with_bond`](../contracts/aidline/src/lib.rs), [`deregister_verifier`](../contracts/aidline/src/lib.rs), [`withdraw_bond`](../contracts/aidline/src/lib.rs), [`slash_bond`](../contracts/aidline/src/lib.rs)
- **Reference**: [Architecture: Verifier Bonds and Slashing](ARCHITECTURE.md#issue-29-verifier-bonds-and-slashing)
