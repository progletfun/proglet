use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use proglet_math::{lending, BPS};

use crate::{
    constants::*,
    error::ErrorCode,
    events::Borrowed,
    state::{GlobalConfig, Pool, Position, TeamFeeVault},
};

#[derive(Accounts)]
pub struct Borrow<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, GlobalConfig>>,
    #[account(mut, seeds = [POOL_SEED, mint.key().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [TEAM_FEE_SEED, pool.key().as_ref()], bump = pool.team_fee_bump)]
    pub team_fee_vault: Box<Account<'info, TeamFeeVault>>,
    #[account(address = pool.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub pool_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::authority = owner, token::token_program = token_program)]
    pub owner_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = owner,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_borrow(
    ctx: Context<Borrow>,
    collateral: u64,
    debt: u64,
    max_fee_bps: u64,
) -> Result<()> {
    require!(collateral > 0 || debt > 0, ErrorCode::NothingToBorrow);
    let is_new = ctx.accounts.position.owner == Pubkey::default();
    let slot = Clock::get()?.slot;
    if is_new || debt > 0 {
        require!(!ctx.accounts.config.borrow_paused, ErrorCode::BorrowPaused);
        let pool = &ctx.accounts.pool;
        require!(pool.lending_open(), ErrorCode::LendingClosed);
        require!(
            slot >= pool.created_slot.saturating_add(LENDING_COOLDOWN_SLOTS),
            ErrorCode::LendingCooldown
        );
        require!(debt >= MIN_DEBT_LAMPORTS, ErrorCode::DebtTooSmall);
    }

    if collateral > 0 {
        let vault_before = ctx.accounts.pool_token_vault.amount;
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.owner_token_account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.pool_token_vault.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            collateral,
            ctx.accounts.mint.decimals,
        )?;
        ctx.accounts.pool_token_vault.reload()?;
        require!(
            ctx.accounts
                .pool_token_vault
                .amount
                .checked_sub(vault_before)
                == Some(collateral),
            ErrorCode::TransferAmountMismatch
        );
    }

    let total_collateral = ctx
        .accounts
        .position
        .collateral
        .checked_add(collateral)
        .ok_or(ErrorCode::MathOverflow)?;
    let total_debt = ctx
        .accounts
        .position
        .debt
        .checked_add(debt)
        .ok_or(ErrorCode::MathOverflow)?;

    let (fee, fee_bps, team_fee, loan_reserve_token) = if debt > 0 {
        let pool = &ctx.accounts.pool;
        let spot = pool.spot_price_q64().ok_or(ErrorCode::EmptyPool)?;
        let safe = pool
            .safe_price_q64(slot, SAFE_PRICE_MIN_AGE_SLOTS)
            .ok_or(ErrorCode::EmptyPool)?;
        let value = pool
            .collateral_value(total_collateral, spot.min(safe))
            .ok_or(ErrorCode::MathOverflow)?;
        require!(
            total_debt <= lending::max_borrow(value),
            ErrorCode::LtvExceeded
        );

        let cap = pool.borrow_cap();
        let debt_after = pool
            .total_debt
            .checked_add(debt)
            .ok_or(ErrorCode::MathOverflow)?;
        require!(
            debt_after <= cap && debt < pool.sol_reserve,
            ErrorCode::BorrowCapExceeded
        );
        let fee_bps = lending::borrow_fee_bps(lending::utilization_bps(debt_after, cap));
        require!(fee_bps <= max_fee_bps, ErrorCode::FeeAboveMax);
        let fee = lending::fee_amount(debt, fee_bps);
        let team_fee = (fee as u128 * TEAM_SHARE_BPS as u128 / BPS as u128) as u64;
        let loan_reserve_token =
            (debt as u128 * pool.token_reserve as u128 / pool.sol_reserve as u128) as u64;

        let pool = &mut ctx.accounts.pool;
        pool.sol_reserve = pool.sol_reserve - debt + (fee - team_fee);
        pool.token_reserve -= loan_reserve_token;
        pool.total_debt = debt_after;
        pool.total_loan_reserve_token += loan_reserve_token;
        (fee, fee_bps, team_fee, loan_reserve_token)
    } else {
        (0, 0, 0, 0)
    };
    ctx.accounts.pool.total_collateral += collateral;

    let pool_key = ctx.accounts.pool.key();
    let position = &mut ctx.accounts.position;
    if is_new {
        position.owner = ctx.accounts.owner.key();
        position.pool = pool_key;
        position.bump = ctx.bumps.position;
    }
    position.collateral = total_collateral;
    position.debt = total_debt;
    position.loan_reserve_token += loan_reserve_token;
    if debt > 0 {
        position.opened_slot = slot;
    }

    if debt > 0 {
        let pool_info = ctx.accounts.pool.to_account_info();
        **pool_info.try_borrow_mut_lamports()? -= debt - fee + team_fee;
        **ctx
            .accounts
            .owner
            .to_account_info()
            .try_borrow_mut_lamports()? += debt - fee;
        **ctx
            .accounts
            .team_fee_vault
            .to_account_info()
            .try_borrow_mut_lamports()? += team_fee;
    }

    emit!(Borrowed {
        pool: pool_key,
        owner: ctx.accounts.owner.key(),
        collateral,
        debt,
        fee,
        fee_bps,
        loan_reserve_token,
    });
    Ok(())
}
