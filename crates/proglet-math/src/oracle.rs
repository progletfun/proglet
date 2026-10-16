#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observation {
    pub slot: u64,
    pub price_q64: u128,
    pub cumulative: u128,
}

pub fn price_q64(sol_reserve: u64, token_reserve: u64) -> Option<u128> {
    if token_reserve == 0 {
        return None;
    }
    Some(((sol_reserve as u128) << 64) / token_reserve as u128)
}

pub fn sol_value(token_amount: u64, price_q64: u128) -> Option<u64> {
    let v = (token_amount as u128).checked_mul(price_q64)? >> 64;
    u64::try_from(v).ok()
}

pub fn record(obs: &mut [Observation], head: usize, slot: u64, pre_swap_price_q64: u128) -> usize {
    let last = obs[head];
    if last.slot != 0 && slot <= last.slot {
        return head;
    }
    let cumulative = if last.slot == 0 {
        0
    } else {
        last.cumulative
            .wrapping_add(pre_swap_price_q64.wrapping_mul((slot - last.slot) as u128))
    };
    let next = if last.slot == 0 {
        head
    } else {
        (head + 1) % obs.len()
    };
    obs[next] = Observation {
        slot,
        price_q64: pre_swap_price_q64,
        cumulative,
    };
    next
}

pub fn safe_price(
    obs: &[Observation],
    head: usize,
    spot_q64: u128,
    last_swap_slot: u64,
    current_slot: u64,
    min_age: u64,
) -> Option<u128> {
    if current_slot >= last_swap_slot.saturating_add(min_age) {
        return Some(spot_q64);
    }
    let cutoff = current_slot.checked_sub(min_age)?;
    (0..obs.len())
        .map(|i| obs[(head + obs.len() - i) % obs.len()])
        .find(|o| o.slot != 0 && o.slot <= cutoff)
        .map(|o| o.price_q64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_observation_per_slot() {
        let mut obs = [Observation::default(); 4];
        let mut head = record(&mut obs, 0, 100, 10);
        head = record(&mut obs, head, 100, 999);
        assert_eq!(head, 0);
        assert_eq!(obs[0].price_q64, 10);
        head = record(&mut obs, head, 105, 20);
        assert_eq!(head, 1);
        assert_eq!(obs[1].cumulative, 20 * 5);
    }

    #[test]
    fn ring_wraps() {
        let mut obs = [Observation::default(); 3];
        let mut head = 0;
        for s in 1..=5 {
            head = record(&mut obs, head, s, s as u128);
        }
        assert_eq!(obs[head].slot, 5);
    }

    #[test]
    fn safe_price_ignores_recent_pump() {
        let mut obs = [Observation::default(); 8];
        let mut head = record(&mut obs, 0, 100, 1_000);
        for s in 195..200 {
            head = record(&mut obs, head, s, 50_000);
        }
        assert_eq!(safe_price(&obs, head, 50_000, 199, 200, 10), Some(1_000));
    }

    #[test]
    fn sol_value_matches_price() {
        let p = price_q64(10_000_000_000, 100_000_000_000_000).unwrap();
        assert_eq!(sol_value(1_000_000, p), Some(99));
        assert_eq!(sol_value(0, p), Some(0));
    }

    #[test]
    fn quiet_pool_uses_spot() {
        let obs = [Observation::default(); 4];
        assert_eq!(safe_price(&obs, 0, 7, 100, 110, 10), Some(7));
    }
}
