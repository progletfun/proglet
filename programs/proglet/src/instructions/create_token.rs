use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::{self, AssociatedToken},
    token_2022::Token2022,
};

use crate::{
    constants::*,
    events::TokenCreated,
    pump,
    state::{Pool, TeamFeeVault},
};

#[derive(Accounts)]
pub struct CreateToken<'info> {
    #[account(mut)]
    pub mint: Signer<'info>,
    /// CHECK: pump PDA `["mint-authority"]`, checked by pump.
    pub mint_authority: UncheckedAccount<'info>,
    /// CHECK: pump PDA `["bonding-curve", mint]`.
    #[account(mut, seeds = [b"bonding-curve", mint.key().as_ref()], bump, seeds::program = pump::ID)]
    pub bonding_curve: UncheckedAccount<'info>,
    /// CHECK: bonding curve's ATA, created by pump.
    #[account(mut)]
    pub associated_bonding_curve: UncheckedAccount<'info>,
    /// CHECK: pump PDA `["global"]`, checked by pump.
    pub global: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: pump's mayhem program.
    #[account(mut, address = pubkey!("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e"))]
    pub mayhem_program_id: UncheckedAccount<'info>,
    /// CHECK: mayhem PDA, checked by pump.
    pub global_params: UncheckedAccount<'info>,
    /// CHECK: mayhem PDA, checked by pump.
    #[account(mut)]
    pub sol_vault: UncheckedAccount<'info>,
    /// CHECK: mayhem PDA, checked by pump.
    #[account(mut)]
    pub mayhem_state: UncheckedAccount<'info>,
    /// CHECK: mayhem ATA, checked by pump.
    #[account(mut)]
    pub mayhem_token_vault: UncheckedAccount<'info>,
    /// CHECK: pump event authority PDA, checked by pump.
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: the pump program itself.
    #[account(address = pump::ID)]
    pub pump_program: UncheckedAccount<'info>,

    /// CHECK: per-mint creator PDA passed to pump as `creator`. A data-less
    #[account(mut, seeds = [CREATOR_SEED, mint.key().as_ref()], bump)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: creator PDA's token ATA (buyback tokens land here); created in
    #[account(mut)]
    pub creator_token_account: UncheckedAccount<'info>,
    #[account(
        init,
        payer = user,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED, mint.key().as_ref()],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        init,
        payer = user,
        space = 8 + TeamFeeVault::INIT_SPACE,
        seeds = [TEAM_FEE_SEED, pool.key().as_ref()],
        bump
    )]
    pub team_fee_vault: Box<Account<'info, TeamFeeVault>>,
    /// CHECK: pool's token ATA; created in the handler once the mint exists.
    #[account(mut)]
    pub pool_token_vault: UncheckedAccount<'info>,
}

pub fn handle_create_token(
    ctx: Context<CreateToken>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    let a = &ctx.accounts;

    pump::cpi::create_v2(
        CpiContext::new(
            pump::ID,
            pump::cpi::accounts::CreateV2 {
                mint: a.mint.to_account_info(),
                mint_authority: a.mint_authority.to_account_info(),
                bonding_curve: a.bonding_curve.to_account_info(),
                associated_bonding_curve: a.associated_bonding_curve.to_account_info(),
                global: a.global.to_account_info(),
                user: a.user.to_account_info(),
                system_program: a.system_program.to_account_info(),
                token_program: a.token_program.to_account_info(),
                associated_token_program: a.associated_token_program.to_account_info(),
                mayhem_program_id: a.mayhem_program_id.to_account_info(),
                global_params: a.global_params.to_account_info(),
                sol_vault: a.sol_vault.to_account_info(),
                mayhem_state: a.mayhem_state.to_account_info(),
                mayhem_token_vault: a.mayhem_token_vault.to_account_info(),
                event_authority: a.event_authority.to_account_info(),
                program: a.pump_program.to_account_info(),
            },
        ),
        name,
        symbol,
        uri,
        a.creator.key(),
        false,
        pump::types::OptionBool(false),
        pump::types::OptionU64(0),
        pump::types::OptionBool(false),
    )?;

    for (account, authority) in [
        (&a.pool_token_vault, a.pool.to_account_info()),
        (&a.creator_token_account, a.creator.to_account_info()),
    ] {
        associated_token::create(CpiContext::new(
            associated_token::ID,
            associated_token::Create {
                payer: a.user.to_account_info(),
                associated_token: account.to_account_info(),
                authority,
                mint: a.mint.to_account_info(),
                system_program: a.system_program.to_account_info(),
                token_program: a.token_program.to_account_info(),
            },
        ))?;
    }

    let rent_min = Rent::get()?.minimum_balance(0);
    let missing = rent_min.saturating_sub(a.creator.lamports());
    if missing > 0 {
        system_program::transfer(
            CpiContext::new(
                system_program::ID,
                system_program::Transfer {
                    from: a.user.to_account_info(),
                    to: a.creator.to_account_info(),
                },
            ),
            missing,
        )?;
    }

    let pool_key = ctx.accounts.pool.key();
    let slot = Clock::get()?.slot;
    let pool = &mut ctx.accounts.pool;
    pool.mint = ctx.accounts.mint.key();
    pool.token_program = ctx.accounts.token_program.key();
    pool.created_slot = slot;
    pool.bump = ctx.bumps.pool;
    pool.creator_bump = ctx.bumps.creator;
    pool.team_fee_bump = ctx.bumps.team_fee_vault;

    ctx.accounts.team_fee_vault.pool = pool_key;
    ctx.accounts.team_fee_vault.bump = ctx.bumps.team_fee_vault;

    emit!(TokenCreated {
        mint: ctx.accounts.mint.key(),
        pool: pool_key,
        creator: ctx.accounts.creator.key(),
        user: ctx.accounts.user.key(),
    });
    Ok(())
}
