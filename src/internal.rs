use std::str::FromStr;

use near_sdk::{env, json_types::U128, AccountId, Gas, NearToken};

use crate::{
    ft_contract,
    models::{Action, Bid},
    ShedaContract,
};
use near_sdk_contract_tools::nft::Nep177Controller;

pub fn extract_base_uri(url: &str) -> String {
    if let Some(cid) = url.split("/ipfs/").nth(1) {
        return format!("ipfs://{}", cid);
    }

    // fallback base_uri = origin of the URL
    // ex: https://example.com/path/image.png → https://example.com
    url.split('/').take(3).collect::<Vec<_>>().join("/")
}

pub fn internal_accept_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let bid = {
        let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");
        bids.into_iter()
            .find(|b| b.id == bid_id)
            .expect("Bid not found for the property")
            .clone()
    };

    let property = contract
        .properties
        .get(&property_id)
        .expect("Property does not exist");

    assert_eq!(
        property.owner_id,
        env::predecessor_account_id(),
        "Only the property owner can accept bids"
    );

    assert_eq!(
        bid.property_id, property_id,
        "Bid is not for the specified property"
    );

    // Transfer stablecoin from contract to property owner
    #[allow(unused_must_use)]
    ft_contract::ext(bid.stablecoin_token.clone())
        .with_attached_deposit(NearToken::from_yoctonear(1))
        .with_static_gas(Gas::from_tgas(30))
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));

    // Transfer NFT to bidder
    contract.token.internal_transfer(
        &property.owner_id,
        &bid.bidder,
        &property_id.to_string(),
        None,
        None,
    );

    // Remove the bid from storage
    contract
        .bids
        .get_mut(&property_id)
        .unwrap()
        .retain(|b| b.id != bid_id);

    //release other bids for the property
    let remaining_bids = contract.bids.get(&property_id).unwrap().clone();
    for other_bid in remaining_bids.iter() {
        // Refund stablecoin to other bidders
        #[allow(unused_must_use)]
        ft_contract::ext(other_bid.stablecoin_token.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(Gas::from_tgas(30))
            .ft_transfer(other_bid.bidder.clone(), U128(other_bid.amount));

        // Remove the bid from storage
        contract
            .bids
            .get_mut(&property_id)
            .unwrap()
            .retain(|b| b.id != other_bid.id);
    }
    //lease or mark as sold
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
            updated_property.active_lease = Some(crate::models::Lease {
                id: contract.lease_counter,
                property_id,
                tenant_id: bid.bidder.clone(),
                start_time: env::block_timestamp(),
                end_time: env::block_timestamp()
                    + property.lease_duration_months.unwrap() * 30 * 24 * 60 * 60 * 1_000_000_000,
                active: true,
                dispute_status: crate::models::DisputeStatus::None,
                escrow_held: bid.amount,
            });
            contract.lease_counter += 1;
            contract.properties.insert(property_id, updated_property);
        }
    }
}

pub fn internal_reject_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");

    let bid = bids
        .into_iter()
        .find(|b| b.id == bid_id)
        .expect("Bid not found for the property");

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

    // Remove the bid from storage
    contract.bids.remove(&bid_id);
}

pub fn internal_cancel_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let bids: &Vec<Bid> = contract.bids.get(&property_id).expect("Bid does not exist");

    let bid = bids
        .into_iter()
        .find(|b| b.id == bid_id)
        .expect("Bid not found for the property");

    assert_eq!(
        bid.bidder,
        env::predecessor_account_id(),
        "Only the bidder can cancel their bid"
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

    // Remove the bid from storage
    contract.bids.remove(&bid_id);
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
        .expect("Property not found");

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

    //burn the NFT
    // contract.token.internal_transfer(
    //     &property.owner_id,
    //     &get_burn_account_id(),
    //     &property_id.to_string(),
    //     None,
    //     None,
    // );
    
    Nep177Controller::burn_with_metadata(contract, &property_id.to_string(), &env::predecessor_account_id())
            .unwrap_or_else(|e| env::panic_str(&e.to_string()));
    

    // Remove the property from storage
    contract.properties.remove(&property_id);
}

pub fn get_burn_account_id() -> AccountId {
    let acc = env::current_account_id();

    if acc.as_str().ends_with(".testnet") {
        AccountId::from_str("burn.testnet").expect("Failed to convert address")
    } else {
        AccountId::from_str("burn.near").expect("Failed to convert address")
    }
}
