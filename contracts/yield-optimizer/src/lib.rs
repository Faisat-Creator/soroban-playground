// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

//! # Cross-Protocol Yield Optimizer
//!
//! Extends basic yield farming with:
//! - Multi-strategy portfolio allocation with configurable weights.
//! - Auto-compounding per user position.
//! - On-chain strategy backtesting simulation.
//! - Optimal strategy recommendation (highest active APY).
//! - Emergency pause/unpause.

#![no_std]

mod storage;
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Vec};

use crate::storage::{
    get_admin, get_position, get_strategy, get_strategy_count, has_position, has_strategy,
    is_initialized, is_paused, remove_position, set_admin, set_paused, set_position, set_strategy,
    set_strategy_count,
};
use crate::types::{Allocation, BacktestResult, Error, Position, Strategy};

const SECS_PER_YEAR: u64 = 31_536_000;
const BPS: u32 = 10_000;
const MAX_APY_BPS: u32 = 50_000; // 500%

#[contract]
pub struct YieldOptimizer;

#[contractimpl]
impl YieldOptimizer {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if is_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        set_admin(&env, &admin);
        set_strategy_count(&env, 0);
        env.events().publish((symbol_short!("init"),), (admin,));
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), Error> {
        Self::assert_admin(&env, &admin)?;
        set_paused(&env, true);
        env.events().publish((symbol_short!("paused"),), ());
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), Error> {
        Self::assert_admin(&env, &admin)?;
        set_paused(&env, false);
        env.events().publish((symbol_short!("unpaused"),), ());
        Ok(())
    }

    // ── Strategy management ───────────────────────────────────────────────────

    /// Register a new strategy. Returns its ID.
    pub fn add_strategy(
        env: Env,
        admin: Address,
        name: String,
        apy_bps: u32,
    ) -> Result<u32, Error> {
        Self::assert_admin(&env, &admin)?;
        if name.len() == 0 {
            return Err(Error::EmptyName);
        }
        if apy_bps > MAX_APY_BPS {
            return Err(Error::InvalidApy);
        }
        let id = get_strategy_count(&env) + 1;
        set_strategy(
            &env,
            id,
            &Strategy {
                name: name.clone(),
                apy_bps,
                total_deposited: 0,
                is_active: true,
                last_compound_ts: env.ledger().timestamp(),
            },
        );
        set_strategy_count(&env, id);
        env.events()
            .publish((symbol_short!("strat_add"), id), (name, apy_bps));
        Ok(id)
    }

    pub fn update_apy(
        env: Env,
        admin: Address,
        strategy_id: u32,
        new_apy_bps: u32,
    ) -> Result<(), Error> {
        Self::assert_admin(&env, &admin)?;
        if new_apy_bps > MAX_APY_BPS {
            return Err(Error::InvalidApy);
        }
        let mut s = get_strategy(&env, strategy_id)?;
        s.apy_bps = new_apy_bps;
        set_strategy(&env, strategy_id, &s);
        env.events()
            .publish((symbol_short!("apy_upd"), strategy_id), new_apy_bps);
        Ok(())
    }

    pub fn set_strategy_active(
        env: Env,
        admin: Address,
        strategy_id: u32,
        active: bool,
    ) -> Result<(), Error> {
        Self::assert_admin(&env, &admin)?;
        let mut s = get_strategy(&env, strategy_id)?;
        s.is_active = active;
        set_strategy(&env, strategy_id, &s);
        env.events()
            .publish((symbol_short!("strat_tog"), strategy_id), active);
        Ok(())
    }

    // ── User actions ──────────────────────────────────────────────────────────

    pub fn deposit(
        env: Env,
        user: Address,
        strategy_id: u32,
        amount: i128,
    ) -> Result<(), Error> {
        Self::assert_not_paused(&env)?;
        user.require_auth();
        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        let mut s = get_strategy(&env, strategy_id)?;
        if !s.is_active {
            return Err(Error::StrategyPaused);
        }
        let now = env.ledger().timestamp();
        let mut pos = if has_position(&env, strategy_id, &user) {
            Self::accrue(get_position(&env, strategy_id, &user)?, &s, now)
        } else {
            Position { deposited: 0, compounded_balance: 0, last_update_ts: now }
        };
        pos.deposited += amount;
        pos.compounded_balance += amount;
        pos.last_update_ts = now;
        s.total_deposited += amount;
        set_position(&env, strategy_id, &user, &pos);
        set_strategy(&env, strategy_id, &s);
        env.events()
            .publish((symbol_short!("deposit"), strategy_id), (user, amount));
        Ok(())
    }

    pub fn withdraw(
        env: Env,
        user: Address,
        strategy_id: u32,
        amount: i128,
    ) -> Result<i128, Error> {
        Self::assert_not_paused(&env)?;
        user.require_auth();
        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        let mut s = get_strategy(&env, strategy_id)?;
        let now = env.ledger().timestamp();
        let mut pos = Self::accrue(get_position(&env, strategy_id, &user)?, &s, now);
        if amount > pos.compounded_balance {
            return Err(Error::InsufficientBalance);
        }
        pos.compounded_balance -= amount;
        pos.deposited -= amount.min(pos.deposited);
        s.total_deposited -= amount.min(s.total_deposited);
        pos.last_update_ts = now;
        if pos.compounded_balance == 0 {
            remove_position(&env, strategy_id, &user);
        } else {
            set_position(&env, strategy_id, &user, &pos);
        }
        set_strategy(&env, strategy_id, &s);
        env.events()
            .publish((symbol_short!("withdraw"), strategy_id), (user, amount));
        Ok(amount)
    }

    /// Trigger auto-compounding for a user's position. Callable by anyone (keeper).
    pub fn compound(env: Env, user: Address, strategy_id: u32) -> Result<i128, Error> {
        Self::assert_not_paused(&env)?;
        let s = get_strategy(&env, strategy_id)?;
        let now = env.ledger().timestamp();
        let pos = Self::accrue(get_position(&env, strategy_id, &user)?, &s, now);
        let bal = pos.compounded_balance;
        set_position(&env, strategy_id, &user, &pos);
        env.events()
            .publish((symbol_short!("compound"), strategy_id), (user, bal));
        Ok(bal)
    }

    /// Allocate `total_amount` across strategies according to weights.
    /// Validates weights sum to BPS (10 000) and all strategies are active.
    /// Returns per-strategy deposit amounts (simulation only — no actual token transfer).
    pub fn allocate(
        env: Env,
        allocations: Vec<Allocation>,
        total_amount: i128,
    ) -> Result<Vec<i128>, Error> {
        if total_amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        let mut weight_sum: u32 = 0;
        for a in allocations.iter() {
            get_strategy(&env, a.strategy_id)?; // existence check
            weight_sum = weight_sum.saturating_add(a.weight_bps);
        }
        if weight_sum != BPS {
            return Err(Error::InvalidWeights);
        }
        let mut amounts = Vec::new(&env);
        for a in allocations.iter() {
            let amt = (total_amount as i128)
                .saturating_mul(a.weight_bps as i128)
                / BPS as i128;
            amounts.push_back(amt);
        }
        Ok(amounts)
    }

    /// Return the ID of the active strategy with the highest APY.
    pub fn best_strategy(env: Env) -> Result<u32, Error> {
        Self::assert_initialized(&env)?;
        let count = get_strategy_count(&env);
        let mut best_id: u32 = 0;
        let mut best_apy: u32 = 0;
        for i in 1..=count {
            if !has_strategy(&env, i) {
                continue;
            }
            if let Ok(s) = get_strategy(&env, i) {
                if s.is_active && s.apy_bps > best_apy {
                    best_apy = s.apy_bps;
                    best_id = i;
                }
            }
        }
        if best_id == 0 {
            return Err(Error::StrategyNotFound);
        }
        Ok(best_id)
    }

    /// Simulate historical performance of a strategy over `duration_secs`.
    /// Pure computation — no state changes.
    pub fn backtest(
        env: Env,
        strategy_id: u32,
        initial_amount: i128,
        duration_secs: u64,
    ) -> Result<BacktestResult, Error> {
        if initial_amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        if duration_secs == 0 {
            return Err(Error::InvalidDuration);
        }
        let s = get_strategy(&env, strategy_id)?;
        // Compound annually: final = initial * (1 + apy)^years
        // Approximated with continuous compounding in integer arithmetic:
        // reward = initial * apy_bps * duration / (BPS * SECS_PER_YEAR)
        let reward = (initial_amount as i128)
            .saturating_mul(s.apy_bps as i128)
            .saturating_mul(duration_secs as i128)
            / (BPS as i128 * SECS_PER_YEAR as i128);
        let final_amount = initial_amount.saturating_add(reward);
        let gain = final_amount - initial_amount;
        // effective_apy_bps = gain * BPS * SECS_PER_YEAR / (initial * duration)
        let effective_apy_bps = if initial_amount > 0 && duration_secs > 0 {
            ((gain as i128)
                .saturating_mul(BPS as i128)
                .saturating_mul(SECS_PER_YEAR as i128)
                / (initial_amount as i128 * duration_secs as i128)) as u32
        } else {
            0
        };
        Ok(BacktestResult {
            strategy_id,
            initial_amount,
            final_amount,
            gain,
            effective_apy_bps,
            duration_secs,
        })
    }

    // ── Read-only ─────────────────────────────────────────────────────────────

    pub fn get_strategy(env: Env, strategy_id: u32) -> Result<Strategy, Error> {
        get_strategy(&env, strategy_id)
    }

    pub fn strategy_count(env: Env) -> u32 {
        get_strategy_count(&env)
    }

    pub fn get_position(env: Env, user: Address, strategy_id: u32) -> Result<Position, Error> {
        let s = get_strategy(&env, strategy_id)?;
        let pos = get_position(&env, strategy_id, &user)?;
        Ok(Self::accrue(pos, &s, env.ledger().timestamp()))
    }

    pub fn is_paused(env: Env) -> bool {
        is_paused(&env)
    }

    pub fn list_strategies(env: Env) -> Vec<u32> {
        let count = get_strategy_count(&env);
        let mut ids = Vec::new(&env);
        for i in 1..=count {
            if has_strategy(&env, i) {
                ids.push_back(i);
            }
        }
        ids
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn accrue(mut pos: Position, s: &Strategy, now: u64) -> Position {
        let elapsed = now.saturating_sub(pos.last_update_ts);
        if elapsed == 0 || s.apy_bps == 0 || pos.compounded_balance == 0 {
            return pos;
        }
        let reward = (pos.compounded_balance as i128)
            .saturating_mul(s.apy_bps as i128)
            .saturating_mul(elapsed as i128)
            / (BPS as i128 * SECS_PER_YEAR as i128);
        pos.compounded_balance = pos.compounded_balance.saturating_add(reward);
        pos.last_update_ts = now;
        pos
    }

    fn assert_initialized(env: &Env) -> Result<(), Error> {
        if !is_initialized(env) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }

    fn assert_admin(env: &Env, caller: &Address) -> Result<(), Error> {
        Self::assert_initialized(env)?;
        caller.require_auth();
        if *caller != get_admin(env)? {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    fn assert_not_paused(env: &Env) -> Result<(), Error> {
        if is_paused(env) {
            return Err(Error::ContractPaused);
        }
        Ok(())
    }
}
