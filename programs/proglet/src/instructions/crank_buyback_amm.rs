use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    token::{self, CloseAccount, SyncNative, Token},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::*,
    error::ErrorCode,
    instructions::crank_buyback::add_bought_liquidity,
    pump, pump_amm,
    state::{GlobalConfig, Pool},
};

#[derive(Accounts)]
pub struct CrankBuybackAmm<'info> {
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
    #[account(address = WSOL_MINT)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = creator,
        associated_token::token_program = quote_token_program
    )]
    pub creator_wsol_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: canonical PumpSwap pool of `mint`, checked in the handler.
    #[account(mut)]
    pub amm_pool: UncheckedAccount<'info>,
    /// CHECK: PumpSwap PDA `["global_config"]`.
    pub amm_global_config: UncheckedAccount<'info>,
    /// CHECK: `amm_pool.pool_base_token_account`, checked by PumpSwap.
    #[account(mut)]
    pub amm_pool_base_token_account: UncheckedAccount<'info>,
    /// CHECK: `amm_pool.pool_quote_token_account`, checked by PumpSwap.
    #[account(mut)]
    pub amm_pool_quote_token_account: UncheckedAccount<'info>,
    /// CHECK: PumpSwap PDA `["user_volume_accumulator", creator]`; created by
    #[account(mut)]
    pub amm_user_volume_accumulator: UncheckedAccount<'info>,
    /// CHECK: pump fees PDA for PumpSwap, checked by PumpSwap.
    pub amm_fee_config: UncheckedAccount<'info>,
    /// CHECK: WSOL ATA of one of pump's buyback fee recipients, checked by PumpSwap.
    #[account(mut)]
    pub buyback_fee_recipient: UncheckedAccount<'info>,
    /// CHECK: PumpSwap event authority PDA, checked by PumpSwap.
    pub amm_event_authority: UncheckedAccount<'info>,
    /// CHECK: the PumpSwap program itself.
    #[account(address = pump_amm::ID)]
    pub amm_program: UncheckedAccount<'info>,

    pub quote_token_program: Program<'info, Token>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_crank_buyback_amm(
    mut ctx: Context<CrankBuybackAmm>,
    spend_lamports: u64,
    min_tokens_out: u64,
) -> Result<()> {
    require!(
        spend_lamports > 0 && spend_lamports <= MAX_CRANK_SPEND_LAMPORTS,
        ErrorCode::InvalidBuybackAmount
    );
    let mint_key = ctx.accounts.mint.key();

    let pool_authority =
        Pubkey::find_program_address(&[b"pool-authority", mint_key.as_ref()], &pump::ID).0;
    let canonical_pool = Pubkey::find_program_address(
        &[
            b"pool",
            &0u16.to_le_bytes(),
            pool_authority.as_ref(),
            mint_key.as_ref(),
            WSOL_MINT.as_ref(),
        ],
        &pump_amm::ID,
    )
    .0;
    require_keys_eq!(
        ctx.accounts.amm_pool.key(),
        canonical_pool,
        ErrorCode::NotCanonicalPool
    );

    let rent_min = Rent::get()?.minimum_balance(0);
    let wsol = ctx.accounts.creator_wsol_account.amount;
    let native = ctx.accounts.creator.lamports().saturating_sub(rent_min);
    require!(
        wsol.saturating_add(native) >= spend_lamports,
        ErrorCode::InsufficientCreatorFunds
    );

    let creator_seeds: &[&[u8]] = &[
        CREATOR_SEED,
        mint_key.as_ref(),
        &[ctx.accounts.pool.creator_bump],
    ];
    let signer = &[creator_seeds];

    if wsol < spend_lamports {
        system_program::transfer(
            CpiContext::new_with_signer(
                system_program::ID,
                system_program::Transfer {
                    from: ctx.accounts.creator.to_account_info(),
                    to: ctx.accounts.creator_wsol_account.to_account_info(),
                },
                signer,
            ),
            spend_lamports - wsol,
        )?;
        token::sync_native(CpiContext::new(
            ctx.accounts.quote_token_program.key(),
            SyncNative {
                account: ctx.accounts.creator_wsol_account.to_account_info(),
            },
        ))?;
    }

    let tokens_before = ctx.accounts.creator_token_account.amount;
    {
        let a = &ctx.accounts;
        pump_amm::cpi::buy_exact_quote_in_v2(
            CpiContext::new_with_signer(
                pump_amm::ID,
                pump_amm::cpi::accounts::BuyExactQuoteInV2 {
                    pool: a.amm_pool.to_account_info(),
                    user: a.creator.to_account_info(),
                    global_config: a.amm_global_config.to_account_info(),
                    base_mint: a.mint.to_account_info(),
                    quote_mint: a.quote_mint.to_account_info(),
                    user_base_token_account: a.creator_token_account.to_account_info(),
                    user_quote_token_account: a.creator_wsol_account.to_account_info(),
                    pool_base_token_account: a.amm_pool_base_token_account.to_account_info(),
                    pool_quote_token_account: a.amm_pool_quote_token_account.to_account_info(),
                    base_token_program: a.token_program.to_account_info(),
                    quote_token_program: a.quote_token_program.to_account_info(),
                    system_program: a.system_program.to_account_info(),
                    user_volume_accumulator: a.amm_user_volume_accumulator.to_account_info(),
                    fee_config: a.amm_fee_config.to_account_info(),
                    buyback_fee_recipient: a.buyback_fee_recipient.to_account_info(),
                    event_authority: a.amm_event_authority.to_account_info(),
                    program: a.amm_program.to_account_info(),
                },
                signer,
            ),
            spend_lamports,
            min_tokens_out.max(1),
        )?;
    }
    ctx.accounts.creator_token_account.reload()?;
    let tokens_bought = ctx
        .accounts
        .creator_token_account
        .amount
        .checked_sub(tokens_before)
        .ok_or(ErrorCode::MathOverflow)?;

    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.quote_token_program.key(),
        CloseAccount {
            account: ctx.accounts.creator_wsol_account.to_account_info(),
            destination: ctx.accounts.creator.to_account_info(),
            authority: ctx.accounts.creator.to_account_info(),
        },
        signer,
    ))?;

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
