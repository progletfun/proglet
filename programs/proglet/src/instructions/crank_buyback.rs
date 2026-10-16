use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    token::Token,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use proglet_math::{oracle::sol_value, swap::proportional_amounts, BPS};

use crate::{
    constants::*,
    error::ErrorCode,
    events::{BuybackExecuted, LiquidityAdded},
    pump,
    state::{GlobalConfig, Pool},
};

#[derive(Accounts)]
pub struct CrankBuyback<'info> {
    #[account(mut)]
    pub crank_authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = crank_authority @ ErrorCode::NotCrankAuthority
    )]
    pub config: Box<Account<'info, GlobalConfig>>,
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
    #[account(mut, seeds = [CREATOR_SEED, mint.key().as_ref()], bump = pool.creator_bump)]
    pub creator: SystemAccount<'info>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = creator,
        associated_token::token_program = token_program
    )]
    pub creator_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: pump PDA `["global"]`.
    pub pump_global: UncheckedAccount<'info>,
    /// CHECK: SOL-paired coins pass wrapped SOL.
    #[account(address = WSOL_MINT)]
    pub quote_mint: UncheckedAccount<'info>,
    pub quote_token_program: Program<'info, Token>,
    /// CHECK: pump PDA `["bonding-curve", mint]`.
    #[account(mut, seeds = [b"bonding-curve", mint.key().as_ref()], bump, seeds::program = pump::ID)]
    pub bonding_curve: UncheckedAccount<'info>,
    /// CHECK: bonding curve's token ATA, checked by pump.
    #[account(mut)]
    pub associated_base_bonding_curve: UncheckedAccount<'info>,
    /// CHECK: unused for SOL-paired coins; derived address, checked by pump.
    #[account(mut)]
    pub associated_quote_bonding_curve: UncheckedAccount<'info>,
    /// CHECK: creator PDA's WSOL ATA address; unused for SOL-paired coins.
    #[account(mut)]
    pub associated_quote_user: UncheckedAccount<'info>,
    /// CHECK: pump PDA `["user_volume_accumulator", creator]`; pump creates it
    #[account(mut)]
    pub user_volume_accumulator: UncheckedAccount<'info>,
    /// CHECK: pump fees PDA, checked by pump.
    pub fee_config: UncheckedAccount<'info>,
    /// CHECK: one of pump's buyback fee recipients, checked by pump.
    #[account(mut)]
    pub buyback_fee_recipient: UncheckedAccount<'info>,
    /// CHECK: pump event authority PDA, checked by pump.
    pub pump_event_authority: UncheckedAccount<'info>,
    /// CHECK: the pump program itself.
    #[account(address = pump::ID)]
    pub pump_program: UncheckedAccount<'info>,

    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_crank_buyback(
    mut ctx: Context<CrankBuyback>,
    spend_lamports: u64,
    min_tokens_out: u64,
) -> Result<()> {
    require!(
        spend_lamports > 0 && spend_lamports <= MAX_CRANK_SPEND_LAMPORTS,
        ErrorCode::InvalidBuybackAmount
    );
    let rent_min = Rent::get()?.minimum_balance(0);
    require!(
        ctx.accounts.creator.lamports() >= rent_min.saturating_add(spend_lamports),
        ErrorCode::InsufficientCreatorFunds
    );

    let mint_key = ctx.accounts.mint.key();
    let creator_seeds: &[&[u8]] = &[
        CREATOR_SEED,
        mint_key.as_ref(),
        &[ctx.accounts.pool.creator_bump],
    ];
    let signer = &[creator_seeds];

    let tokens_before = ctx.accounts.creator_token_account.amount;
    {
        let a = &ctx.accounts;
        pump::cpi::buy_exact_quote_in_v3(
            CpiContext::new_with_signer(
                pump::ID,
                pump::cpi::accounts::BuyExactQuoteInV3 {
                    global: a.pump_global.to_account_info(),
                    base_mint: a.mint.to_account_info(),
                    quote_mint: a.quote_mint.to_account_info(),
                    base_token_program: a.token_program.to_account_info(),
                    quote_token_program: a.quote_token_program.to_account_info(),
                    bonding_curve: a.bonding_curve.to_account_info(),
                    associated_base_bonding_curve: a
                        .associated_base_bonding_curve
                        .to_account_info(),
                    associated_quote_bonding_curve: a
                        .associated_quote_bonding_curve
                        .to_account_info(),
                    user: a.creator.to_account_info(),
                    associated_base_user: a.creator_token_account.to_account_info(),
                    associated_quote_user: a.associated_quote_user.to_account_info(),
                    user_volume_accumulator: a.user_volume_accumulator.to_account_info(),
                    fee_config: a.fee_config.to_account_info(),
                    buyback_fee_recipient: a.buyback_fee_recipient.to_account_info(),
                    system_program: a.system_program.to_account_info(),
                    event_authority: a.pump_event_authority.to_account_info(),
                    program: a.pump_program.to_account_info(),
                },
                signer,
            ),
            spend_lamports,
            min_tokens_out,
            pump::types::OptionBool(false),
        )?;
    }
    ctx.accounts.creator_token_account.reload()?;
    let tokens_bought = ctx
        .accounts
        .creator_token_account
        .amount
        .checked_sub(tokens_before)
        .ok_or(ErrorCode::MathOverflow)?;

    let a = &mut ctx.accounts;
    add_bought_liquidity(
        &mut a.pool,
        &a.mint,
        &mut a.pool_token_vault,
        &a.creator_token_account,
        a.creator.to_account_info(),
        a.token_program.to_account_info(),
        a.system_program.to_account_info(),
        signer,
        spend_lamports,
        tokens_bought,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn add_bought_liquidity<'info>(
    pool: &mut Account<'info, Pool>,
    mint: &InterfaceAccount<'info, Mint>,
    pool_token_vault: &mut InterfaceAccount<'info, TokenAccount>,
    creator_token_account: &InterfaceAccount<'info, TokenAccount>,
    creator: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    signer: &[&[&[u8]]],
    spend_lamports: u64,
    tokens_bought: u64,
) -> Result<()> {
    require!(tokens_bought > 0, ErrorCode::NoTokensBought);
    let rent_min = Rent::get()?.minimum_balance(0);
    let slot = Clock::get()?.slot;

    let bootstrap = pool.sol_reserve == 0 || pool.token_reserve == 0;
    if !bootstrap {
        let safe = pool
            .safe_price_q64(slot, SAFE_PRICE_MIN_AGE_SLOTS)
            .ok_or(ErrorCode::EmptyPool)?;
        let value = sol_value(tokens_bought, safe).ok_or(ErrorCode::MathOverflow)?;
        require!(
            value as u128 * BPS as u128
                >= spend_lamports as u128 * (BPS - MAX_BUYBACK_DEVIATION_BPS) as u128,
            ErrorCode::BuybackPriceDeviation
        );
    }

    let available_sol = creator.lamports().saturating_sub(rent_min);
    let available_tokens = creator_token_account.amount;
    let (sol_in, token_in) = if bootstrap {
        let sol_in = available_sol.min(spend_lamports);
        let token_in = (tokens_bought as u128 * sol_in as u128 / spend_lamports as u128) as u64;
        (sol_in, token_in)
    } else {
        proportional_amounts(
            pool.sol_reserve,
            pool.token_reserve,
            available_sol,
            available_tokens,
        )
        .ok_or(ErrorCode::MathOverflow)?
    };
    require!(sol_in > 0 && token_in > 0, ErrorCode::ZeroLiquidity);

    let vault_before = pool_token_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program.key(),
            TransferChecked {
                from: creator_token_account.to_account_info(),
                mint: mint.to_account_info(),
                to: pool_token_vault.to_account_info(),
                authority: creator.clone(),
            },
            signer,
        ),
        token_in,
        mint.decimals,
    )?;
    pool_token_vault.reload()?;
    require!(
        pool_token_vault.amount.checked_sub(vault_before) == Some(token_in),
        ErrorCode::TransferAmountMismatch
    );
    system_program::transfer(
        CpiContext::new_with_signer(
            system_program.key(),
            system_program::Transfer {
                from: creator,
                to: pool.to_account_info(),
            },
            signer,
        ),
        sol_in,
    )?;

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
    } else {
        pool.fold_buffers();
    }

    emit!(BuybackExecuted {
        pool: pool.key(),
        sol_spent: spend_lamports,
        tokens_bought
    });
    emit!(LiquidityAdded {
        pool: pool.key(),
        sol_amount: sol_in,
        token_amount: token_in,
        sol_reserve: pool.sol_reserve,
        token_reserve: pool.token_reserve,
    });
    Ok(())
}
