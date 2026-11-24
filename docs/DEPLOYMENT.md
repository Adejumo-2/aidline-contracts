# Deployment

How to build and deploy the Aidline contract. Everything here targets **testnet**. Do not deploy to mainnet before an external audit. See [THREAT_MODEL.md](THREAT_MODEL.md).

## Prerequisites

- Rust with the `wasm32v1-none` target: `rustup target add wasm32v1-none`
- The [Stellar CLI](https://developers.stellar.org/docs/tools/cli) (CI uses version 28.1.0)
- A funded testnet identity:

  ```sh
  stellar keys generate deployer --network testnet --fund
  ```

## Build and test

```sh
make fmt     # format
make lint    # clippy with warnings as errors
make test    # unit tests
make build   # builds target/wasm32v1-none/release/aidline.wasm
```

These are the same checks CI runs.

## Verify a build (WASM hash)

Mainnet needs a recorded WASM hash, and a reviewer needs a way to check that a build matches it.

```sh
make wasm-hash
```

prints a report you can paste into a release note:

```text
wasm:    aidline.optimized.wasm
sha256:  <64 hex characters>
size:    <bytes> bytes
rustc:   rustc <version> (<commit> <date>)
cargo:   cargo <version> (<commit> <date>)
stellar: stellar <version> (<commit>)
```

The script builds with `--locked` into a fresh target directory, with incremental compilation off, a fixed `SOURCE_DATE_EPOCH` and the checkout and cargo paths remapped, then runs `stellar contract optimize`. The hash is for the optimized file, not the raw `cargo build` output that `make build` leaves in `target/`.

**What has to match.** The hash only reproduces on the same `rustc`, `cargo` and Stellar CLI versions, and from the same `Cargo.lock`. CI uses the floating `stable` Rust channel and Stellar CLI 28.1.0, so record the `rustc` and `stellar` lines from the report next to the hash in every release note, and install exactly those versions to verify it.

**Verifying a release.**

1. Check out the release tag.
2. Install the `rustc` and Stellar CLI versions from the release note.
3. Run `./scripts/wasm-hash.sh --verify <sha256 from the release note>`. It exits non-zero and prints both hashes if they differ.
4. Compare the hash with the one in the contract's deployed wasm (`stellar contract info`, or the hash used in `upgrade`) before trusting a deployment.

`./scripts/wasm-hash.sh --twice` builds twice from clean target directories and fails if the two hashes differ. Run it after changing the toolchain or the build profile.

## Deploy to testnet

```sh
./scripts/deploy-testnet.sh deployer
```

The script:

1. Builds the contract.
2. Deploys it with the identity as **admin** and native XLM as the token. Pass a token contract id as the second argument to use another token.
3. Registers the admin as the first verifier so you can test the whole flow.
4. Writes the result to `.env.testnet`.

`.env.testnet` looks like this:

```
AIDLINE_CONTRACT_ID=C...
AIDLINE_TOKEN_ID=C...
AIDLINE_ADMIN=G...
AIDLINE_NETWORK=testnet
```

Copy `AIDLINE_CONTRACT_ID` and `AIDLINE_TOKEN_ID` into the [backend](https://github.com/aidline-org/aidline-backend) `.env` and the [frontend](https://github.com/aidline-org/aidline-frontend) `.env.local`. The backend should also get `INDEXER_START_LEDGER` set to the deployment ledger.

## Testnet smoke test

After deploying to testnet, run the end-to-end smoke test script to verify that the deployed contract works correctly on-chain:

```sh
./scripts/smoke-test-testnet.sh <contract_id> <creator_identity> <beneficiary_identity> <verifier_identity> <donor_identity> [token_contract_id]
```

Or pass identities and the contract id via environment variables:

```sh
export AIDLINE_CONTRACT_ID="CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK"
export AIDLINE_CREATOR="creator"
export AIDLINE_BENEFICIARY="beneficiary"
export AIDLINE_VERIFIER="verifier"
export AIDLINE_DONOR="donor"

./scripts/smoke-test-testnet.sh
```

The script exercises the core contract lifecycle using the Stellar CLI:
1. **Creates a campaign** with two milestones.
2. **Donates tokens** into escrow.
3. **Approves the first milestone** as verifier, releasing payout to the beneficiary.
4. **Cancels the campaign** as creator.
5. **Claims a pro-rata refund** for the donor.

The script prints the outcome of each step, outputs `=== TESTNET SMOKE TEST PASSED ===` upon completion, and exits non-zero on any failure.

## Current testnet deployment

|          |                                                                                                                             |
| -------- | --------------------------------------------------------------------------------------------------------------------------- |
| Contract | `CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK`                                                                  |
| Token    | Native XLM (`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`)                                                     |
| Explorer | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK) |

## Try it from the command line

See [CLI.md](CLI.md) for complete copy-and-paste examples for every contract function.

```sh
# Is an address a verifier?
stellar contract invoke --id $AIDLINE_CONTRACT_ID --network testnet --source deployer \
  -- is_verifier --who $(stellar keys address deployer)

# How many campaigns exist?
stellar contract invoke --id $AIDLINE_CONTRACT_ID --network testnet --source deployer \
  -- campaign_count
```

## Keys and secrets

- The deployer identity becomes the admin. Treat its key as the most sensitive thing in the project.
- Never commit secret keys. `.env.testnet` holds only public values.
- For mainnet, use a hardware wallet or a multisig account as admin.

## Checklist before a mainnet deploy

- [ ] External audit completed and findings resolved
- [ ] Open items in [THREAT_MODEL.md](THREAT_MODEL.md) closed or accepted in writing
- [ ] Admin is a multisig, not a single key
- [ ] Token contract verified (native XLM or a well known issuer)
- [ ] Resource costs regenerated (see [ARCHITECTURE.md](ARCHITECTURE.md))
- [ ] Deployed WASM hash recorded and matches a tagged release
- [ ] Backend and frontend point at the new contract id
