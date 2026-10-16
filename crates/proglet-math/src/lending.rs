use crate::BPS;

pub const MAX_LTV_BPS: u64 = 4_000;
pub const LIQUIDATION_RATIO_BPS: u64 = 15_000;

pub const FEE_MIN_BPS: u64 = 100;
pub const FEE_KINK_BPS: u64 = 300;
pub const FEE_MAX_BPS: u64 = 1_000;
pub const FEE_KINK_UTIL_BPS: u64 = 8_000;

pub const BONUS_MIN_BPS: u64 = 250;
pub const BONUS_MAX_BPS: u64 = 1_500;

pub fn utilization_bps(debt_after: u64, cap: u64) -> u64 {
    if cap == 0 {
        return BPS;
    }
    ((debt_after as u128 * BPS as u128 / cap as u128) as u64).min(BPS)
}

pub fn borrow_fee_bps(util_bps: u64) -> u64 {
    let u = util_bps.min(BPS);
    if u <= FEE_KINK_UTIL_BPS {
        FEE_MIN_BPS + (FEE_KINK_BPS - FEE_MIN_BPS) * u / FEE_KINK_UTIL_BPS
    } else {
        FEE_KINK_BPS
            + (FEE_MAX_BPS - FEE_KINK_BPS) * (u - FEE_KINK_UTIL_BPS) / (BPS - FEE_KINK_UTIL_BPS)
    }
}

pub fn fee_amount(amount: u64, fee_bps: u64) -> u64 {
    (amount as u128 * fee_bps as u128).div_ceil(BPS as u128) as u64
}

pub fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = (n as f64).sqrt() as u128;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

pub fn mul_shr64(a: u128, b: u128) -> Option<u128> {
    let (a1, a0) = (a >> 64, a & u64::MAX as u128);
    let (b1, b0) = (b >> 64, b & u64::MAX as u128);
    let high = a1.checked_mul(b1)?;
    if high > u64::MAX as u128 {
        return None;
    }
    (high << 64)
        .checked_add(a1 * b0)?
        .checked_add(a0 * b1)?
        .checked_add((a0 * b0) >> 64)
}

pub fn virtual_reserves(
    sol_reserve: u64,
    token_reserve: u64,
    price_q64: u128,
) -> Option<(u128, u128)> {
    let k = sol_reserve as u128 * token_reserve as u128;
    let s = isqrt(mul_shr64(k, price_q64)?);
    if s == 0 {
        return None;
    }
    Some((s, k / s))
}

pub fn collateral_value(
    collateral: u64,
    sol_reserve: u64,
    token_reserve: u64,
    price_q64: u128,
) -> Option<u64> {
    let (s, t) = virtual_reserves(sol_reserve, token_reserve, price_q64)?;
    let out = s.checked_mul(collateral as u128)? / (t + collateral as u128);
    u64::try_from(out).ok()
}

pub fn max_borrow(collateral_value: u64) -> u64 {
    (collateral_value as u128 * MAX_LTV_BPS as u128 / BPS as u128) as u64
}

pub fn is_liquidatable(collateral_value: u64, debt: u64) -> bool {
    (collateral_value as u128) * (BPS as u128) < (debt as u128) * (LIQUIDATION_RATIO_BPS as u128)
}

pub fn is_underwater(collateral_value: u64, debt: u64) -> bool {
    collateral_value < debt
}

pub fn liquidation_bonus_bps(value: u64, debt: u64) -> u64 {
    if debt == 0 {
        return BONUS_MIN_BPS;
    }
    let ratio = (value as u128 * BPS as u128 / debt as u128) as u64;
    if ratio >= LIQUIDATION_RATIO_BPS {
        return BONUS_MIN_BPS;
    }
    if ratio <= BPS {
        return BONUS_MAX_BPS;
    }
    BONUS_MIN_BPS
        + (BONUS_MAX_BPS - BONUS_MIN_BPS) * (LIQUIDATION_RATIO_BPS - ratio)
            / (LIQUIDATION_RATIO_BPS - BPS)
}

pub fn tokens_for_sol(sol: u128, price_q64: u128) -> Option<u64> {
    if price_q64 == 0 {
        return None;
    }
    u64::try_from((sol << 64) / price_q64).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::price_q64;

    #[test]
    fn fee_curve() {
        assert_eq!(borrow_fee_bps(0), 100);
        assert_eq!(borrow_fee_bps(4_000), 200);
        assert_eq!(borrow_fee_bps(8_000), 300);
        assert_eq!(borrow_fee_bps(9_000), 650);
        assert_eq!(borrow_fee_bps(10_000), 1_000);
        assert_eq!(borrow_fee_bps(50_000), 1_000);
        assert_eq!(utilization_bps(15, 100), 1_500);
        assert_eq!(utilization_bps(200, 100), 10_000);
        assert_eq!(fee_amount(1_000_001, 100), 10_001);
    }

    #[test]
    fn valuation_follows_the_given_price_not_live_reserves() {
        let (s, t) = (2_000_000_000u64, 20_000_000_000_000u64);
        let spot = price_q64(s, t).unwrap();
        let (vs, vt) = virtual_reserves(s, t, spot).unwrap();
        assert!(vs.abs_diff(s as u128) <= 2 && vt.abs_diff(t as u128) <= t as u128 / 1_000_000_000);
        let small = collateral_value(1_000_000_000, s, t, spot).unwrap();
        assert!(small > 99_000 && small <= 100_000, "{small}");
        let quarter = collateral_value(1_000_000_000, s, t, spot / 4).unwrap();
        assert!(quarter.abs_diff(small / 4) <= 2, "{quarter}");
        let (s2, t2) = (s / 2, t * 2);
        let after_dump = collateral_value(1_000_000_000, s2, t2, spot).unwrap();
        assert!(after_dump.abs_diff(small) <= 2, "{after_dump}");
        let v = collateral_value(t, s, t, spot).unwrap();
        assert!(v.abs_diff(s / 2) <= 2, "{v}");
        assert_eq!(isqrt(1 << 100), 1 << 50);
        assert_eq!(mul_shr64(1 << 100, 1 << 80), Some(1 << 116));
        assert_eq!(mul_shr64(3 << 64, 5), Some(15));
        assert_eq!(mul_shr64(u128::MAX, u128::MAX), None);
        assert_eq!(isqrt(99), 9);
    }

    #[test]
    fn health_and_bonus() {
        assert_eq!(max_borrow(1_000), 400);
        assert!(!is_liquidatable(150, 100));
        assert!(is_liquidatable(149, 100));
        assert!(is_underwater(99, 100) && !is_underwater(100, 100));
        assert_eq!(liquidation_bonus_bps(200, 100), BONUS_MIN_BPS);
        assert_eq!(liquidation_bonus_bps(150, 100), BONUS_MIN_BPS);
        assert_eq!(liquidation_bonus_bps(125, 100), 875);
        assert_eq!(liquidation_bonus_bps(100, 100), BONUS_MAX_BPS);
        assert_eq!(liquidation_bonus_bps(50, 100), BONUS_MAX_BPS);
        let p = price_q64(1, 10_000).unwrap();
        assert_eq!(tokens_for_sol(1, p), Some(10_000));
    }
}
