use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Signer is not the config admin")]
    NotAdmin,
    #[msg("Signer is not the crank authority")]
    NotCrankAuthority,
    #[msg("Pool has no liquidity yet")]
    EmptyPool,
    #[msg("Swap amount too small or exceeds the pool")]
    InvalidSwapAmount,
    #[msg("Output is below min_out")]
    SlippageExceeded,
    #[msg("Liquidity amount is zero after keeping the pool ratio")]
    ZeroLiquidity,
    #[msg("Nothing to sweep")]
    NothingToSweep,
    #[msg("Token amount received by the vault differs from the amount sent")]
    TransferAmountMismatch,
    #[msg("Buyback spend is zero or above the hard cap")]
    InvalidBuybackAmount,
    #[msg("Creator PDA does not hold enough SOL")]
    InsufficientCreatorFunds,
    #[msg("Buyback received no tokens")]
    NoTokensBought,
    #[msg("pump.fun buyback price deviates too far from the pool's safe price")]
    BuybackPriceDeviation,
    #[msg("Not the coin's canonical PumpSwap pool")]
    NotCanonicalPool,
    #[msg("New loans are paused")]
    BorrowPaused,
    #[msg("Pool is below the minimum depth for lending")]
    LendingClosed,
    #[msg("Pool is too new for lending")]
    LendingCooldown,
    #[msg("Loan is below the minimum size")]
    DebtTooSmall,
    #[msg("Loan exceeds the max loan-to-value")]
    LtvExceeded,
    #[msg("Loan would exceed the pool's borrow cap")]
    BorrowCapExceeded,
    #[msg("Fee is above the caller's max_fee_bps")]
    FeeAboveMax,
    #[msg("Loan is too new to repay")]
    RepayTooEarly,
    #[msg("Position is healthy")]
    NotLiquidatable,
    #[msg("Collateral still covers the debt")]
    NotUnderwater,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Nothing to add: collateral and debt are both zero")]
    NothingToBorrow,
    #[msg("Partial repay must be above zero and below the debt; repay in full instead")]
    InvalidRepayAmount,
}
