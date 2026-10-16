use anchor_lang::prelude::*;

use crate::{constants::*, program::Proglet, state::GlobalConfig};

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + GlobalConfig::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, GlobalConfig>,
    #[account(constraint = program.programdata_address()? == Some(program_data.key()))]
    pub program: Program<'info, Proglet>,
    #[account(constraint = program_data.upgrade_authority_address == Some(admin.key()))]
    pub program_data: Account<'info, ProgramData>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    crank_authority: Pubkey,
    treasury: Pubkey,
) -> Result<()> {
    ctx.accounts.config.set_inner(GlobalConfig {
        admin: ctx.accounts.admin.key(),
        crank_authority,
        treasury,
        borrow_paused: false,
        bump: ctx.bumps.config,
    });
    Ok(())
}
