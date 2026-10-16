use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::ErrorCode,
    events::Swept,
    state::{GlobalConfig, TeamFeeVault},
};

#[derive(Accounts)]
pub struct Sweep<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = treasury)]
    pub config: Account<'info, GlobalConfig>,
    #[account(mut, seeds = [TEAM_FEE_SEED, team_fee_vault.pool.as_ref()], bump = team_fee_vault.bump)]
    pub team_fee_vault: Account<'info, TeamFeeVault>,
    /// CHECK: must equal `config.treasury`; only receives lamports.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
}

pub fn handle_sweep(ctx: Context<Sweep>) -> Result<()> {
    let vault = ctx.accounts.team_fee_vault.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(vault.data_len());
    let amount = vault.lamports().saturating_sub(rent_floor);
    require!(amount > 0, ErrorCode::NothingToSweep);

    **vault.try_borrow_mut_lamports()? -= amount;
    **ctx
        .accounts
        .treasury
        .to_account_info()
        .try_borrow_mut_lamports()? += amount;

    emit!(Swept {
        pool: ctx.accounts.team_fee_vault.pool,
        amount
    });
    Ok(())
}
