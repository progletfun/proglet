use anchor_lang::prelude::*;

pub const CONFIG_SEED: &[u8] = b"config";
pub const POOL_SEED: &[u8] = b"pool";
pub const TEAM_FEE_SEED: &[u8] = b"team_fee";
pub const CREATOR_SEED: &[u8] = b"creator";
pub const POSITION_SEED: &[u8] = b"position";

pub const SWAP_FEE_BPS: u64 = 100;
pub const TEAM_SHARE_BPS: u64 = 2_000;

pub const ORACLE_LEN: usize = 16;
pub const SAFE_PRICE_MIN_AGE_SLOTS: u64 = 10;

pub const MAX_CRANK_SPEND_LAMPORTS: u64 = 50_000_000_000;
pub const MAX_BUYBACK_DEVIATION_BPS: u64 = 1_000;

pub const MIN_POOL_SOL_FOR_LENDING: u64 = 100_000_000;

pub const BORROW_CAP_BPS: u64 = 1_500;
pub const MIN_DEBT_LAMPORTS: u64 = 10_000_000;
pub const MIN_LOAN_SLOTS: u64 = 2;
pub const LENDING_COOLDOWN_SLOTS: u64 = 150;

pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

#[constant]
pub const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
