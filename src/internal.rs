use near_contract_standards::non_fungible_token::core::NonFungibleTokenCore;
use near_sdk::{assert_one_yocto, env, json_types::U128, log, require, AccountId, Gas, NearToken, Promise, PromiseResult};

use crate::{
    ext::ft_contract,
    models::{Action, Bid, BidStatus},
    ShedaContract,
};

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
        format!("Insufficient storage deposit. Required at least {}", required)
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
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept bids"
    );

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
            bid.updated_at = env::block_timestamp();
        })
    };

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    // Part 1: Transfer stablecoin from contract to property owner
    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));

    // Update stablecoin balance after payment to seller
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract
        .stable_coin_balances
        .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

    // Part 2: Callback to handle success/failure
    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(50))
            .accept_bid_callback(property_id, bid_id)
    )
}

// Callback to handle the result of ft_transfer
pub fn accept_bid_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    // Check if the promise succeeded
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            log!("ft_transfer successful, proceeding with NFT transfer and bid updates");

            let property = contract
                .properties
                .get(&property_id)
                .expect("Property does not exist");

            let bid = {
                let bids = contract
                    .bids
                    .get(&property_id)
                    .expect("Bid does not exist");
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

                    if env::used_gas().as_gas()
                        >= env::prepaid_gas().as_gas() - Gas::from_tgas(40).as_gas()
                    {
                        continue;
                    }

                    #[allow(unused_must_use)]
                    ft_contract::ext(other_bid.stablecoin_token.clone())
                        .with_attached_deposit(NearToken::from_yoctonear(1))
                        .with_static_gas(Gas::from_tgas(30))
                        .ft_transfer(other_bid.bidder.clone(), U128(other_bid.amount));

                    let current_balance = *contract
                        .stable_coin_balances
                        .get(&other_bid.stablecoin_token)
                        .unwrap_or(&0);
                    contract.stable_coin_balances.insert(
                        other_bid.stablecoin_token.clone(),
                        current_balance.saturating_sub(other_bid.amount),
                    );

                    other_bid.status = BidStatus::Rejected;
                    other_bid.updated_at = env::block_timestamp();
                }
            }

            match bid.action {
                Action::Purchase => {
                    let mut updated_property = property.clone();
                    updated_property.sold = Some(crate::models::Sold {
                        property_id,
                        buyer_id: bid.bidder.clone(),
                        amount: bid.amount,
                        previous_owner_id: property.owner_id.clone(),
                        sold_at: env::block_timestamp(),
                    });
                    updated_property.is_for_sale = false;
                    contract.properties.insert(property_id, updated_property);
                }
                Action::Lease => {
                    let mut updated_property = property.clone();
                    let lease = crate::models::Lease {
                        id: contract.lease_counter,
                        property_id,
                        tenant_id: bid.bidder.clone(),
                        start_time: env::block_timestamp(),
                        end_time: env::block_timestamp()
                            + property.lease_duration_months.unwrap() * 30 * 24 * 60 * 60 * 1_000_000_000,
                        active: true,
                        dispute_status: crate::models::DisputeStatus::None,
                        escrow_held: bid.amount,
                    };
                    updated_property.active_lease = Some(lease.clone());
                    contract.leases.insert(lease.id, lease);
                    contract.lease_counter += 1;
                    contract.properties.insert(property_id, updated_property);
                }
            }
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
            contract
                .stable_coin_balances
                .insert(bid.stablecoin_token.clone(), current_balance + bid.amount);

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::Pending;
                    bid.updated_at = env::block_timestamp();
                });
            }

            env::panic_str("Payment transfer failed. Bid acceptance aborted.");
        }
    }
}

pub fn internal_reject_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
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
    #[allow(unused_must_use)]
    ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));

    // Update stablecoin balance after refund
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract
        .stable_coin_balances
        .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Rejected;
            bid.updated_at = env::block_timestamp();
        });
    }
}

pub fn internal_cancel_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
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
    let property = contract.properties.get(&property_id).expect("Property does not exist");
    
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
    #[allow(unused_must_use)]
    ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));

    // Update stablecoin balance after refund
    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract
        .stable_coin_balances
        .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Pending {
                env::panic_str("Bid is not in a pending state");
            }
            bid.status = BidStatus::Cancelled;
            bid.updated_at = env::block_timestamp();
        });
    }
}

pub fn internal_accept_bid_with_escrow(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept bids"
    );

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
            bid.updated_at = env::block_timestamp();
        })
    };

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        for other_bid in bids.iter_mut() {
            if other_bid.id == bid_id || other_bid.status != BidStatus::Pending {
                continue;
            }

            if env::used_gas().as_gas()
                >= env::prepaid_gas().as_gas() - Gas::from_tgas(40).as_gas()
            {
                continue;
            }

            #[allow(unused_must_use)]
            ft_contract::ext(other_bid.stablecoin_token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(Gas::from_tgas(30))
                .ft_transfer(other_bid.bidder.clone(), U128(other_bid.amount));

            let current_balance = *contract
                .stable_coin_balances
                .get(&other_bid.stablecoin_token)
                .unwrap_or(&0);
            contract.stable_coin_balances.insert(
                other_bid.stablecoin_token.clone(),
                current_balance.saturating_sub(other_bid.amount),
            );

            other_bid.status = BidStatus::Rejected;
            other_bid.updated_at = env::block_timestamp();
        }
    }

    true
}

pub fn internal_confirm_document_release(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    document_token_id: String,
) -> bool {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can release documents"
    );

    if let Some(bids) = contract.bids.get_mut(&property_id) {
        let _ = update_bid_in_list(bids, bid_id, |bid| {
            if bid.status != BidStatus::Accepted {
                env::panic_str("Bid is not in an accepted state");
            }
            bid.status = BidStatus::DocsReleased;
            bid.updated_at = env::block_timestamp();
            bid.document_token_id = Some(document_token_id);
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
            if bid.bidder != env::predecessor_account_id() {
                env::panic_str("Only the bidder can confirm receipt");
            }
            bid.status = BidStatus::DocsConfirmed;
            bid.updated_at = env::block_timestamp();
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

    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));

    let current_balance = *contract
        .stable_coin_balances
        .get(&bid.stablecoin_token)
        .unwrap_or(&0);
    contract
        .stable_coin_balances
        .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

    promise.then(
        crate::ShedaContract::ext(env::current_account_id())
            .with_static_gas(Gas::from_tgas(50))
            .release_escrow_callback(property_id, bid_id),
    )
}

pub fn release_escrow_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            let property = contract
                .properties
                .get(&property_id)
                .expect("Property does not exist");

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

                    let mut updated_property = property.clone();
                    updated_property.sold = Some(crate::models::Sold {
                        property_id,
                        buyer_id: bid.bidder.clone(),
                        amount: bid.amount,
                        previous_owner_id: property.owner_id.clone(),
                        sold_at: env::block_timestamp(),
                    });
                    updated_property.is_for_sale = false;
                    contract.properties.insert(property_id, updated_property);
                }
                Action::Lease => {
                    contract.tokens.internal_transfer(
                        &property.owner_id,
                        &bid.bidder,
                        &property_id.to_string(),
                        None,
                        None,
                    );

                    let mut updated_property = property.clone();
                    let lease = crate::models::Lease {
                        id: contract.lease_counter,
                        property_id,
                        tenant_id: bid.bidder.clone(),
                        start_time: env::block_timestamp(),
                        end_time: env::block_timestamp()
                            + property.lease_duration_months.unwrap() * 30 * 24 * 60 * 60 * 1_000_000_000,
                        active: true,
                        dispute_status: crate::models::DisputeStatus::None,
                        escrow_held: bid.amount,
                    };
                    updated_property.active_lease = Some(lease.clone());
                    contract.leases.insert(lease.id, lease);
                    contract.lease_counter += 1;
                    contract.properties.insert(property_id, updated_property);
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
            contract
                .stable_coin_balances
                .insert(bid.stablecoin_token.clone(), current_balance + bid.amount);

            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::DocsConfirmed;
                    bid.updated_at = env::block_timestamp();
                });
            }

            env::panic_str("Escrow release failed. Payment transfer aborted.");
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
            bid.dispute_reason = Some(reason);
        });
    } else {
        env::panic_str("Bid does not exist");
    }

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
    contract
        .stable_coin_balances
        .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

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
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            if let Some(bids) = contract.bids.get_mut(&property_id) {
                let _ = update_bid_in_list(bids, bid_id, |bid| {
                    bid.status = BidStatus::Cancelled;
                    bid.updated_at = env::block_timestamp();
                    bid.escrow_release_tx = Some(format!("refund:{}", env::block_height()));
                });
            }
        }
        PromiseResult::Failed => {
            let current_balance = *contract
                .stable_coin_balances
                .get(&stablecoin_token)
                .unwrap_or(&0);
            contract
                .stable_coin_balances
                .insert(stablecoin_token, current_balance + amount);

            env::panic_str("Timeout refund failed. Balance reverted.");
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
}

pub fn internal_delete_property(contract: &mut ShedaContract, property_id: u64) {
    let property = contract
        .properties
        .get(&property_id)
        .expect("Property not found")
        .clone();

    assert_eq!(
        property.owner_id,
        env::signer_account_id(),
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
}

pub fn internal_raise_dispute(contract: &mut ShedaContract, lease_id: u64) {
    let mut lease = contract
        .leases
        .get(&lease_id)
        .cloned()
        .expect("Lease not found");

    assert_eq!(
        lease.tenant_id,
        env::predecessor_account_id(),
        "Only the tenant can raise a dispute"
    );

    assert_eq!(
        lease.dispute_status,
        crate::models::DisputeStatus::None,
        "Dispute already raised for this lease"
    );

    lease.dispute_status = crate::models::DisputeStatus::Raised;

    contract.leases.insert(lease_id, lease);
}

pub fn internal_expire_lease(contract: &mut ShedaContract, lease_id: u64) {
    let mut lease = contract
        .leases
        .get(&lease_id)
        .cloned()
        .expect("Lease not found");

    let current_time = env::block_timestamp();

    // Check if lease has expired
    require!(
        lease.end_time <= current_time,
        "Lease has not expired yet"
    );

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
}
