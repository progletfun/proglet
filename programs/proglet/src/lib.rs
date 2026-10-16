pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("PRGKG2z95rbzHWU5EnhQAMFx7V4m94VqziiNeqHiRgE");

declare_program!(pump);
declare_program!(pump_amm);

#[program]
pub mod proglet {
    use super::*;

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        crank_authority: Pubkey,
        treasury: Pubkey,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(ctx, crank_authority, treasury)
    }

    pub fn update_config(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, args)
    }

    pub fn create_token(
        ctx: Context<CreateToken>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_token::handle_create_token(ctx, name, symbol, uri)
    }

    pub fn seed_liquidity(
        ctx: Context<SeedLiquidity>,
        sol_amount: u64,
        token_amount: u64,
    ) -> Result<()> {
        instructions::seed_liquidity::handle_seed_liquidity(ctx, sol_amount, token_amount)
    }

    pub fn crank_buyback(
        ctx: Context<CrankBuyback>,
        spend_lamports: u64,
        min_tokens_out: u64,
    ) -> Result<()> {
        instructions::crank_buyback::handle_crank_buyback(ctx, spend_lamports, min_tokens_out)
    }

    pub fn crank_buyback_amm(
        ctx: Context<CrankBuybackAmm>,
        spend_lamports: u64,
        min_tokens_out: u64,
    ) -> Result<()> {
        instructions::crank_buyback_amm::handle_crank_buyback_amm(
            ctx,
            spend_lamports,
            min_tokens_out,
        )
    }

    pub fn borrow(
        ctx: Context<Borrow>,
        collateral: u64,
        debt: u64,
        max_fee_bps: u64,
    ) -> Result<()> {
        instructions::borrow::handle_borrow(ctx, collateral, debt, max_fee_bps)
    }

    pub fn repay(ctx: Context<Repay>) -> Result<()> {
        instructions::repay::handle_repay(ctx)
    }

    pub fn repay_partial(ctx: Context<RepayPartial>, amount: u64) -> Result<()> {
        instructions::repay_partial::handle_repay_partial(ctx, amount)
    }

    pub fn liquidate(ctx: Context<Liquidate>) -> Result<()> {
        instructions::liquidate::handle_liquidate(ctx)
    }

    pub fn absorb(ctx: Context<Absorb>) -> Result<()> {
        instructions::absorb::handle_absorb(ctx)
    }

    pub fn quote_swap(ctx: Context<QuoteSwap>, is_buy: bool, amount_in: u64) -> Result<SwapQuote> {
        instructions::quote_swap::handle_quote_swap(ctx, is_buy, amount_in)
    }

    pub fn swap(ctx: Context<Swap>, is_buy: bool, amount_in: u64, min_out: u64) -> Result<()> {
        instructions::swap::handle_swap(ctx, is_buy, amount_in, min_out)
    }

    pub fn sweep(ctx: Context<Sweep>) -> Result<()> {
        instructions::sweep::handle_sweep(ctx)
    }
}
