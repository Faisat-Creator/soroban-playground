// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger as _}, vec, Env, String};

use crate::{YieldOptimizer, YieldOptimizerClient};
use crate::types::{Allocation, Error};

fn setup() -> (Env, Address, YieldOptimizerClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, YieldOptimizer);
    let client = YieldOptimizerClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, admin, client)
}

use soroban_sdk::Address;

fn add_strategy(env: &Env, client: &YieldOptimizerClient, admin: &Address, apy: u32) -> u32 {
    client.add_strategy(admin, &String::from_str(env, "Strategy"), &apy)
        .unwrap()
}

// ── Init ──────────────────────────────────────────────────────────────────────

#[test]
fn test_init_ok() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, YieldOptimizer);
    let client = YieldOptimizerClient::new(&env, &id);
    let admin = Address::generate(&env);
    assert_eq!(client.initialize(&admin), Ok(()));
}

#[test]
fn test_init_twice_fails() {
    let (_, admin, client) = setup();
    assert_eq!(client.initialize(&admin), Err(Ok(Error::AlreadyInitialized)));
}

// ── Pause ─────────────────────────────────────────────────────────────────────

#[test]
fn test_pause_unpause() {
    let (_, admin, client) = setup();
    assert!(!client.is_paused());
    client.pause(&admin).unwrap();
    assert!(client.is_paused());
    client.unpause(&admin).unwrap();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_blocks_deposit() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 1000);
    client.pause(&admin).unwrap();
    let user = Address::generate(&env);
    assert_eq!(
        client.deposit(&user, &sid, &1_000_000),
        Err(Ok(Error::ContractPaused))
    );
}

// ── Strategy management ───────────────────────────────────────────────────────

#[test]
fn test_add_strategy_ok() {
    let (env, admin, client) = setup();
    let id = add_strategy(&env, &client, &admin, 500);
    assert_eq!(id, 1);
    let s = client.get_strategy(&id).unwrap();
    assert_eq!(s.apy_bps, 500);
    assert!(s.is_active);
}

#[test]
fn test_add_strategy_invalid_apy() {
    let (env, admin, client) = setup();
    assert_eq!(
        client.add_strategy(&admin, &String::from_str(&env, "S"), &50_001),
        Err(Ok(Error::InvalidApy))
    );
}

#[test]
fn test_update_apy() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 500);
    client.update_apy(&admin, &sid, &2000).unwrap();
    assert_eq!(client.get_strategy(&sid).unwrap().apy_bps, 2000);
}

#[test]
fn test_set_strategy_inactive() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 500);
    client.set_strategy_active(&admin, &sid, &false).unwrap();
    let user = Address::generate(&env);
    assert_eq!(
        client.deposit(&user, &sid, &1_000),
        Err(Ok(Error::StrategyPaused))
    );
}

// ── Deposit / Withdraw ────────────────────────────────────────────────────────

#[test]
fn test_deposit_withdraw() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 1000);
    let user = Address::generate(&env);
    client.deposit(&user, &sid, &1_000_000).unwrap();
    let pos = client.get_position(&user, &sid).unwrap();
    assert_eq!(pos.deposited, 1_000_000);
    client.withdraw(&user, &sid, &500_000).unwrap();
    let pos2 = client.get_position(&user, &sid).unwrap();
    assert_eq!(pos2.compounded_balance, 500_000);
}

#[test]
fn test_withdraw_excess_fails() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 1000);
    let user = Address::generate(&env);
    client.deposit(&user, &sid, &1_000).unwrap();
    assert_eq!(
        client.withdraw(&user, &sid, &2_000),
        Err(Ok(Error::InsufficientBalance))
    );
}

// ── Auto-compound ─────────────────────────────────────────────────────────────

#[test]
fn test_compound_accrues_rewards() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 10_000); // 100% APY
    let user = Address::generate(&env);
    client.deposit(&user, &sid, &1_000_000).unwrap();
    // Advance ledger by 1 year
    env.ledger().with_mut(|l| l.timestamp += 31_536_000);
    let bal = client.compound(&user, &sid).unwrap();
    assert!(bal > 1_000_000, "balance should have grown");
}

// ── Allocate ──────────────────────────────────────────────────────────────────

#[test]
fn test_allocate_ok() {
    let (env, admin, client) = setup();
    let s1 = add_strategy(&env, &client, &admin, 500);
    let s2 = add_strategy(&env, &client, &admin, 1000);
    let allocs = vec![
        &env,
        Allocation { strategy_id: s1, weight_bps: 6000 },
        Allocation { strategy_id: s2, weight_bps: 4000 },
    ];
    let amounts = client.allocate(&allocs, &1_000_000).unwrap();
    assert_eq!(amounts.get(0).unwrap(), 600_000);
    assert_eq!(amounts.get(1).unwrap(), 400_000);
}

#[test]
fn test_allocate_invalid_weights() {
    let (env, admin, client) = setup();
    let s1 = add_strategy(&env, &client, &admin, 500);
    let allocs = vec![&env, Allocation { strategy_id: s1, weight_bps: 5000 }];
    assert_eq!(
        client.allocate(&allocs, &1_000_000),
        Err(Ok(Error::InvalidWeights))
    );
}

// ── Best strategy ─────────────────────────────────────────────────────────────

#[test]
fn test_best_strategy() {
    let (env, admin, client) = setup();
    let s1 = add_strategy(&env, &client, &admin, 500);
    let s2 = add_strategy(&env, &client, &admin, 2000);
    assert_eq!(client.best_strategy().unwrap(), s2);
    // Deactivate s2 — best should fall back to s1
    client.set_strategy_active(&admin, &s2, &false).unwrap();
    assert_eq!(client.best_strategy().unwrap(), s1);
}

// ── Backtest ──────────────────────────────────────────────────────────────────

#[test]
fn test_backtest_one_year() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 1000); // 10% APY
    let result = client.backtest(&sid, &1_000_000, &31_536_000).unwrap();
    assert_eq!(result.initial_amount, 1_000_000);
    assert!(result.final_amount > 1_000_000);
    assert_eq!(result.gain, result.final_amount - 1_000_000);
}

#[test]
fn test_backtest_zero_duration_fails() {
    let (env, admin, client) = setup();
    let sid = add_strategy(&env, &client, &admin, 1000);
    assert_eq!(
        client.backtest(&sid, &1_000_000, &0),
        Err(Ok(Error::InvalidDuration))
    );
}

#[test]
fn test_list_strategies() {
    let (env, admin, client) = setup();
    add_strategy(&env, &client, &admin, 500);
    add_strategy(&env, &client, &admin, 1000);
    let list = client.list_strategies();
    assert_eq!(list.len(), 2);
}
