use crate::BPS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapResult {
    pub amount_out: u64,
    pub fee: u64,
    pub team_fee: u64,
    pub sol_reserve_after: u64,
    pub token_reserve_after: u64,
}

fn ceil_div(a: u128, b: u128) -> u128 {
    a.div_ceil(b)
}

fn fees(amount: u64, fee_bps: u64, team_share_bps: u64) -> Option<(u64, u64)> {
    if fee_bps > BPS || team_share_bps > BPS {
        return None;
    }
    let fee = u64::try_from(ceil_div(amount as u128 * fee_bps as u128, BPS as u128)).ok()?;
    let team_fee = u64::try_from(fee as u128 * team_share_bps as u128 / BPS as u128).ok()?;
    Some((fee, team_fee))
}

fn k_not_decreased(sol_before: u64, token_before: u64, sol_after: u64, token_after: u64) -> bool {
    sol_after as u128 * token_after as u128 >= sol_before as u128 * token_before as u128
}

pub fn buy(
    sol_reserve: u64,
    token_reserve: u64,
    sol_in: u64,
    fee_bps: u64,
    team_share_bps: u64,
) -> Option<SwapResult> {
    if sol_reserve == 0 || token_reserve == 0 || sol_in == 0 {
        return None;
    }
    let (fee, team_fee) = fees(sol_in, fee_bps, team_share_bps)?;
    let net_in = sol_in.checked_sub(fee)?;
    let amount_out = u64::try_from(
        token_reserve as u128 * net_in as u128 / (sol_reserve as u128 + net_in as u128),
    )
    .ok()?;
    if amount_out == 0 || amount_out >= token_reserve {
        return None;
    }
    let sol_reserve_after = sol_reserve.checked_add(sol_in - team_fee)?;
    let token_reserve_after = token_reserve - amount_out;
    if !k_not_decreased(
        sol_reserve,
        token_reserve,
        sol_reserve_after,
        token_reserve_after,
    ) {
        return None;
    }
    Some(SwapResult {
        amount_out,
        fee,
        team_fee,
        sol_reserve_after,
        token_reserve_after,
    })
}

pub fn sell(
    sol_reserve: u64,
    token_reserve: u64,
    token_in: u64,
    fee_bps: u64,
    team_share_bps: u64,
) -> Option<SwapResult> {
    if sol_reserve == 0 || token_reserve == 0 || token_in == 0 {
        return None;
    }
    let gross_out = u64::try_from(
        sol_reserve as u128 * token_in as u128 / (token_reserve as u128 + token_in as u128),
    )
    .ok()?;
    let (fee, team_fee) = fees(gross_out, fee_bps, team_share_bps)?;
    let amount_out = gross_out.checked_sub(fee)?;
    if amount_out == 0 {
        return None;
    }
    let sol_reserve_after = sol_reserve.checked_sub(amount_out)?.checked_sub(team_fee)?;
    let token_reserve_after = token_reserve.checked_add(token_in)?;
    if !k_not_decreased(
        sol_reserve,
        token_reserve,
        sol_reserve_after,
        token_reserve_after,
    ) {
        return None;
    }
    Some(SwapResult {
        amount_out,
        fee,
        team_fee,
        sol_reserve_after,
        token_reserve_after,
    })
}

pub fn min_out_with_slippage(quote: u64, slippage_bps: u64) -> u64 {
    let keep = BPS - slippage_bps.min(BPS);
    (quote as u128 * keep as u128 / BPS as u128) as u64
}

pub fn price_impact_bps(
    sol_reserve: u64,
    token_reserve: u64,
    is_buy: bool,
    amount_in: u64,
    amount_out: u64,
) -> u64 {
    if sol_reserve == 0 || token_reserve == 0 || amount_in == 0 {
        return 0;
    }
    let ideal = if is_buy {
        amount_in as u128 * token_reserve as u128 / sol_reserve as u128
    } else {
        amount_in as u128 * sol_reserve as u128 / token_reserve as u128
    };
    if ideal == 0 || amount_out as u128 >= ideal {
        return 0;
    }
    ((ideal - amount_out as u128) * BPS as u128 / ideal) as u64
}

pub fn proportional_amounts(
    sol_reserve: u64,
    token_reserve: u64,
    sol_offered: u64,
    token_offered: u64,
) -> Option<(u64, u64)> {
    if sol_reserve == 0 || token_reserve == 0 {
        return None;
    }
    let token_for_sol = sol_offered as u128 * token_reserve as u128 / sol_reserve as u128;
    if token_for_sol <= token_offered as u128 {
        return Some((sol_offered, token_for_sol as u64));
    }
    let sol_for_token = token_offered as u128 * sol_reserve as u128 / token_reserve as u128;
    Some((sol_for_token as u64, token_offered))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEE: u64 = 100;
    const TEAM: u64 = 2_000;

    #[test]
    fn buy_splits_fee_and_grows_k() {
        let r = buy(100_000_000_000, 1_000_000_000_000, 1_000_000_000, FEE, TEAM).unwrap();
        assert_eq!(r.fee, 10_000_000);
        assert_eq!(r.team_fee, 2_000_000);
        assert_eq!(
            r.sol_reserve_after,
            100_000_000_000 + 1_000_000_000 - 2_000_000
        );
        assert!(
            r.sol_reserve_after as u128 * r.token_reserve_after as u128
                > 100_000_000_000u128 * 1_000_000_000_000
        );
    }

    #[test]
    fn sell_splits_fee_and_grows_k() {
        let r = sell(
            100_000_000_000,
            1_000_000_000_000,
            10_000_000_000,
            FEE,
            TEAM,
        )
        .unwrap();
        let gross = r.amount_out + r.fee;
        assert_eq!(r.fee, gross.div_ceil(100));
        assert_eq!(
            r.sol_reserve_after,
            100_000_000_000 - r.amount_out - r.team_fee
        );
        assert!(
            r.sol_reserve_after as u128 * r.token_reserve_after as u128
                > 100_000_000_000u128 * 1_000_000_000_000
        );
    }

    #[test]
    fn round_trip_loses_value() {
        let (s, t) = (50_000_000_000, 800_000_000_000);
        let b = buy(s, t, 2_000_000_000, FEE, TEAM).unwrap();
        let sl = sell(
            b.sol_reserve_after,
            b.token_reserve_after,
            b.amount_out,
            FEE,
            TEAM,
        )
        .unwrap();
        assert!(sl.amount_out < 2_000_000_000);
    }

    #[test]
    fn rejects_empty_pool_and_dust() {
        assert!(buy(0, 1, 1_000, FEE, TEAM).is_none());
        assert!(buy(1_000_000, 1_000_000, 0, FEE, TEAM).is_none());
        assert!(sell(1_000_000_000, 1_000_000_000_000, 1, FEE, TEAM).is_none());
    }

    #[test]
    fn slippage_and_impact() {
        assert_eq!(min_out_with_slippage(10_000, 100), 9_900);
        assert_eq!(min_out_with_slippage(10_000, 0), 10_000);
        assert_eq!(min_out_with_slippage(10_000, 20_000), 0);
        let (s, t) = (100_000_000_000, 1_000_000_000_000_000);
        let r = buy(s, t, 1_000_000, FEE, TEAM).unwrap();
        assert_eq!(price_impact_bps(s, t, true, 1_000_000, r.amount_out), 100);
        let r = buy(s, t, s / 10, FEE, TEAM).unwrap();
        let impact = price_impact_bps(s, t, true, s / 10, r.amount_out);
        assert!(impact > 900 && impact < 1_000, "impact {impact}");
    }

    #[test]
    fn proportional_keeps_ratio() {
        assert_eq!(proportional_amounts(10, 1_000, 5, 1_000), Some((5, 500)));
        assert_eq!(proportional_amounts(10, 1_000, 5, 100), Some((1, 100)));
    }
}
