use anchor_lang::prelude::*;

use crate::{constants::*, error::ErrorCode, state::GlobalConfig};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UpdateConfigArgs {
    pub admin: Option<Pubkey>,
    pub crank_authority: Option<Pubkey>,
    pub treasury: Option<Pubkey>,
    pub borrow_paused: Option<bool>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ ErrorCode::NotAdmin
    )]
    pub config: Account<'info, GlobalConfig>,
}

pub fn handle_update_config(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
    let config = &mut ctx.accounts.config;
    if let Some(admin) = args.admin {
        config.admin = admin;
    }
    if let Some(crank_authority) = args.crank_authority {
        config.crank_authority = crank_authority;
    }
    if let Some(treasury) = args.treasury {
        config.treasury = treasury;
    }
    if let Some(borrow_paused) = args.borrow_paused {
        config.borrow_paused = borrow_paused;
    }
    Ok(())
}
