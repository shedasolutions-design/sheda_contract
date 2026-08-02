use near_contract_standards::non_fungible_token::core::NonFungibleTokenCore;
use near_sdk::{
    assert_one_yocto, env, json_types::U128, log, require, AccountId, Gas, NearToken, Promise,
    PromiseResult,
};

use crate::{
    events::{
        emit_event, BidApprovedEvent, BidCancelledEvent, BidRefundedEvent, BidRejectedEvent,
        DealFinalizedEvent, DisputeRaisedEvent, LeaseExpiredEvent, LeaseRenewedEvent,
        PropertyDeletedEvent, PropertyDelistedEvent,
    },
    ext::ft_contract,
    models::{Action, Bid, BidStatus},
    ShedaContract,
};

fn checked_add_u128(left: u128, right: u128, label: &str) -> u128 {
    left.checked_add(right)
        .unwrap_or_else(|| env::panic_str(&format!("Overflow in {}", label)))
}

fn checked_sub_u128(left: u128, right: u128, label: &str) -> u128 {
    left.checked_sub(right)
        .unwrap_or_else(|| env::panic_str(&format!("Underflow in {}", label)))
}

fn checked_add_u64(left: u64, right: u64, label: &str) -> u64 {
    left.checked_add(right)
        .unwrap_or_else(|| env::panic_str(&format!("Overflow in {}", label)))
}

fn checked_mul_u64(left: u64, right: u64, label: &str) -> u64 {
    left.checked_mul(right)
        .unwrap_or_else(|| env::panic_str(&format!("Overflow in {}", label)))
}

fn bid_lock_key(property_id: u64, bid_id: u64) -> String {
    format!("bid:{}:{}", property_id, bid_id)
}

fn ft_lock_key(property_id: u64, account_id: &AccountId) -> String {
    format!("ft:{}:{}", account_id, property_id)
}

pub fn lock_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let key = bid_lock_key(property_id, bid_id);
    require!(
        !contract.reentrancy_locks.contains(&key),
        "ReentrancyGuard: bid locked"
    );
    contract.reentrancy_locks.insert(key);
}

pub fn unlock_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let key = bid_lock_key(property_id, bid_id);
    contract.reentrancy_locks.remove(&key);
}

pub fn lock_ft_on_transfer(contract: &mut ShedaContract, property_id: u64, account_id: &AccountId) {
    let key = ft_lock_key(property_id, account_id);
    require!(
        !contract.reentrancy_locks.contains(&key),
        "ReentrancyGuard: ft_on_transfer locked"
    );
    contract.reentrancy_locks.insert(key);
}

pub fn unlock_ft_on_transfer(
    contract: &mut ShedaContract,
    property_id: u64,
    account_id: &AccountId,
) {
    let key = ft_lock_key(property_id, account_id);
    contract.reentrancy_locks.remove(&key);
}

pub(crate) fn update_bid_in_list<F>(bids: &mut Vec<Bid>, bid_id: u64, update: F) -> Bid
where
    F: FnOnce(&mut Bid),
{
    let index = bids
        .iter()
        .position(|bid| bid.id == bid_id)
        .expect("Bid not found for the property");
    let bid = bids.get_mut(index).expect("Bid not found for the property");
    update(bid);
    bid.clone()
}

fn get_bid_from_list(bids: &Vec<Bid>, bid_id: u64) -> Bid {
    bids.iter()
        .find(|bid| bid.id == bid_id)
        .expect("Bid not found for the property")
        .clone()
}

pub fn extract_base_uri(url: &str) -> String {
    if let Some(cid) = url.split("/ipfs/").nth(1) {
        return format!("ipfs://{}", cid);
    }

    // fallback base_uri = origin of the URL
    // ex: https://example.com/path/image.png → https://example.com
    url.split('/').take(3).collect::<Vec<_>>().join("/")
}

// Storage deposit check helper - can be used in payable methods that create new storage
// Example usage: assert_storage_deposit_for_bytes(1000);
// The bytes parameter should be estimated based on the size of data being stored
#[allow(dead_code)]
pub fn assert_storage_deposit_for_bytes(estimated_bytes: u128) {
    let attached = env::attached_deposit();
    let required = near_sdk::env::storage_byte_cost().saturating_mul(estimated_bytes);

    require!(
        attached >= required,
        format!(
            "Insufficient storage deposit. Required at least {}",
            required
        )
    );
}

pub fn burn_nft(contract: &mut ShedaContract, token_id: String) {
    assert_one_yocto();

    let token = contract
        .tokens
        .nft_token(token_id.clone())
        .expect("Token not found");

    assert_eq!(
        env::signer_account_id(),
        token.owner_id,
        "Only owner can burn"
    );

    // Remove token ownership and metadata
    contract.tokens.owner_by_id.remove(&token_id);
    if let Some(tokens_per_owner) = contract.tokens.tokens_per_owner.as_mut() {
        let mut owner_tokens = tokens_per_owner.get(&token.owner_id).unwrap_or_else(|| {
            env::panic_str("Unable to access tokens per owner in unguarded call.")
        });
        owner_tokens.remove(&token_id);
        if owner_tokens.is_empty() {
            tokens_per_owner.remove(&token.owner_id);
        } else {
            tokens_per_owner.insert(&token.owner_id.clone(), &owner_tokens);
        }
    }
    if let Some(token_metadata_by_id) = contract.tokens.token_metadata_by_id.as_mut() {
        token_metadata_by_id.remove(&token_id);
    }
    if let Some(approvals_by_id) = contract.tokens.approvals_by_id.as_mut() {
        approvals_by_id.remove(&token_id);
    }
}

pub fn internal_accept_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) -> Promise {
    lock_bid(contract, property_id, bid_id);
    let (owner_id, has_active_lease) = {
        let property = contract
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        (
            property.owner_id.clone(),
            property
                .active_lease
                .as_ref()
                .map(|lease| lease.active)
                .unwrap_or(false),
        )
    };

    assert_eq!(
        owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept bids"
    );

    // finalize_accepted_bid() (Purchase and Lease alike) transfers the NFT
    // from property.owner_id to the bidder. While a lease is active the NFT
    // is actually held by the tenant, not the owner, so that transfer would
    // panic there instead of failing cleanly here — after payment has
    // already gone out on the accept_bid fast path. Reject up front instead.
    require!(
        !has_active_lease,
        "Cannot accept a bid while the property has an active lease"
    );

    let now = env::block_timestamp();
    let bid_snapshot = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    if let Some(expires_at) = bid_snapshot.expires_at {
        if now > expires_at {
            unlock_bid(contract, property_id, bid_id);
            internal_reject_bid(contract, property_id, bid_id);
            env::panic_str("Bid expired and was rejected");
        }
    }

    let bid = {
        let bids = contract
            .bids
            .get_mut(&property_id)
            .expect("Bid does not exist");
        update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Accepted;
            bid.updated_at = now;
        })
    };

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    emit_event(
        "BidApproved",
        BidApprovedEvent {
            token_id: property_id,
            bidder_id: bid.bidder.clone(),
            seller_id: owner_id.clone(),
            amount: bid.amount,
        },
    );

    // Part 1: Transfer stablecoin from contract to property owner
    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(owner_id.clone(), U128(bid.amount));

    // Update stablecoin balance after payment to seller
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "accept_bid balance"),
    );

    if contract.mock_transfers_enabled {
        unlock_bid(contract, property_id, bid_id);
        finalize_accepted_bid(contract, property_id, bid_id);
        return Promise::new(env::current_account_id()).transfer(NearToken::from_yoctonear(0));
    }

    // Part 2: Callback to handle success/failure
    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(50))
            .accept_bid_callback(property_id, bid_id),
    )
}

// Callback to handle the result of ft_transfer
pub fn accept_bid_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    unlock_bid(contract, property_id, bid_id);
    // Check if the promise succeeded
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            log!("ft_transfer successful, proceeding with NFT transfer and bid updates");
            finalize_accepted_bid(contract, property_id, bid_id);
        }
        PromiseResult::Failed => {
            log!("ft_transfer failed, reverting. NFT and bid remain unchanged");

            // Revert the stablecoin balance update
            let bid = {
                let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
                get_bid_from_list(bids, bid_id)
            };

            let current_balance = *contract
                .stable_coin_balances
                .get(&bid.stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                bid.stablecoin_token.clone(),
                checked_add_u128(current_balance, bid.amount, "accept_bid revert"),
            );

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::Pending;
                    bid.updated_at = env::block_timestamp();
                });
            }

            // NOTE: do not panic here. A panicking receipt discards every state
            // write it made in this same call, including the revert above, which
            // would otherwise leave the balance permanently short and the bid
            // stuck in `Accepted` with no way to retry/cancel/reject it.
            log!("Payment transfer failed. Bid acceptance aborted; balance and status reverted to Pending.");
        }
    }
}

// Shared callback for the refund transfers fired from internal_reject_bid,
// internal_cancel_bid, and the "refund other pending bidders" loops in
// internal_accept_bid_with_escrow / finalize_accepted_bid. Those call sites
// optimistically decrement the ledger and flip the bid to a terminal status
// before the transfer confirms; if the transfer actually fails (e.g. the
// bidder's account isn't storage-registered on the token contract), this
// callback puts both back so the refund can be retried instead of the funds
// silently vanishing from the contract's own accounting.
pub fn refund_pending_bid_callback(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    stablecoin_token: AccountId,
    amount: u128,
) {
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            log!("Refund transfer for bid {} succeeded", bid_id);
        }
        PromiseResult::Failed => {
            let current_balance = *contract
                .stable_coin_balances
                .get(&stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                stablecoin_token,
                checked_add_u128(current_balance, amount, "refund_pending_bid revert"),
            );

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::Pending;
                    bid.updated_at = env::block_timestamp();
                });
            }

            log!(
                "Refund transfer for bid {} failed; balance and status reverted to Pending",
                bid_id
            );
        }
    }
}

pub fn internal_reject_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) -> Promise {
    let bid = {
        let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    if bid.status != BidStatus::Pending {
        env::panic_str("Bid is not in a pending state");
    }

    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can reject bids"
    );

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    // Refund stablecoin to bidder
    let refund_promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));

    // Update stablecoin balance after refund (reverted on transfer failure)
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "reject_bid refund"),
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Rejected;
            bid.updated_at = env::block_timestamp();
        });
    }

    emit_event(
        "BidRejected",
        BidRejectedEvent {
            token_id: property_id,
            bid_id,
            bidder_id: bid.bidder.clone(),
            amount: bid.amount,
        },
    );

    refund_promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(20))
            .refund_pending_bid_callback(
                property_id,
                bid_id,
                bid.stablecoin_token.clone(),
                bid.amount,
            ),
    )
}

pub fn internal_cancel_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) -> Promise {
    let bid = {
        let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    if bid.status != BidStatus::Pending {
        env::panic_str("Bid is not in a pending state");
    }

    assert_eq!(
        bid.bidder,
        env::predecessor_account_id(),
        "Only the bidder can cancel their bid"
    );

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    //ensure my bid was not accepted yet
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    if let Some(sold) = &property.sold {
        if sold.buyer_id == bid.bidder {
            env::panic_str("Cannot cancel bid: property already sold to you");
        }
    }
    if let Some(lease) = &property.active_lease {
        if lease.tenant_id == bid.bidder && lease.active {
            env::panic_str("Cannot cancel bid: property already leased to you");
        }
    }

    // Refund stablecoin to bidder
    let refund_promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));

    // Update stablecoin balance after refund (reverted on transfer failure)
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "cancel_bid refund"),
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Cancelled;
            bid.updated_at = env::block_timestamp();
        });
    }

    emit_event(
        "BidCancelled",
        BidCancelledEvent {
            token_id: property_id,
            bid_id,
            bidder_id: bid.bidder.clone(),
            amount: bid.amount,
        },
    );

    refund_promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(20))
            .refund_pending_bid_callback(
                property_id,
                bid_id,
                bid.stablecoin_token.clone(),
                bid.amount,
            ),
    )
}

pub fn internal_accept_bid_with_escrow(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    let (owner_id, lease_duration_months, has_active_lease) = {
        let property = contract
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        (
            property.owner_id.clone(),
            property.lease_duration_months,
            property
                .active_lease
                .as_ref()
                .map(|lease| lease.active)
                .unwrap_or(false),
        )
    };

    assert_eq!(
        owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept bids"
    );

    // See internal_accept_bid for why this has to be rejected here rather
    // than left to fail later at NFT-transfer time.
    require!(
        !has_active_lease,
        "Cannot accept a bid while the property has an active lease"
    );

    let now = env::block_timestamp();
    let bid_snapshot = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    if let Some(expires_at) = bid_snapshot.expires_at {
        if now > expires_at {
            internal_reject_bid(contract, property_id, bid_id);
            env::panic_str("Bid expired and was rejected");
        }
    }

    let bid = {
        let bids = contract
            .bids
            .get_mut(&property_id)
            .expect("Bid does not exist");
        update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Accepted;
            bid.updated_at = now;
        })
    };

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    emit_event(
        "BidApproved",
        BidApprovedEvent {
            token_id: property_id,
            bidder_id: bid.bidder.clone(),
            seller_id: owner_id.clone(),
            amount: bid.amount,
        },
    );

    lock_bid(contract, property_id, bid_id);

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        for other_bid in bids.iter_mut() {
            if other_bid.id == bid_id || other_bid.status != BidStatus::Pending {
                continue;
            }

            if env::used_gas().as_gas() >= env::prepaid_gas().as_gas() - Gas::from_tgas(40).as_gas()
            {
                continue;
            }

            let other_bid_id = other_bid.id;
            let other_token = other_bid.stablecoin_token.clone();
            let other_amount = other_bid.amount;

            let other_refund_promise = ft_contract::ext(other_token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(Gas::from_tgas(30))
                .ft_transfer(other_bid.bidder.clone(), U128(other_amount));

            let current_balance = *contract
                .stable_coin_balances
                .get(&other_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                other_token.clone(),
                checked_sub_u128(current_balance, other_amount, "accept_bid_with_escrow"),
            );

            other_bid.status = BidStatus::Rejected;
            other_bid.updated_at = env::block_timestamp();

            other_refund_promise.then(
                crate::ShedaContract::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(20))
                    .refund_pending_bid_callback(
                        property_id,
                        other_bid_id,
                        other_token,
                        other_amount,
                    ),
            );
        }
    }

    unlock_bid(contract, property_id, bid_id);

    if matches!(&bid.action, Action::Lease) {
        let mut updated_property = contract
            .properties
            .get(&property_id)
            .cloned()
            .expect("Property does not exist");
        let lease = crate::models::Lease {
            id: contract.lease_counter,
            property_id,
            tenant_id: bid.bidder.clone(),
            start_time: env::block_timestamp(),
            end_time: checked_add_u64(
                env::block_timestamp(),
                checked_mul_u64(
                    lease_duration_months.unwrap(),
                    30 * 24 * 60 * 60 * 1_000_000_000,
                    "lease duration",
                ),
                "lease end_time",
            ),
            active: true,
            dispute_status: crate::models::DisputeStatus::None,
            dispute: None,
            escrow_held: bid.amount,
            escrow_token: bid.stablecoin_token.clone(),
        };
        let lease_id = lease.id;
        updated_property.active_lease = Some(lease.clone());
        contract.leases.insert(lease.id, lease);
        contract.lease_counter = checked_add_u64(contract.lease_counter, 1, "lease_counter");
        contract.properties.insert(property_id, updated_property);

        let mut tenant_leases = contract
            .lease_per_tenant
            .get(&bid.bidder)
            .cloned()
            .unwrap_or_default();
        tenant_leases.push(lease_id);
        contract
            .lease_per_tenant
            .insert(bid.bidder.clone(), tenant_leases);

        if let Some(bids) = contract.bids.get_mut(&property_id) {
            let _ = update_bid_in_list(bids, bid_id, |b| {
                b.lease_id = Some(lease_id);
            });
        }

        emit_event(
            "DealFinalized",
            DealFinalizedEvent {
                token_id: property_id,
                buyer_id: bid.bidder.clone(),
                seller_id: owner_id.clone(),
                amount: bid.amount,
                lease_duration_nanos: lease_duration_months.unwrap_or(0)
                    * 30
                    * 24
                    * 60
                    * 60
                    * 1_000_000_000,
            },
        );
    }

    true
}

fn finalize_accepted_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist")
        .clone();

    let bid = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    // Transfer NFT to bidder
    contract.tokens.internal_transfer(
        &property.owner_id,
        &bid.bidder,
        &property_id.to_string(),
        None,
        None,
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        for other_bid in bids.iter_mut() {
            if other_bid.id == bid_id {
                other_bid.status = BidStatus::Completed;
                other_bid.updated_at = env::block_timestamp();
                other_bid.escrow_release_tx = Some(format!("block:{}", env::block_height()));
                continue;
            }

            if other_bid.status != BidStatus::Pending {
                continue;
            }

            if env::used_gas().as_gas() >= env::prepaid_gas().as_gas() - Gas::from_tgas(40).as_gas()
            {
                continue;
            }

            let other_bid_id = other_bid.id;
            let other_token = other_bid.stablecoin_token.clone();
            let other_amount = other_bid.amount;

            let other_refund_promise = ft_contract::ext(other_token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(Gas::from_tgas(30))
                .ft_transfer(other_bid.bidder.clone(), U128(other_amount));

            let current_balance = *contract
                .stable_coin_balances
                .get(&other_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                other_token.clone(),
                checked_sub_u128(current_balance, other_amount, "accept_bid refund"),
            );

            other_bid.status = BidStatus::Rejected;
            other_bid.updated_at = env::block_timestamp();

            other_refund_promise.then(
                crate::ShedaContract::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(20))
                    .refund_pending_bid_callback(
                        property_id,
                        other_bid_id,
                        other_token,
                        other_amount,
                    ),
            );
        }
    }

    match bid.action {
        Action::Purchase => {
            // Emit before the handover, while property.owner_id still holds
            // the seller — transfer_property_ownership overwrites it.
            emit_event(
                "DealFinalized",
                DealFinalizedEvent {
                    token_id: property_id,
                    buyer_id: bid.bidder.clone(),
                    seller_id: property.owner_id.clone(),
                    amount: bid.amount,
                    lease_duration_nanos: 0,
                },
            );

            transfer_property_ownership(contract, property_id, &bid.bidder);
        }
        Action::Lease => {
            let mut updated_property = property.clone();
            let lease = crate::models::Lease {
                id: contract.lease_counter,
                property_id,
                tenant_id: bid.bidder.clone(),
                start_time: env::block_timestamp(),
                end_time: checked_add_u64(
                    env::block_timestamp(),
                    checked_mul_u64(
                        property.lease_duration_months.unwrap(),
                        30 * 24 * 60 * 60 * 1_000_000_000,
                        "lease duration",
                    ),
                    "lease end_time",
                ),
                active: true,
                dispute_status: crate::models::DisputeStatus::None,
                dispute: None,
                escrow_held: bid.amount,
                escrow_token: bid.stablecoin_token.clone(),
            };
            let lease_id = lease.id;
            updated_property.active_lease = Some(lease.clone());
            contract.leases.insert(lease.id, lease);
            contract.lease_counter = checked_add_u64(contract.lease_counter, 1, "lease_counter");
            contract.properties.insert(property_id, updated_property);

            let mut tenant_leases = contract
                .lease_per_tenant
                .get(&bid.bidder)
                .cloned()
                .unwrap_or_default();
            tenant_leases.push(lease_id);
            contract
                .lease_per_tenant
                .insert(bid.bidder.clone(), tenant_leases);

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |b| {
                    b.lease_id = Some(lease_id);
                });
            }

            emit_event(
                "DealFinalized",
                DealFinalizedEvent {
                    token_id: property_id,
                    buyer_id: bid.bidder.clone(),
                    seller_id: property.owner_id.clone(),
                    amount: bid.amount,
                    lease_duration_nanos: property.lease_duration_months.unwrap_or(0)
                        * 30
                        * 24
                        * 60
                        * 60
                        * 1_000_000_000,
                },
            );
        }
    }
}

// ---- Lease renewal ----
//
// A renewal is a Lease bid placed the normal way (ft_on_transfer) by the
// account that already holds the property's active lease. Unlike a regular
// bid it doesn't compete for the NFT — the tenant already has it — so it
// skips the NFT-transfer machinery entirely and just extends the existing
// Lease and mints this term's own document. accept_bid/accept_bid_with_escrow
// deliberately refuse to touch a bid while the property has an active lease
// (see their has_active_lease guard); this is the one path allowed to act
// while a lease is active, and only for the current tenant's own bid.
pub fn internal_accept_lease_renewal(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> Promise {
    lock_bid(contract, property_id, bid_id);

    let (owner_id, lease_duration_months, current_lease) = {
        let property = contract
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        (
            property.owner_id.clone(),
            property.lease_duration_months,
            property.active_lease.clone(),
        )
    };

    assert_eq!(
        owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept a lease renewal"
    );

    let lease = current_lease.expect("Property has no active lease to renew");
    let duration_months =
        lease_duration_months.expect("Property is not configured with a lease duration");

    let bid = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );
    require!(
        matches!(&bid.action, Action::Lease),
        "Renewal bid must be a Lease bid"
    );
    require!(
        bid.status == BidStatus::Pending,
        "Bid is not in a pending state"
    );
    assert_eq!(
        bid.bidder, lease.tenant_id,
        "Only the current tenant's own bid can be accepted as a renewal"
    );

    // Extend from the lease's own end_time, not from now — renewing a few
    // days early (or a few days after end_time, before anyone reclaims)
    // shouldn't shift the schedule or waste/duplicate any paid-for time.
    let new_end_time = checked_add_u64(
        lease.end_time,
        checked_mul_u64(
            duration_months,
            30 * 24 * 60 * 60 * 1_000_000_000,
            "renewal duration",
        ),
        "renewal end_time",
    );

    {
        let bids = contract
            .bids
            .get_mut(&property_id)
            .expect("Bid does not exist");
        update_bid_in_list(bids, bid_id, |b| {
            b.status = BidStatus::Accepted;
            b.updated_at = env::block_timestamp();
            b.lease_id = Some(lease.id);
        });
    }

    emit_event(
        "BidApproved",
        BidApprovedEvent {
            token_id: property_id,
            bidder_id: bid.bidder.clone(),
            seller_id: owner_id.clone(),
            amount: bid.amount,
        },
    );

    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(owner_id.clone(), U128(bid.amount));

    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "accept_lease_renewal balance"),
    );

    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(50))
            .accept_lease_renewal_callback(property_id, bid_id, new_end_time),
    )
}

pub fn accept_lease_renewal_callback(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    new_end_time: u64,
) {
    unlock_bid(contract, property_id, bid_id);
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            let bid = {
                let bids = contract.bids.get(&property_id).expect("Bid does not exist");
                get_bid_from_list(bids, bid_id)
            };
            let lease_id = bid.lease_id.expect("Renewal bid missing lease_id");

            let mut lease = contract
                .leases
                .get(&lease_id)
                .cloned()
                .expect("Lease not found");
            lease.end_time = new_end_time;
            lease.active = true;
            contract.leases.insert(lease_id, lease.clone());

            let mut property = contract
                .properties
                .get(&property_id)
                .cloned()
                .expect("Property does not exist");
            property.active_lease = Some(lease.clone());
            let owner_id = property.owner_id.clone();
            contract.properties.insert(property_id, property);

            // This renewal's own permanent document — never touches or
            // reissues the original term's token.
            let document_token_id = format!("doc:{}:{}", property_id, bid_id);
            let token_metadata =
                near_contract_standards::non_fungible_token::metadata::TokenMetadata {
                    title: Some(format!(
                        "Rent Agreement (Renewal) #{} — Property #{}",
                        bid_id, property_id
                    )),
                    description: Some(format!(
                        "Lease renewal for property #{}, new term ends at {}",
                        property_id, new_end_time
                    )),
                    copies: Some(1),
                    extra: Some(build_document_extra(
                        property_id,
                        bid_id,
                        "Lease",
                        true,
                        Some(lease.start_time),
                        Some(new_end_time),
                    )),
                    ..Default::default()
                };
            contract.tokens.internal_mint(
                document_token_id.clone(),
                owner_id.clone(),
                Some(token_metadata),
            );
            contract.tokens.internal_transfer(
                &owner_id,
                &bid.bidder,
                &document_token_id,
                None,
                None,
            );

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |b| {
                    b.status = BidStatus::Completed;
                    b.updated_at = env::block_timestamp();
                    b.document_token_id = Some(document_token_id.clone());
                    b.escrow_release_tx = Some(format!("block:{}", env::block_height()));
                });
            }

            emit_event(
                "LeaseRenewed",
                LeaseRenewedEvent {
                    token_id: property_id,
                    lease_id,
                    tenant_id: bid.bidder.clone(),
                    owner_id,
                    amount: bid.amount,
                    new_end_time,
                },
            );
        }
        PromiseResult::Failed => {
            let bid = {
                let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
                get_bid_from_list(bids, bid_id)
            };

            let current_balance = *contract
                .stable_coin_balances
                .get(&bid.stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                bid.stablecoin_token.clone(),
                checked_add_u128(current_balance, bid.amount, "accept_lease_renewal revert"),
            );

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |b| {
                    b.status = BidStatus::Pending;
                    b.updated_at = env::block_timestamp();
                    b.lease_id = None;
                });
            }

            log!("Lease renewal payment failed; balance and status reverted to Pending.");
        }
    }
}

// Every document/agreement token this contract mints must be traceable back
// to the exact property it's for — the token_id already encodes it
// ("doc:{property_id}:{bid_id}"), but that only helps a caller who knows to
// parse the id. This puts the same information in the title (human-readable)
// and in `extra` as machine-readable JSON, for indexers/wallets/the app to
// read directly without parsing IDs. All interpolated values here are
// non-string (u64/bool) or a literal we control ("Purchase"/"Lease"), so
// plain string formatting is safe — no untrusted input needs escaping.
fn build_document_extra(
    property_id: u64,
    bid_id: u64,
    action_label: &str,
    is_renewal: bool,
    lease_start: Option<u64>,
    lease_end: Option<u64>,
) -> String {
    format!(
        "{{\"property_id\":{},\"bid_id\":{},\"action\":\"{}\",\"is_renewal\":{},\"lease_start\":{},\"lease_end\":{}}}",
        property_id,
        bid_id,
        action_label,
        is_renewal,
        lease_start
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
        lease_end
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
}

pub fn internal_confirm_document_release(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    document_image_uri: String,
    document_description: String,
) -> bool {
    let property_owner_id = {
        let property = contract
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        property.owner_id.clone()
    };

    assert_eq!(
        property_owner_id,
        env::predecessor_account_id(),
        "Only the property owner can release documents"
    );

    let bid_snapshot = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    if bid_snapshot.status != BidStatus::Accepted {
        env::panic_str("Bid is not in an accepted state");
    }

    if bid_snapshot.document_token_id.is_some() {
        env::panic_str("Document already minted for this bid");
    }

    let trimmed_uri = document_image_uri.trim();
    if trimmed_uri.is_empty() {
        env::panic_str("Document image URI is required");
    }

    let trimmed_description = document_description.trim();
    if trimmed_description.is_empty() {
        env::panic_str("Document description is required");
    }

    let document_token_id = format!("doc:{}:{}", property_id, bid_id);
    let (agreement_label, action_label) = match bid_snapshot.action {
        Action::Purchase => ("Property Document", "Purchase"),
        Action::Lease => ("Rent Agreement", "Lease"),
    };

    // Path B (accept_bid_with_escrow) — the only path that reaches this
    // function — already creates the Lease before docs can be released, so
    // bid_snapshot.lease_id and its real start/end are available here.
    let (lease_start, lease_end) = bid_snapshot
        .lease_id
        .and_then(|id| contract.leases.get(&id))
        .map(|lease| (Some(lease.start_time), Some(lease.end_time)))
        .unwrap_or((None, None));

    let token_metadata = near_contract_standards::non_fungible_token::metadata::TokenMetadata {
        title: Some(format!(
            "{} #{} — Property #{}",
            agreement_label, bid_id, property_id
        )),
        description: Some(trimmed_description.to_string()),
        media: Some(trimmed_uri.to_string()),
        copies: Some(1),
        extra: Some(build_document_extra(
            property_id,
            bid_id,
            action_label,
            false,
            lease_start,
            lease_end,
        )),
        ..Default::default()
    };

    contract.tokens.internal_mint(
        document_token_id.clone(),
        property_owner_id.clone(),
        Some(token_metadata),
    );
    contract.tokens.internal_transfer(
        &property_owner_id,
        &bid_snapshot.bidder,
        &document_token_id,
        None,
        None,
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Accepted {
                env::panic_str("Bid is not in an accepted state");
            }
            bid.status = BidStatus::DocsReleased;
            bid.updated_at = env::block_timestamp();
            bid.document_token_id = Some(document_token_id.clone());
            bid.document_image_uri = Some(trimmed_uri.to_string());
            bid.document_description = Some(trimmed_description.to_string());
        });
    } else {
        env::panic_str("Bid does not exist");
    }

    true
}

pub fn internal_confirm_document_receipt(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::DocsReleased {
                env::panic_str("Bid is not in a document released state");
            }
            if bid.document_token_id.is_none() {
                env::panic_str("No document was minted for this bid");
            }
            if bid.bidder != env::predecessor_account_id() {
                env::panic_str("Only the bidder can confirm receipt");
            }
            bid.status = BidStatus::DocsConfirmed;
            bid.updated_at = env::block_timestamp();
            bid.escrow_release_after = Some(checked_add_u64(
                env::block_timestamp(),
                contract.escrow_release_delay_ns,
                "escrow timelock",
            ));
        });
    } else {
        env::panic_str("Bid does not exist");
    }

    true
}

pub fn internal_release_escrow(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> Promise {
    lock_bid(contract, property_id, bid_id);
    let bid = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        bid.bidder,
        env::predecessor_account_id(),
        "Only the bidder can release escrow"
    );

    if bid.status != BidStatus::DocsConfirmed {
        env::panic_str("Bid is not in a document confirmed state");
    }

    if let Some(unlock_at) = bid.escrow_release_after {
        require!(
            env::block_timestamp() >= unlock_at,
            "Escrow timelock not reached"
        );
    }

    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));

    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "release_escrow"),
    );

    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(50))
            .release_escrow_callback(property_id, bid_id),
    )
}

pub fn release_escrow_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    unlock_bid(contract, property_id, bid_id);
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            let property = contract
                .properties
                .get(&property_id)
                .expect("Property does not exist")
                .clone();

            let bid = {
                let bids = contract.bids.get(&property_id).expect("Bid does not exist");
                get_bid_from_list(bids, bid_id)
            };

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::PaymentReleased;
                    bid.updated_at = env::block_timestamp();
                    bid.escrow_release_tx = Some(format!("block:{}", env::block_height()));
                });
            }

            match bid.action {
                Action::Purchase => {
                    contract.tokens.internal_transfer(
                        &property.owner_id,
                        &bid.bidder,
                        &property_id.to_string(),
                        None,
                        None,
                    );

                    // Keep Property.owner_id and property_per_owner in step
                    // with the token we just moved — without this the buyer
                    // holds the NFT but the contract still records the seller
                    // as owner, which leaves the property unusable by either
                    // party.
                    transfer_property_ownership(contract, property_id, &bid.bidder);
                }
                Action::Lease => {
                    // The Lease record was already created in
                    // internal_accept_bid_with_escrow when the bid was accepted
                    // (escrow_held there tracks the funds we just released).
                    // Creating a second Lease here would orphan that first
                    // record (it stays active/untracked in `leases`) and would
                    // never be indexed in `lease_per_tenant`, so all we do at
                    // this stage is hand over the NFT.
                    contract.tokens.internal_transfer(
                        &property.owner_id,
                        &bid.bidder,
                        &property_id.to_string(),
                        None,
                        None,
                    );
                }
            }
        }
        PromiseResult::Failed => {
            let bid = {
                let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
                get_bid_from_list(bids, bid_id)
            };

            let current_balance = *contract
                .stable_coin_balances
                .get(&bid.stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                bid.stablecoin_token.clone(),
                checked_add_u128(current_balance, bid.amount, "release_escrow revert"),
            );

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::DocsConfirmed;
                    bid.updated_at = env::block_timestamp();
                });
            }

            // NOTE: do not panic here, it would discard the revert writes above
            // (a panicking receipt rolls back every state change it made) and
            // leave the balance short with the bid stuck past DocsConfirmed.
            log!("Escrow release failed. Payment transfer aborted; balance and status reverted to DocsConfirmed.");
        }
    }
}

pub fn internal_raise_bid_dispute(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    reason: String,
) -> bool {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            let caller = env::predecessor_account_id();
            if caller != bid.bidder && caller != property.owner_id {
                env::panic_str("Only buyer or seller can raise dispute");
            }

            match bid.status {
                BidStatus::Accepted | BidStatus::DocsReleased | BidStatus::DocsConfirmed => {}
                _ => env::panic_str("Bid is not in a disputable state"),
            }

            bid.status = BidStatus::Disputed;
            bid.updated_at = env::block_timestamp();
            bid.dispute_reason = Some(reason.clone());
        });
    } else {
        env::panic_str("Bid does not exist");
    }

    emit_event(
        "DisputeRaised",
        DisputeRaisedEvent {
            token_id: property_id,
            tenant_id: env::predecessor_account_id(),
            bond_amount: 0,
        },
    );

    true
}

pub fn internal_complete_transaction(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            let caller = env::predecessor_account_id();
            if caller != bid.bidder && caller != property.owner_id {
                env::panic_str("Only buyer or seller can complete the transaction");
            }

            if bid.status != BidStatus::PaymentReleased {
                env::panic_str("Bid is not in a payment released state");
            }

            bid.status = BidStatus::Completed;
            bid.updated_at = env::block_timestamp();
        });
    } else {
        env::panic_str("Bid does not exist");
    }

    true
}

pub fn internal_refund_escrow_timeout(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    timeout_nanos: u64,
) -> Promise {
    lock_bid(contract, property_id, bid_id);
    let bid = {
        let bids = contract.bids.get(&property_id).expect("Bid does not exist");
        get_bid_from_list(bids, bid_id)
    };

    match bid.status {
        BidStatus::Accepted | BidStatus::DocsReleased => {}
        _ => env::panic_str("Bid is not in a refundable timeout state"),
    }

    let now = env::block_timestamp();
    if now.saturating_sub(bid.updated_at) < timeout_nanos {
        env::panic_str("Timeout threshold not reached");
    }

    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));

    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract.stable_coin_balances.insert(
        bid.stablecoin_token.clone(),
        checked_sub_u128(current_balance, bid.amount, "refund_escrow_timeout"),
    );

    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(30))
            .refund_escrow_timeout_callback(property_id, bid_id, bid.stablecoin_token, bid.amount),
    )
}

pub fn refund_escrow_timeout_callback(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    stablecoin_token: AccountId,
    amount: u128,
) {
    unlock_bid(contract, property_id, bid_id);
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            let bidder_id = contract
                .bids
                .get(&property_id)
                .map(|bids| get_bid_from_list(bids, bid_id).bidder)
                .unwrap_or_else(|| env::predecessor_account_id());
            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::Cancelled;
                    bid.updated_at = env::block_timestamp();
                    bid.escrow_release_tx = Some(format!("refund:{}", env::block_height()));
                });
            }

            emit_event(
                "BidRefunded",
                BidRefundedEvent {
                    token_id: property_id,
                    bid_id,
                    bidder_id,
                    amount,
                    reason: "escrow_timeout".to_string(),
                },
            );
        }
        PromiseResult::Failed => {
            let current_balance = *contract
                .stable_coin_balances
                .get(&stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                stablecoin_token,
                checked_add_u128(current_balance, amount, "refund_escrow_timeout revert"),
            );

            // NOTE: no panic here — see accept_bid_callback/release_escrow_callback
            // for why a trailing panic would discard this very revert write.
            log!("Timeout refund failed. Balance reverted.");
        }
    }
}

pub fn internal_delist_property(contract: &mut ShedaContract, property_id: u64) {
    let mut property = contract
        .properties
        .get(&property_id)
        .expect("Property not found")
        .clone();

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can delist the property"
    );

    assert!(
        property.active_lease.is_none(),
        "Cannot delist a property with an active lease"
    );

    assert!(property.sold.is_none(), "Cannot delist a sold property");

    // Set the property as not for sale
    property.is_for_sale = false;

    // Update the property in storage
    contract.properties.insert(property_id, property);

    emit_event(
        "PropertyDelisted",
        PropertyDelistedEvent {
            token_id: property_id,
            actor_id: env::predecessor_account_id(),
        },
    );
}

/// Hand a property over to its new owner, keeping `Property.owner_id` and the
/// `property_per_owner` index in step with the NEP-171 token.
///
/// The NFT was already being transferred on a completed purchase, but neither
/// of those two were updated alongside it, so the token said the buyer owned
/// the property while the contract's own records still said the seller did.
/// That left a purchased property permanently stuck: the seller couldn't act
/// on it (`delete_property`/`delist_property` both refuse once `sold` is set)
/// and the buyer couldn't either (they weren't `owner_id`), and it never
/// appeared in the buyer's portfolio, since `get_property_by_owner` reads
/// `property_per_owner`.
///
/// The property is left in a clean unlisted state so the new owner can do
/// whatever they like with it — relist it for sale, put it up for lease, or
/// transfer it on. `sold` is cleared rather than kept: leaving it set would
/// re-trip the very guards that froze the property in the first place. The
/// sale itself is still recorded in the `DealFinalized` event.
///
/// Call this immediately after `tokens.internal_transfer`, so the token and
/// these records can never drift apart again.
///
/// Purchases only. A lease deliberately leaves `owner_id` alone — the
/// landlord stays the owner for the duration, and `internal_expire_lease`
/// hands the token back when it ends.
pub fn transfer_property_ownership(
    contract: &mut ShedaContract,
    property_id: u64,
    new_owner: &AccountId,
) {
    let mut property = contract
        .properties
        .get(&property_id)
        .expect("Property not found")
        .clone();

    let previous_owner = property.owner_id.clone();
    if previous_owner == *new_owner {
        return;
    }

    // Drop it from the previous owner's index, removing the key outright when
    // that was their last property (same cleanup internal_delete_property does).
    let mut previous_owner_properties = contract
        .property_per_owner
        .get(&previous_owner)
        .cloned()
        .unwrap_or_default();
    previous_owner_properties.retain(|id| *id != property_id);
    if previous_owner_properties.is_empty() {
        contract.property_per_owner.remove(&previous_owner);
    } else {
        contract
            .property_per_owner
            .insert(previous_owner.clone(), previous_owner_properties);
    }

    // Add it to the new owner's index, guarding against a double-push in case
    // this ever runs twice for the same pair.
    let mut new_owner_properties = contract
        .property_per_owner
        .get(new_owner)
        .cloned()
        .unwrap_or_default();
    if !new_owner_properties.contains(&property_id) {
        new_owner_properties.push(property_id);
    }
    contract
        .property_per_owner
        .insert(new_owner.clone(), new_owner_properties);

    property.owner_id = new_owner.clone();
    property.sold = None;
    property.is_for_sale = false;
    contract.properties.insert(property_id, property);
}

pub fn internal_delete_property(contract: &mut ShedaContract, property_id: u64) {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property not found")
        .clone();

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can delete the property"
    );

    assert!(
        property.active_lease.is_none(),
        "Cannot delete a property with an active lease"
    );

    assert!(property.sold.is_none(), "Cannot delete a sold property");

    burn_nft(contract, property_id.to_string());

    // Remove the property from storage
    contract.properties.remove(&property_id);

    let mut owner_properties = contract
        .property_per_owner
        .get(&property.owner_id)
        .cloned()
        .unwrap_or_default();
    owner_properties.retain(|id| *id != property_id);
    if owner_properties.is_empty() {
        contract.property_per_owner.remove(&property.owner_id);
    } else {
        contract
            .property_per_owner
            .insert(property.owner_id.clone(), owner_properties);
    }

    emit_event(
        "PropertyDeleted",
        PropertyDeletedEvent {
            token_id: property_id,
            actor_id: env::predecessor_account_id(),
        },
    );
}

pub fn internal_raise_dispute(contract: &mut ShedaContract, lease_id: u64, reason: String) {
    let mut lease = contract
        .leases
        .get(&lease_id)
        .cloned()
        .expect("Lease not found");

    let caller = env::predecessor_account_id();
    assert_eq!(
        lease.tenant_id, caller,
        "Only the tenant can raise a dispute"
    );

    assert_eq!(
        lease.dispute_status,
        crate::models::DisputeStatus::None,
        "Dispute already raised for this lease"
    );

    lease.dispute_status = crate::models::DisputeStatus::Raised;
    lease.dispute = Some(crate::models::DisputeInfo {
        raised_by: caller,
        raised_at: env::block_timestamp(),
        reason,
        votes_for_tenant: 0,
        votes_for_owner: 0,
        oracle_result: None,
        oracle_request_id: None,
        oracle_updated_at: None,
        resolved_by: None,
        resolved_at: None,
    });

    let property_id = lease.property_id;
    contract.leases.insert(lease_id, lease);

    emit_event(
        "DisputeRaised",
        DisputeRaisedEvent {
            token_id: property_id,
            tenant_id: env::predecessor_account_id(),
            bond_amount: 0,
        },
    );
}

pub fn internal_expire_lease(contract: &mut ShedaContract, lease_id: u64) {
    let mut lease = contract
        .leases
        .get(&lease_id)
        .cloned()
        .expect("Lease not found");

    let current_time = env::block_timestamp();

    // Check if lease has expired
    require!(lease.end_time <= current_time, "Lease has not expired yet");

    require!(lease.active, "Lease is already inactive");

    // Mark lease as inactive
    lease.active = false;
    contract.leases.insert(lease_id, lease.clone());

    log!("Lease {} has ended and is now inactive", lease_id);

    // Transfer NFT back to owner
    let property = contract
        .properties
        .get(&lease.property_id)
        .expect("Property not found");

    contract.tokens.internal_transfer(
        &lease.tenant_id,
        &property.owner_id,
        &lease.property_id.to_string(),
        None,
        None,
    );

    // Update property to remove active lease
    let mut updated_property = property.clone();
    updated_property.active_lease = None;
    contract
        .properties
        .insert(lease.property_id, updated_property);

    emit_event(
        "LeaseExpired",
        LeaseExpiredEvent {
            token_id: lease.property_id,
            tenant_id: lease.tenant_id,
            escrow_returned: 0,
        },
    );
}
