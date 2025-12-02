// Find all our documentation at https://docs.near.org
pub mod admin;
pub mod events;
pub mod internal;
pub mod models;
pub mod views;
#[allow(unused_imports)]
use crate::models::{Bid, ContractError, DisputeStatus, Lease, Property};
use near_sdk::{
    AccountId, collections::LazyOption, env, near, store::{IterableMap, IterableSet}
};
#[allow(unused_imports)]
use near_contract_standards::non_fungible_token::{
    metadata::{NFTContractMetadata, TokenMetadata},
    NonFungibleToken,
};
use crate::internal::*;
// Define the contract structure
#[near(contract_state)]
pub struct ShedaContract {
    pub token: NonFungibleToken,
    pub metadata: LazyOption<NFTContractMetadata>,
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
trait HasNew {
    fn new(media_url: String) -> Self;
}

impl HasNew for NFTContractMetadata {
    fn new(media_url: String) -> Self {
        Self {
            spec: "nft-2.0.0".to_string(),
            name: "Sheda NFT".to_string(),
            symbol: "SHEDA".to_string(),
            icon: Some(media_url.clone()),
            base_uri: Some(format!("ipfs://{}", extract_base_uri(&media_url))),
            reference: None,
            reference_hash: None,
        }
    }
}

// Define the default, which automatically initializes the contract
impl Default for ShedaContract {
    fn default() -> Self {
        Self {
            token: NonFungibleToken::new(
                b"t".to_vec(),
                env::signer_account_id(),
                Some(b"m".to_vec()),
                Some(b"n".to_vec()),
                Some(b"o".to_vec()),

            ),
            metadata: LazyOption::new(b"m".to_vec(), None),
            properties: IterableMap::new(b"p".to_vec()),
            bids: IterableMap::new(b"b".to_vec()),
            leases: IterableMap::new(b"l".to_vec()),
            property_counter: 0,
            bid_counter: 0,
            lease_counter: 0,
            property_per_owner: IterableMap::new(b"o".to_vec()),
            lease_per_tenant: IterableMap::new(b"t".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id: env::signer_account_id(),
        }
    }
}

// Implement the contract structure
#[near]
impl ShedaContract {
    #[init]
    pub fn new(media_url: String) -> Self {
        let owner_id = env::signer_account_id();
        Self {
            token: NonFungibleToken::new(
                b"t".to_vec(),
                owner_id.clone(),
                Some(b"m".to_vec()),
                Some(b"n".to_vec()),
                Some(b"o".to_vec()),
            ),
            metadata: LazyOption::new(
                b"m".to_vec(),
                Some(&NFTContractMetadata::new(media_url)),
            ),
            properties: IterableMap::new(b"p".to_vec()),
            bids: IterableMap::new(b"b".to_vec()),
            leases: IterableMap::new(b"l".to_vec()),
            property_counter: 0,
            bid_counter: 0,
            lease_counter: 0,
            property_per_owner: IterableMap::new(b"o".to_vec()),
            lease_per_tenant: IterableMap::new(b"t".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id: owner_id,
        }
    }

    #[payable]
    pub fn list_property(
        &mut self,
        title: String,
        description: String,
        media_uri: String, // IPFS link to image
        price: u128,
        is_for_sale: bool,
        lease_duration_nanos: Option<u64>,
    ) -> u64 {
        // 1. Calculate IDs
        let property_id = self.property_counter;
        self.property_counter += 1;
        let token_id_str = property_id.to_string(); // NEP-171 requires String IDs

        let owner_id = env::predecessor_account_id();

        // 2. Create Standard NFT Metadata (Visible in Wallets)
        let token_metadata = TokenMetadata {
            title: Some(title),
            description: Some(description.clone()),
            media: Some(media_uri.clone()), // Wallet shows this image
            copies: Some(1),
            ..Default::default()
        };

        // 3. Mint the Standard NFT (Events & Ownership)
        // This handles "property_per_owner" internally via the standard
        self.token.internal_mint(
            token_id_str,
            owner_id.clone(),
            Some(token_metadata),
        );

        // 4. Create Your Custom Property Object
        let property = Property {
            id: property_id,
            owner_id: owner_id.clone(),
            description,
            metadata_uri: media_uri,
            is_for_sale,
            price,
            lease_duration_nanos,
            damage_escrow: 0, // Starts at 0 until leased
            active_lease: None,
        };

        // 5. Save Custom Data
        self.properties.insert(property_id, property);

        // 6. Return the ID for the frontend
        property_id
    }

}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {
    
}
