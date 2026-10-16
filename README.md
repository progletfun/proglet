# Proglet

**Every coin gets a pool that only grows.**

Proglet is a Solana program that launches coins on [pump.fun](https://pump.fun) with a creator account the program owns. Every pump.fun trade pays that creator a fee. Proglet spends the fee buying the coin back and locks what it buys, together with SOL, in the coin's own constant-product pool. Nothing can withdraw that liquidity, not the team and not the program itself. Once a pool is deep enough, holders can borrow SOL from it against their coins, with no interest and no deadline.

| | |
| --- | --- |
| Program ID | `PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE` |
| Status | Live on mainnet · [verified build](https://verify.osec.io/status/PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE) |
| Framework | Anchor 1.2.1, Rust 1.89.0 |
| Website | [proglet.fun](https://proglet.fun) · [docs](https://proglet.fun/docs) |
| SDK | [`@proglet/sdk`](https://www.npmjs.com/package/@proglet/sdk) |
| X | [@ProgletFun](https://x.com/ProgletFun) |

This repository holds the on-chain program, so anyone can read it and check that the deployed binary was built from it (see [Verify the deployed program](#verify-the-deployed-program)).

---

## How it works

```
 pump.fun trades ──► creator fee ──► creator PDA ──► buyback on pump.fun ──► Proglet pool (locked)
                                                                                    │
                                          holders lock coins, borrow SOL ◄──────────┘
```

1. **Launch.** `create_token` creates the coin through pump.fun's `create_v2` with a per-mint PDA, `["creator", mint]`, as its creator. It also creates the coin's Proglet pool and vaults.
2. **Fees.** pump.fun pays creator fees to that PDA, first on the bonding curve, then on PumpSwap after the coin graduates.
3. **Buyback.** The crank claims the fees and calls `crank_buyback` (bonding curve) or `crank_buyback_amm` (PumpSwap). The creator PDA buys the coin, and the tokens plus SOL join the pool at its current ratio, so the buyback never moves the pool's price.
4. **Locked liquidity.** Pools are SOL/token x·y=k pools. There is no remove-liquidity instruction and no LP token, so liquidity can only grow.
5. **Lending.** Holders lock coins in the pool and borrow SOL from it. The only cost is a one-time fee; there is no interest and no term.

## Instructions

| Instruction | Who can call it | What it does |
| --- | --- | --- |
| `initialize_config` | The program's upgrade authority, once | Sets the admin, crank authority and treasury |
| `update_config` | Admin | Hands over the admin role, rotates the crank or treasury; pauses or resumes new loans |
| `create_token` | Anyone | Launches a coin on pump.fun with its creator PDA, pool and vaults |
| `crank_buyback` | Crank authority | Spends creator fees on a bonding-curve buyback and adds the result to the pool |
| `crank_buyback_amm` | Crank authority | The same on the coin's canonical PumpSwap pool |
| `seed_liquidity` | Crank authority | Adds tokens and SOL to a pool at its ratio |
| `swap` | Anyone | Buys or sells against a pool, with a `min_out` slippage guard |
| `quote_swap` | Anyone (read-only) | Returns the exact swap quote as return data |
| `sweep` | Anyone | Pays the pool's team-fee vault to the configured treasury, and nowhere else |
| `borrow` | Anyone | Opens a loan or adds collateral and/or debt to it |
| `repay` / `repay_partial` | The borrower | Repays all or part of a loan and releases the matching collateral |
| `liquidate` | Anyone | Repays an unhealthy loan and takes its collateral plus a bonus |
| `absorb` | Anyone | Writes off a loan whose collateral is worth less than its debt |

Nothing can stop swaps. The admin can pause new loans; repaying and liquidating always work.

## Parameters

| Parameter | Value |
| --- | --- |
| Swap fee | 1%, charged in SOL; 80% stays in the pool, 20% goes to the team-fee vault |
| Max loan-to-value | 40% |
| Liquidation threshold | Collateral below 1.5x the debt, at the safe price |
| Liquidation bonus | 2.5% to 15%, rising as the loan gets less healthy |
| Borrow fee | 1% while the pool's lending capacity is mostly unused, 3% at 80%, 10% at full |
| Pool lending cap | 15% of the pool's SOL across all loans |
| Minimum loan | 0.01 SOL |
| Lending opens | Pool holds at least 0.1 SOL and is at least 150 slots old |
| Buyback cap | 50 SOL per call |
| Buyback price guard | Reverts if pump.fun's price is more than 10% worse than the pool's safe price |

## Safety properties

- **Liquidity can't leave.** The program has no instruction that removes liquidity from a pool. The SOL reserve is the pool account's own lamports. The token reserve sits in the pool's Token-2022 account. Both are tracked by internal counters, never read from balances.
- **Account invariants.** Every handler keeps two invariants. Pool lamports equal rent plus the SOL reserve and the crank buffer. Vault tokens equal the token reserve, the crank buffer, the collateral and the tokens set aside for loans.
- **Manipulation-resistant prices.** An oracle ring buffer records the pool's pre-trade price once per slot. Loans value collateral at the lower of the spot price and a *safe* price that a trade within the last few slots can't move. Collateral is valued against a virtual pool of the same depth, so it's what the tokens would really sell for.
- **Loans don't move the price.** A loan takes its SOL, plus the matching share of tokens, out of the reserves at once.
- **Bounded authority.** The crank can only spend a coin's own creator fees, within the buyback cap and the price guard. The admin can't touch pools or loans. The team fee can only go to the configured treasury.
- **Config can't be hijacked.** Only the program's upgrade authority can call `initialize_config`, so nobody can front-run the setup after deployment.
- **Upgradeable, for now.** Like most new Solana programs, Proglet is deployed as upgradeable so bugs can be fixed. Whoever holds the upgrade authority could change the program. It starts with the deployer, `3ZXh17E3HhE4dSG5UzXVEg8UokLb1ybxeqVYYjQg5J19`. The plan is to move it to a multisig, and you can always check it on chain with `solana program show PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE`.

## Repository layout

```
programs/proglet/      the Anchor program (one file per instruction in src/instructions/)
crates/proglet-math/   pure integer math: swap quotes, fee split, liquidity, oracle, lending
idls/                  pump.fun and PumpSwap IDLs, pinned (used by declare_program!)
scripts/verify-build.sh  the reproducible build
```

`proglet-math` has no Solana dependencies. The program and the TypeScript SDK both use the same math, with the same rounding.

## Build and test

```sh
anchor build --arch v0          # Anchor 1.2.1; writes target/deploy/proglet.so and the IDL
cargo test                      # math unit tests and LiteSVM program tests (build first)
cargo test -p proglet-math      # math only, no SBF build needed
```

Pass `--arch v0`: Anchor 1.2.1 defaults to SBPF v3, which LiteSVM can't load.

The full end-to-end suite runs against the real pump.fun program on a mainnet fork. It covers every instruction, every rejection, the exact on-chain vs. off-chain quotes, and graduation to PumpSwap. It lives with the SDK, outside this repository.

## Verify the deployed program

Mainnet deploys use the reproducible build only, never a plain `anchor build`:

```sh
scripts/verify-build.sh         # solana-verify build in a digest-pinned Docker image
```

The `Verifiable build` GitHub Action runs the same script on an x86 runner. It publishes `proglet.so` and its hash as an artifact.

The mainnet program is verified by OtterSec against commit `90a55d4` ([status](https://verify.osec.io/status/PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE)); its executable hash is `6c6b2cc6fb7bf8419c6327f4fa232d683bd72711e7ac305a35c0eaf768fb6923`. To check it yourself:

```sh
solana-verify verify-from-repo \
  --program-id PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE \
  --library-name proglet \
  --base-image solanafoundation/solana-verifiable-build@sha256:12fd4c0a0790f0fc41ef74b0cdb6bccc167ba6137eb1adb749bac649481c86bd \
  https://github.com/progletfun/proglet
```

## pump.fun IDLs

`idls/pump.json`, `idls/pump_amm.json` and `idls/pump_fees.json` are pinned to [pump-fun/pump-public-docs](https://github.com/pump-fun/pump-public-docs) commit `639b87fb45`. They're refreshed only on purpose, when pump.fun ships a breaking change.
