use anchor_lang::prelude::*;
use proglet_math::{lending, oracle, swap::proportional_amounts, BPS};

use crate::constants::{BORROW_CAP_BPS, MIN_POOL_SOL_FOR_LENDING, ORACLE_LEN};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace)]
pub struct Observation {
    pub slot: u64,
    pub price_q64: u128,
    pub cumulative: u128,
}

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub sol_reserve: u64,
    pub token_reserve: u64,
    pub crank_buffer_sol: u64,
    pub crank_buffer_token: u64,
    pub total_debt: u64,
    pub total_loan_reserve_token: u64,
    pub created_slot: u64,
    pub last_swap_slot: u64,
    pub oracle_head: u8,
    pub observations: [Observation; ORACLE_LEN],
    pub bump: u8,
    pub creator_bump: u8,
    pub team_fee_bump: u8,
    pub total_collateral: u64,
}

impl Pool {
    pub fn effective_sol(&self) -> u64 {
        self.sol_reserve.saturating_add(self.total_debt)
    }

    pub fn borrow_cap(&self) -> u64 {
        (self.effective_sol() as u128 * BORROW_CAP_BPS as u128 / BPS as u128) as u64
    }

    pub fn collateral_value(&self, collateral: u64, price_q64: u128) -> Option<u64> {
        lending::collateral_value(collateral, self.sol_reserve, self.token_reserve, price_q64)
    }

    pub fn return_liquidity(&mut self, sol: u64, token: u64) {
        self.crank_buffer_sol = self.crank_buffer_sol.saturating_add(sol);
        self.crank_buffer_token = self.crank_buffer_token.saturating_add(token);
        self.fold_buffers();
    }

    pub fn fold_buffers(&mut self) {
        if self.sol_reserve == 0 || self.token_reserve == 0 {
            return;
        }
        if let Some((s, t)) = proportional_amounts(
            self.sol_reserve,
            self.token_reserve,
            self.crank_buffer_sol,
            self.crank_buffer_token,
        ) {
            if s > 0 && t > 0 {
                self.sol_reserve += s;
                self.token_reserve += t;
                self.crank_buffer_sol -= s;
                self.crank_buffer_token -= t;
            }
        }
    }

    pub fn lending_open(&self) -> bool {
        self.sol_reserve >= MIN_POOL_SOL_FOR_LENDING
    }

    pub fn spot_price_q64(&self) -> Option<u128> {
        oracle::price_q64(self.sol_reserve, self.token_reserve)
    }

    fn observations_math(&self) -> [oracle::Observation; ORACLE_LEN] {
        self.observations.map(|o| oracle::Observation {
            slot: o.slot,
            price_q64: o.price_q64,
            cumulative: o.cumulative,
        })
    }

    pub fn record_observation(&mut self, slot: u64) {
        let Some(price) = self.spot_price_q64() else {
            return;
        };
        let mut obs = self.observations_math();
        let head = oracle::record(&mut obs, self.oracle_head as usize, slot, price);
        self.oracle_head = head as u8;
        self.observations[head] = Observation {
            slot: obs[head].slot,
            price_q64: obs[head].price_q64,
            cumulative: obs[head].cumulative,
        };
    }

    pub fn safe_price_q64(&self, current_slot: u64, min_age: u64) -> Option<u128> {
        oracle::safe_price(
            &self.observations_math(),
            self.oracle_head as usize,
            self.spot_price_q64()?,
            self.last_swap_slot,
            current_slot,
            min_age,
        )
    }
}
