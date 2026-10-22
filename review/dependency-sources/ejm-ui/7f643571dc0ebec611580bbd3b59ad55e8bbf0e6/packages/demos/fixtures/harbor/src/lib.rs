//! harbor: order helpers.

pub mod retry;

/// Total of an order in cents.
pub fn order_total(prices: &[u64], quantities: &[u32]) -> u64 {
    prices
        .iter()
        .zip(quantities)
        .map(|(price, qty)| price * u64::from(*qty))
        .sum()
}
