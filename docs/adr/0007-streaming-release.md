# 0007: Streaming release and what happens to unvested funds

Status: Accepted

## Context

A milestone pays out in one lump. For long projects such as reforestation, paying over time limits the damage if work stops halfway. Streaming raises the question of what happens to the part that has not vested when the campaign is cancelled.

## Decision

A milestone can be marked as streamed at creation (`create_campaign_with_streams`, days per milestone). Approving it starts a stream instead of transferring funds, and the beneficiary claims the vested part with `claim_streamed`.

When a campaign is cancelled (by the creator, the admin, a missed due date or a donor vote), or when the first refund is taken after it expired, every running stream is **settled**:

1. The vested part at that moment stays claimable by the beneficiary. The stream's total is cut to it and vesting is over.
2. The unvested part is taken back out of `released`, so it is part of the pro rata refund pool: donors recover it through `refund` with the same rounding as any other unreleased escrow.
3. Settling again changes nothing.

## Consequences

- The beneficiary never loses what they had already earned, and donors never pay for time that was not worked.
- Escrow stays solvent: the contract always holds at least the unclaimed vested amounts plus the refundable pool.
- A completed campaign keeps its streams running; `claim_streamed` works until everything has vested and been claimed.
- Lump sum milestones are unchanged. A streamed milestone counts as released at approval, so the refund pool is computed on settlement rather than at approval.

## Alternatives considered

- **Forfeit the vested part on cancel:** punishes work already done.
- **Let the unvested part keep vesting after cancel:** defeats the purpose of streaming.
- **Pay the unvested part to the creator:** the creator is not owed money donors would have gotten back.
