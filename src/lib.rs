// Find all our documentation at https://docs.near.org
pub mod admin;
pub mod events;
pub mod internal;
pub mod models;
pub mod views;

pub mod ext;
#[allow(unused_imports)]
use crate::models::{Bid, BidStatus, ContractError, DisputeStatus, Lease, Property};
use crate::{internal::*, models::Action};

#[allow(unused_imports)]
use near_contract_standards::non_fungible_token::{
    core::NonFungibleTokenCore,
    metadata::{NFTContractMetadata, TokenMetadata},
    NonFungibleToken, Token,
};
use near_sdk::{
    collections::LazyOption,
    env,
    json_types::U128,
    near, require,
    store::{IterableMap, IterableSet},
    AccountId,
    PanicOnDefault,
};

pub use crate::ext::*;

pub type TokenId = String;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct ShedaContract {
    pub tokens: NonFungibleToken,
    pub metadata: LazyOption<NFTContractMetadata>,
    pub properties: IterableMap<u64, Property>,
    pub bids: IterableMap<u64, Vec<Bid>>, //property_id to list of bids
    pub leases: IterableMap<u64, Lease>,

    //tracking
    pub property_counter: u64,
    pub bid_counter: u64,
    pub lease_counter: u64,
    pub property_per_owner: IterableMap<AccountId, Vec<u64>>, //owner to list of property ids

    pub lease_per_tenant: IterableMap<AccountId, Vec<u64>>, //tenant to list of lease ids
    //admins
    pub admins: IterableSet<AccountId>,
    pub owner_id: AccountId,

    //accepted stablecoin info could go here
    pub accepted_stablecoin: Vec<AccountId>,
    pub stable_coin_balances: IterableMap<AccountId, u128>,
}
trait HasNew {
    fn new(media_url: String) -> Self;
}

//implement NEP-171 standard, checking that nft is not on lease before transfer
#[near]
impl NonFungibleTokenCore for ShedaContract {
    #[payable]
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        let property_id = token_id.parse::<u64>().expect("Invalid token ID");
        let property = self
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        if let Some(lease) = &property.active_lease {
            if lease.active {
                env::panic_str("Cannot transfer property while it is on an active lease");
            }
        }
        self.tokens
            .nft_transfer(receiver_id, token_id, approval_id, memo);
    }

    #[payable]
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> near_sdk::PromiseOrValue<bool> {
        let property_id = token_id.parse::<u64>().expect("Invalid token ID");
        let property = self
            .properties
            .get(&property_id)
            .expect("Property does not exist");
        if let Some(lease) = &property.active_lease {
            if lease.active {
                env::panic_str("Cannot transfer property while it is on an active lease");
            }
        }
        self.tokens
            .nft_transfer_call(receiver_id, token_id, approval_id, memo, msg)
    }

    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        self.tokens.nft_token(token_id)
    }
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

// Implement the contract structure
#[near]
impl ShedaContract {
    //set required init parameters here
    #[init]
    #[private]
    pub fn new(media_url: String, supported_stablecoins: Vec<AccountId>) -> Self {
        assert!(!env::state_exists(), "Contract is already initialized");

        let owner_id = env::predecessor_account_id();

        let mut this = Self {
            tokens: NonFungibleToken::new(
                b"t".to_vec(),
                owner_id.clone(),
                Some(b"m".to_vec()),
                Some(b"n".to_vec()),
                Some(b"o".to_vec()),
            ),
            metadata: LazyOption::new(b"m".to_vec(), Some(&NFTContractMetadata::new(media_url))),
            properties: IterableMap::new(b"p".to_vec()),
            bids: IterableMap::new(b"b".to_vec()),
            leases: IterableMap::new(b"l".to_vec()),
            property_counter: 0,
            bid_counter: 0,
            lease_counter: 0,
            property_per_owner: IterableMap::new(b"o".to_vec()),
            lease_per_tenant: IterableMap::new(b"t".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id: owner_id.clone(),
            accepted_stablecoin: supported_stablecoins.clone(),
            stable_coin_balances: IterableMap::new(b"s".to_vec()),
        };
        this.admins.insert(owner_id);
        for stablecoin in supported_stablecoins {
            this.stable_coin_balances.insert(stablecoin, 0);
        }

        this
    }

    #[payable]
    pub fn mint_property(
        &mut self,
        title: String,
        description: String,
        media_uri: String, // IPFS link to image
        price: u128,
        is_for_sale: bool,
        lease_duration_months: Option<u64>,
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

        //NOTE 3. Mint the Standard NFT (Events & Ownership)
        // This handles "property_per_owner" internally via the standard
        self.tokens
            .internal_mint(token_id_str, owner_id.clone(), Some(token_metadata));

        // 4. Create Your Custom Property Object
        let property = Property {
            id: property_id,
            owner_id: owner_id.clone(),
            description,
            metadata_uri: media_uri,
            is_for_sale,
            price,
            lease_duration_months,
            damage_escrow: 0, // Starts at 0 until leased
            active_lease: None,
            timestamp: env::block_timestamp(),
            sold: None,
        };

        // 5. Save Custom Data
        self.properties.insert(property_id, property);

        // 6. Return the ID for the frontend
        property_id
    }

    //NOTE Placing a Bid
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        let bid_action: models::BidAction =
            serde_json::from_str::<models::BidAction>(&msg).expect("Invalid BidAction");
        let property_id = bid_action.property_id;

        let property = self
            .properties
            .get(&property_id)
            .expect("Property not found");

        // Check if the amount matches the price for sale or lease
        let expected_amount = property.price;

        require!(
            self.accepted_stablecoin
                .contains(&env::predecessor_account_id()),
            "StablecoinNotAccepted"
        );

        require!(
            amount.0 == expected_amount,
            format!(
                "IncorrectBidAmount: expected {}, received {}",
                expected_amount, amount.0
            )
        );

        //assert the property is fo sale if action is sales and for lease if action is lease
        match bid_action.action {
            Action::Purchase => {
                assert!(property.is_for_sale, "Property is not for sale");
            }
            Action::Lease => {
                assert!(
                    property.lease_duration_months.is_some(),
                    "Property is not for lease"
                );
            }
        }

        // Amount matches, create the bid
        let bid_id = self.bid_counter;
        self.bid_counter += 1;

        // Assuming Bid struct has fields: id, property_id, bidder, amount, etc.
        // Adjust based on actual Bid struct definition
        let bid = Bid {
            id: bid_id,
            property_id: property_id,
            bidder: sender_id,
            amount: amount.0,
            created_at: env::block_timestamp(),
            updated_at: env::block_timestamp(),
            status: BidStatus::Pending,
            document_token_id: None,
            escrow_release_tx: None,
            dispute_reason: None,
            action: bid_action.action.clone(),
            stablecoin_token: env::predecessor_account_id(),
        };

        // Insert the bid into the bids map
        self.bids.entry(property_id).or_insert(Vec::new()).push(bid);

        //update stablecoin balance
        let current_balance = *self
            .stable_coin_balances
            .get(&env::predecessor_account_id())
            .unwrap_or(&0);

        self.stable_coin_balances
            .insert(env::predecessor_account_id(), current_balance + amount.0);

        // Returning 0 means: keep all tokens, no refund
        U128(0)
    }

    #[payable]
    pub fn accept_bid(&mut self, bid_id: u64, property_id: u64) -> near_sdk::Promise {
        internal_accept_bid(self, property_id, bid_id)
    }

    #[payable]
    pub fn accept_bid_with_escrow(&mut self, bid_id: u64, property_id: u64) -> bool {
        internal::internal_accept_bid_with_escrow(self, property_id, bid_id)
    }

    #[private]
    pub fn accept_bid_callback(&mut self, property_id: u64, bid_id: u64) {
        internal::accept_bid_callback(self, property_id, bid_id);
    }

    #[payable]
    pub fn reject_bid(&mut self, bid_id: u64, property_id: u64) {
        internal_reject_bid(self, property_id, bid_id);
    }

    #[payable]
    pub fn cancel_bid(&mut self, bid_id: u64, property_id: u64) {
        internal_cancel_bid(self, property_id, bid_id);
    }

    pub fn delist_property(&mut self, property_id: u64) {
        //ensure I own the property

        internal_delist_property(self, property_id);
    }

    #[payable]
    pub fn delete_property(&mut self, property_id: u64) {
        internal_delete_property(self, property_id);
    }

    pub fn raise_lease_dispute(&mut self, lease_id: u64) {
        internal_raise_dispute(self, lease_id);
    }

    pub fn raise_dispute(&mut self, bid_id: u64, property_id: u64, reason: String) -> bool {
        internal::internal_raise_bid_dispute(self, property_id, bid_id, reason)
    }
    
    pub fn expire_lease(&mut self, lease_id: u64) {
        internal::internal_expire_lease(self, lease_id);
    }

    pub fn confirm_document_release(
        &mut self,
        bid_id: u64,
        property_id: u64,
        document_token_id: String,
    ) -> bool {
        internal::internal_confirm_document_release(self, property_id, bid_id, document_token_id)
    }

    pub fn confirm_document_receipt(&mut self, bid_id: u64, property_id: u64) -> bool {
        internal::internal_confirm_document_receipt(self, property_id, bid_id)
    }

    pub fn release_escrow(&mut self, bid_id: u64, property_id: u64) -> near_sdk::Promise {
        internal::internal_release_escrow(self, property_id, bid_id)
    }

    #[private]
    pub fn release_escrow_callback(&mut self, property_id: u64, bid_id: u64) {
        internal::release_escrow_callback(self, property_id, bid_id);
    }

    pub fn complete_transaction(&mut self, bid_id: u64, property_id: u64) -> bool {
        internal::internal_complete_transaction(self, property_id, bid_id)
    }

    pub fn refund_escrow_timeout(
        &mut self,
        bid_id: u64,
        property_id: u64,
        timeout_nanos: u64,
    ) -> near_sdk::Promise {
        internal::internal_refund_escrow_timeout(self, property_id, bid_id, timeout_nanos)
    }

    #[private]
    pub fn refund_escrow_timeout_callback(
        &mut self,
        property_id: u64,
        bid_id: u64,
        stablecoin_token: AccountId,
        amount: u128,
    ) {
        internal::refund_escrow_timeout_callback(self, property_id, bid_id, stablecoin_token, amount);
    }

    // Allow bidders to manually claim/withdraw their bid that was not accepted
    #[payable]
    pub fn claim_lost_bid(&mut self, bid_id: u64, property_id: u64) -> near_sdk::Promise {
        let bids = self.bids.get(&property_id).expect("No bids for this property");
        
        let bid = bids
            .iter()
            .find(|b| b.id == bid_id)
            .expect("Bid not found")
            .clone();

        // Only the bidder can claim their own bid
        assert_eq!(
            bid.bidder,
            env::predecessor_account_id(),
            "Only the bidder can claim their bid"
        );

        // Check if the property has been sold or leased to someone else
        let property = self.properties.get(&property_id).expect("Property not found");
        
        let can_claim = match bid.action {
            crate::models::Action::Purchase => {
                // Can claim if property has been sold to someone else
                property.sold.is_some() && property.sold.as_ref().unwrap().buyer_id != bid.bidder
            },
            crate::models::Action::Lease => {
                // Can claim if property has been leased to someone else
                property.active_lease.is_some() && property.active_lease.as_ref().unwrap().tenant_id != bid.bidder
            }
        };

        assert!(can_claim, "Cannot claim bid: property not yet sold/leased to another party");

        // Refund the bid amount
        let promise = crate::ext::ft_contract::ext(bid.stablecoin_token.clone())
            .with_attached_deposit(near_sdk::NearToken::from_yoctonear(1))
            .with_static_gas(near_sdk::Gas::from_tgas(30))
            .ft_transfer(bid.bidder.clone(), near_sdk::json_types::U128(bid.amount));

        // Update stablecoin balance after refund
        let current_balance = *self
            .stable_coin_balances
            .get(&bid.stablecoin_token)
            .unwrap_or(&0);
        self.stable_coin_balances
            .insert(bid.stablecoin_token.clone(), current_balance.saturating_sub(bid.amount));

        // Return promise and handle callback to update bid status only on success
        promise.then(
            Self::ext(env::current_account_id())
                .with_static_gas(near_sdk::Gas::from_tgas(20))
                .claim_lost_bid_callback(bid_id, property_id, bid.stablecoin_token.clone(), bid.amount)
        )
    }

    #[private]
    pub fn claim_lost_bid_callback(&mut self, bid_id: u64, property_id: u64, stablecoin_token: AccountId, amount: u128) {
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                if let Some(bids) = self.bids.get_mut(&property_id) {
                    let _ = internal::update_bid_in_list(bids, bid_id, |bid| {
                        bid.status = crate::models::BidStatus::Cancelled;
                        bid.updated_at = env::block_timestamp();
                    });
                }

                near_sdk::log!("Bid {} claimed and marked cancelled", bid_id);
            }
            near_sdk::PromiseResult::Failed => {
                // Revert the balance update if transfer failed
                let current_balance = *self
                    .stable_coin_balances
                    .get(&stablecoin_token)
                    .unwrap_or(&0);
                self.stable_coin_balances
                    .insert(stablecoin_token, current_balance + amount);

                near_sdk::log!("Bid claim failed, balance reverted");
            }
        }
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {}
