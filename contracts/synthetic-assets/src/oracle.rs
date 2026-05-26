use soroban_sdk::Env;

use crate::storage::{get_price, set_price};
use crate::types::{Error, PriceData};
use soroban_sdk::Symbol;

pub const PRICE_SCALE: i128 = 100_000_000;

/// Maximum age of a price before it's considered stale (in seconds)
const MAX_PRICE_AGE: u64 = 300; // 5 minutes

/// Minimum confidence level for valid price (0-100)
const MIN_CONFIDENCE: u32 = 50;

/// Update price with validation
pub fn update_price_internal(
    env: &Env,
    asset_symbol: &Symbol,
    new_price: i128,
    confidence: u32,
) -> Result<(), Error> {
    validate_price(new_price, confidence)?;

    let price_data = PriceData {
        price: new_price,
        timestamp: env.ledger().timestamp(),
        confidence,
    };

    set_price(env, asset_symbol, &price_data);
    Ok(())
}

/// Get validated price
pub fn get_price_internal(env: &Env, asset_symbol: &Symbol) -> Result<i128, Error> {
    let price_data = get_price(env, asset_symbol)?;
    
    // Check if price is stale
    let current_time = env.ledger().timestamp();
    // Use checked_add to avoid overflow in timestamp arithmetic
    match price_data.timestamp.checked_add(MAX_PRICE_AGE) {
        Some(expiry) => {
            if current_time > expiry {
                return Err(Error::StalePrice);
            }
        }
        None => return Err(Error::StalePrice),
    }

    if price_data.confidence < MIN_CONFIDENCE {
        return Err(Error::LowConfidence);
    }

    // Reject invalid price values stored in oracle
    if price_data.price <= 0 {
        return Err(Error::InvalidPrice);
    }

    Ok(price_data.price)
}

/// Validate price data
pub fn validate_price(price: i128, confidence: u32) -> Result<(), Error> {
    if price <= 0 {
        return Err(Error::InvalidPrice);
    }

    if confidence > 100 {
        return Err(Error::InvalidPrice);
    }

    // Use explicit LowConfidence error for too-low oracle confidence
    if confidence < MIN_CONFIDENCE {
        return Err(Error::LowConfidence);
    }

    Ok(())
}

/// Calculate price deviation between two prices (in basis points)
pub fn calculate_price_deviation(old_price: i128, new_price: i128) -> u32 {
    if old_price <= 0 {
        return u32::MAX;
    }

    let diff = if new_price >= old_price {
        match new_price.checked_sub(old_price) {
            Some(v) => v,
            None => return u32::MAX,
        }
    } else {
        match old_price.checked_sub(new_price) {
            Some(v) => v,
            None => return u32::MAX,
        }
    };

    // Multiply then divide with checked ops to avoid overflow
    let scaled = match diff.checked_mul(10000) {
        Some(v) => match v.checked_div(old_price) {
            Some(d) => d,
            None => return u32::MAX,
        },
        None => return u32::MAX,
    };

    if scaled > u32::MAX as i128 {
        u32::MAX
    } else {
        scaled as u32
    }
}

/// Check if price deviation is within acceptable bounds
pub fn is_price_valid_deviation(old_price: i128, new_price: i128, max_deviation: u32) -> bool {
    let dev = calculate_price_deviation(old_price, new_price);
    if dev == u32::MAX {
        return false;
    }
    dev <= max_deviation
}
