// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

use soroban_sdk::{contracterror, contracttype, Address, String};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ZeroAmount = 4,
    StrategyNotFound = 5,
    NoPosition = 6,
    InsufficientBalance = 7,
    StrategyPaused = 8,
    /// APY exceeds 50 000 bps (500%).
    InvalidApy = 9,
    EmptyName = 10,
    ContractPaused = 11,
    /// Allocation weights must sum to 10 000 bps.
    InvalidWeights = 12,
    /// Backtest duration must be > 0.
    InvalidDuration = 13,
}

/// A yield strategy registered in the optimizer.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Strategy {
    pub name: String,
    /// APY in basis points (1 bps = 0.01%). Max 50 000 (500%).
    pub apy_bps: u32,
    pub total_deposited: i128,
    pub is_active: bool,
    pub last_compound_ts: u64,
}

/// Per-user position in a single strategy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub deposited: i128,
    pub compounded_balance: i128,
    pub last_update_ts: u64,
}

/// Allocation weight for a strategy in a portfolio (bps, must sum to 10 000).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Allocation {
    pub strategy_id: u32,
    /// Weight in basis points (e.g. 5000 = 50%).
    pub weight_bps: u32,
}

/// Result of a single-strategy backtest simulation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BacktestResult {
    pub strategy_id: u32,
    pub initial_amount: i128,
    pub final_amount: i128,
    /// Net gain = final - initial.
    pub gain: i128,
    /// Effective APY achieved in bps.
    pub effective_apy_bps: u32,
    pub duration_secs: u64,
}

#[contracttype]
pub enum InstanceKey {
    Admin,
    StrategyCount,
    Paused,
}

#[contracttype]
pub enum DataKey {
    Strategy(u32),
    Position(u32, Address),
}
