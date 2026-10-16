use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use proglet_math::swap::proportional_amounts;

use crate::{
    constants::*,
    error::ErrorCode,
    events::LiquidityAdded,
    state::{GlobalConfig, Pool},
};

#[derive(Accounts)]
pub struct SeedLiquidity<'info> {
    #[account(mut)]
    pub crank_authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = crank_authority @ ErrorCode::NotCrankAuthority
    )]
    pub config: Account<'info, GlobalConfig>,
    #[account(mut, seeds = [POOL_SEED, mint.key().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(address = pool.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub pool_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::authority = crank_authority, token::token_program = token_program)]
    pub crank_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_seed_liquidity(
    ctx: Context<SeedLiquidity>,
    sol_amount: u64,
    token_amount: u64,
) -> Result<()> {
    let pool = &ctx.accounts.pool;
    let bootstrap = pool.sol_reserve == 0 || pool.token_reserve == 0;
    let (sol_in, token_in) = if bootstrap {
        (sol_amount, token_amount)
    } else {
        proportional_amounts(
            pool.sol_reserve,
            pool.token_reserve,
            sol_amount,
            token_amount,
        )
        .ok_or(ErrorCode::MathOverflow)?
    };
    require!(sol_in > 0 && token_in > 0, ErrorCode::ZeroLiquidity);

    let vault_before = ctx.accounts.pool_token_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.crank_token_account.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.pool_token_vault.to_account_info(),
                authority: ctx.accounts.crank_authority.to_account_info(),
            },
        ),
        token_in,
        ctx.accounts.mint.decimals,
    )?;
    ctx.accounts.pool_token_vault.reload()?;
    require!(
        ctx.accounts
            .pool_token_vault
            .amount
            .checked_sub(vault_before)
            == Some(token_in),
        ErrorCode::TransferAmountMismatch
    );
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.crank_authority.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        sol_in,
    )?;

    let slot = Clock::get()?.slot;
    let pool = &mut ctx.accounts.pool;
    pool.sol_reserve = pool
        .sol_reserve
        .checked_add(sol_in)
        .ok_or(ErrorCode::MathOverflow)?;
    pool.token_reserve = pool
        .token_reserve
        .checked_add(token_in)
        .ok_or(ErrorCode::MathOverflow)?;
    if bootstrap {
        pool.created_slot = slot;
        pool.record_observation(slot);
    }

    emit!(LiquidityAdded {
        pool: pool.key(),
        sol_amount: sol_in,
        token_amount: token_in,
        sol_reserve: pool.sol_reserve,
        token_reserve: pool.token_reserve,
    });
    Ok(())
}
