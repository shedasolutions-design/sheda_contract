// Find all our documentation at https://docs.near.org
pub mod admin;
pub mod events;
pub mod internal;
pub mod models;
pub mod views;
#[allow(unused_imports)]
use crate::models::{Bid, ContractError, DisputeStatus, Lease, Property};
use near_sdk::{
    env, near,
    store::{IterableMap, IterableSet},
    AccountId,
};
#[allow(unused_imports)]
// Define the contract structure
#[near(contract_state)]
pub struct ShedaContract {
    pub properties: IterableMap<u64, Property>,
    pub bids: IterableMap<u64, Vec<Bid>>, //property_id to list of bids
    pub leases: IterableMap<u64, Lease>,

    //tracking
    pub property_counter: u64,
    pub bid_counter: u64,
    pub lease_counter: u64,
    pub property_per_owner: IterableMap<AccountId, Vec<u64>>, //owner to list of property ids
    pub lease_per_tenant: IterableMap<AccountId, Vec<u64>>,   //tenant to list of lease ids
    //admins
    pub admins: IterableSet<AccountId>,
    pub owner_id: AccountId,
    
}

// Define the default, which automatically initializes the contract
impl Default for ShedaContract {
    fn default() -> Self {
        Self {
            properties: IterableMap::new(b"p".to_vec()),
            bids: IterableMap::new(b"b".to_vec()),
            leases: IterableMap::new(b"l".to_vec()),
            property_counter: 0,
            bid_counter: 0,
            lease_counter: 0,
            property_per_owner: IterableMap::new(b"o".to_vec()),
            lease_per_tenant: IterableMap::new(b"t".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id: "penivera.testnet".parse().unwrap(),
        }
    }
}

// Implement the contract structure
#[near]
impl ShedaContract {
    #[init]
    pub fn new() -> Self {
        let owner_id = env::signer_account_id();
        Self {
            properties: IterableMap::new(b"p".to_vec()),
            bids: IterableMap::new(b"b".to_vec()),
            leases: IterableMap::new(b"l".to_vec()),
            property_counter: 0,
            bid_counter: 0,
            lease_counter: 0,
            property_per_owner: IterableMap::new(b"o".to_vec()),
            lease_per_tenant: IterableMap::new(b"t".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id,
        }
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {
    
}
