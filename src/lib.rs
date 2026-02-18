// Find all our documentation at https://docs.near.org
pub mod admin;
pub mod events;
pub mod internal;
pub mod models;
pub mod views;

pub mod ext;
#[allow(unused_imports)]
use crate::models::{Bid, BidStatus, ContractError, DisputeStatus, Lease, Property};
use crate::events::{emit_event, BidPlacedEvent, LostBidClaimedEvent, PropertyMintedEvent};
use crate::{internal::*, models::Action};

#[allow(unused_imports)]
use near_contract_standards::non_fungible_token::{
    approval::NonFungibleTokenApproval,
    core::NonFungibleTokenCore,
    enumeration::NonFungibleTokenEnumeration,
    metadata::{NFTContractMetadata, TokenMetadata},
    metadata::NonFungibleTokenMetadataProvider,
    NonFungibleToken, Token,
};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::LazyOption,
    env,
    json_types::{Base64VecU8, U128},
    near, require,
    store::{IterableMap, IterableSet},
    AccountId, Gas, NearToken, Promise,
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

    // Guards and configuration
    pub reentrancy_locks: IterableSet<String>,
    pub bid_expiry_ns: u64,
    pub escrow_release_delay_ns: u64,
    pub lost_bid_claim_delay_ns: u64,

    // Global contract factory
    pub global_contract_code: Option<Vec<u8>>,
    pub property_instances: IterableMap<u64, AccountId>,

    // Dispute oracle
    pub oracle_account_id: Option<AccountId>,
    pub oracle_request_nonce: u64,

    // Upgrade governance
    pub upgrade_delay_ns: u64,
    pub pending_upgrade_code: Option<Vec<u8>>,
    pub pending_upgrade_at: Option<u64>,

    pub version: u32,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldBid {
    pub id: u64,
    pub bidder: AccountId,
    pub property_id: u64,
    pub amount: u128,
    pub created_at: u64,
    pub updated_at: u64,
    pub status: BidStatus,
    pub document_token_id: Option<String>,
    pub escrow_release_tx: Option<String>,
    pub dispute_reason: Option<String>,
    pub action: Action,
    pub stablecoin_token: AccountId,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldLease {
    pub id: u64,
    pub property_id: u64,
    pub tenant_id: AccountId,
    pub start_time: u64,
    pub end_time: u64,
    pub active: bool,
    pub dispute_status: DisputeStatus,
    pub escrow_held: u128,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldShedaContract {
    pub tokens: NonFungibleToken,
    pub metadata: LazyOption<NFTContractMetadata>,
    pub properties: IterableMap<u64, Property>,
    pub bids: IterableMap<u64, Vec<OldBid>>, //property_id to list of bids
    pub leases: IterableMap<u64, OldLease>,
    pub property_counter: u64,
    pub bid_counter: u64,
    pub lease_counter: u64,
    pub property_per_owner: IterableMap<AccountId, Vec<u64>>,
    pub lease_per_tenant: IterableMap<AccountId, Vec<u64>>,
    pub admins: IterableSet<AccountId>,
    pub owner_id: AccountId,
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

#[near]
impl NonFungibleTokenApproval for ShedaContract {
    #[payable]
    fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Option<near_sdk::Promise> {
        self.tokens.nft_approve(token_id, account_id, msg)
    }

    #[payable]
    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) {
        self.tokens.nft_revoke(token_id, account_id)
    }

    #[payable]
    fn nft_revoke_all(&mut self, token_id: TokenId) {
        self.tokens.nft_revoke_all(token_id)
    }

    fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> bool {
        self.tokens
            .nft_is_approved(token_id, approved_account_id, approval_id)
    }
}

#[near]
impl NonFungibleTokenEnumeration for ShedaContract {
    fn nft_total_supply(&self) -> U128 {
        self.tokens.nft_total_supply()
    }

    fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token> {
        self.tokens.nft_tokens(from_index, limit)
    }

    fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        self.tokens.nft_supply_for_owner(account_id)
    }

    fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<Token> {
        self.tokens
            .nft_tokens_for_owner(account_id, from_index, limit)
    }
}

#[near]
impl NonFungibleTokenMetadataProvider for ShedaContract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().expect("Metadata not found")
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

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can call"
        );
    }

    fn assert_admin(&self) {
        require!(self.admins.contains(&env::predecessor_account_id()), "UnauthorizedAccess");
    }

    //set required init parameters here
    #[init]
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
            property_per_owner: IterableMap::new(b"po".to_vec()),
            lease_per_tenant: IterableMap::new(b"lt".to_vec()),
            admins: IterableSet::new(b"a".to_vec()),
            owner_id: owner_id.clone(),
            accepted_stablecoin: supported_stablecoins.clone(),
            stable_coin_balances: IterableMap::new(b"s".to_vec()),
            reentrancy_locks: IterableSet::new(b"rl".to_vec()),
            bid_expiry_ns: 7 * 24 * 60 * 60 * 1_000_000_000,
            escrow_release_delay_ns: 24 * 60 * 60 * 1_000_000_000,
            lost_bid_claim_delay_ns: 24 * 60 * 60 * 1_000_000_000,
            global_contract_code: None,
            property_instances: IterableMap::new(b"pi".to_vec()),
            oracle_account_id: None,
            oracle_request_nonce: 0,
            upgrade_delay_ns: 0,
            pending_upgrade_code: None,
            pending_upgrade_at: None,
            version: 2,
        };
        this.admins.insert(owner_id);
        for stablecoin in supported_stablecoins {
            this.stable_coin_balances.insert(stablecoin, 0);
        }

        this
    }

    /// Upgrade hook to migrate state from a previous version.
    #[init(ignore_state)]
    #[private]
    pub fn migrate() -> Self {
        let old: OldShedaContract = env::state_read().expect("Old state does not exist");
        let default_token = old
            .accepted_stablecoin
            .first()
            .cloned()
            .unwrap_or_else(|| old.owner_id.clone());

        let mut bids = IterableMap::new(b"b".to_vec());
        for (property_id, old_bids) in old.bids.iter() {
            let upgraded_bids: Vec<Bid> = old_bids
                .iter()
                .map(|bid| Bid {
                    id: bid.id,
                    bidder: bid.bidder.clone(),
                    property_id: bid.property_id,
                    amount: bid.amount,
                    created_at: bid.created_at,
                    updated_at: bid.updated_at,
                    status: bid.status.clone(),
                    document_token_id: bid.document_token_id.clone(),
                    escrow_release_tx: bid.escrow_release_tx.clone(),
                    dispute_reason: bid.dispute_reason.clone(),
                    expires_at: None,
                    escrow_release_after: None,
                    action: bid.action.clone(),
                    stablecoin_token: bid.stablecoin_token.clone(),
                })
                .collect();
            bids.insert(*property_id, upgraded_bids);
        }

        let mut leases = IterableMap::new(b"l".to_vec());
        for (lease_id, old_lease) in old.leases.iter() {
            leases.insert(
                *lease_id,
                Lease {
                    id: old_lease.id,
                    property_id: old_lease.property_id,
                    tenant_id: old_lease.tenant_id.clone(),
                    start_time: old_lease.start_time,
                    end_time: old_lease.end_time,
                    active: old_lease.active,
                    dispute_status: old_lease.dispute_status.clone(),
                    dispute: None,
                    escrow_held: old_lease.escrow_held,
                    escrow_token: default_token.clone(),
                },
            );
        }
        let upgraded = Self {
            tokens: old.tokens,
            metadata: old.metadata,
            properties: old.properties,
            bids,
            leases,
            property_counter: old.property_counter,
            bid_counter: old.bid_counter,
            lease_counter: old.lease_counter,
            property_per_owner: old.property_per_owner,
            lease_per_tenant: old.lease_per_tenant,
            admins: old.admins,
            owner_id: old.owner_id,
            accepted_stablecoin: old.accepted_stablecoin,
            stable_coin_balances: old.stable_coin_balances,
            reentrancy_locks: IterableSet::new(b"rl".to_vec()),
            bid_expiry_ns: 7 * 24 * 60 * 60 * 1_000_000_000,
            escrow_release_delay_ns: 24 * 60 * 60 * 1_000_000_000,
            lost_bid_claim_delay_ns: 24 * 60 * 60 * 1_000_000_000,
            global_contract_code: None,
            property_instances: IterableMap::new(b"pi".to_vec()),
            oracle_account_id: None,
            oracle_request_nonce: 0,
            upgrade_delay_ns: 0,
            pending_upgrade_code: None,
            pending_upgrade_at: None,
            version: 2,
        };

        upgraded
    }

    /// Owner-only contract upgrade entrypoint.
    #[payable]
    pub fn upgrade_self(&mut self, code: Base64VecU8) -> near_sdk::Promise {
        self.assert_owner();
        require!(env::attached_deposit().as_yoctonear() > 0, "Attach deposit");

        Promise::new(env::current_account_id())
            .deploy_contract(code.0)
            .then(Self::ext(env::current_account_id()).migrate())
    }

    #[payable]
    pub fn set_upgrade_delay(&mut self, delay_ns: u64) {
        self.assert_owner();
        self.upgrade_delay_ns = delay_ns;
    }

    #[payable]
    pub fn propose_upgrade(&mut self, code: Base64VecU8) {
        self.assert_owner();
        require!(
            self.pending_upgrade_code.is_none(),
            "Pending upgrade exists"
        );
        self.pending_upgrade_code = Some(code.0);
        self.pending_upgrade_at = Some(env::block_timestamp());
    }

    #[payable]
    pub fn apply_upgrade(&mut self) -> near_sdk::Promise {
        self.assert_owner();
        let proposed_at = self
            .pending_upgrade_at
            .expect("No pending upgrade");
        require!(
            env::block_timestamp() >= proposed_at + self.upgrade_delay_ns,
            "Upgrade delay not reached"
        );

        let code = self
            .pending_upgrade_code
            .take()
            .expect("No pending upgrade");
        self.pending_upgrade_at = None;

        Promise::new(env::current_account_id())
            .deploy_contract(code)
            .then(Self::ext(env::current_account_id()).migrate())
    }

    /// Store global contract code bytes for per-property instances.
    #[payable]
    pub fn set_global_contract_code(&mut self, code: Base64VecU8) {
        self.assert_owner();
        require!(env::attached_deposit().as_yoctonear() > 0, "Attach deposit");
        self.global_contract_code = Some(code.0);
    }

    /// Update bid expiry and escrow timelock settings (nanoseconds).
    pub fn set_time_lock_config(
        &mut self,
        bid_expiry_ns: u64,
        escrow_release_delay_ns: u64,
        lost_bid_claim_delay_ns: u64,
    ) {
        self.assert_owner();
        self.bid_expiry_ns = bid_expiry_ns;
        self.escrow_release_delay_ns = escrow_release_delay_ns;
        self.lost_bid_claim_delay_ns = lost_bid_claim_delay_ns;
    }

    /// Deploy a per-property instance under a subaccount.
    #[payable]
    pub fn create_property_instance(&mut self, property_id: u64) -> Promise {
        self.assert_owner();
        require!(
            self.properties.get(&property_id).is_some(),
            "Property not found"
        );
        let code = self
            .global_contract_code
            .clone()
            .expect("Global contract code not set");

        let escrow_token = self
            .accepted_stablecoin
            .first()
            .cloned()
            .unwrap_or_else(|| self.owner_id.clone());

        let subaccount: AccountId = format!("{}.{}", property_id, env::current_account_id())
            .parse()
            .expect("Invalid subaccount");
        let initial_balance = NearToken::from_near(1);
        require!(
            env::attached_deposit() >= initial_balance,
            "Insufficient deposit for instance creation"
        );

        let deploy = Promise::new(subaccount.clone())
            .create_account()
            .transfer(initial_balance)
            .deploy_contract(code);

        deploy
            .then(
                property_instance::ext(subaccount.clone())
                    .with_static_gas(Gas::from_tgas(20))
                    .new(self.owner_id.clone(), property_id, escrow_token),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .on_property_instance_created(property_id, subaccount),
            )
    }

    #[private]
    pub fn on_property_instance_created(&mut self, property_id: u64, account_id: AccountId) {
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                self.property_instances.insert(property_id, account_id);
            }
            _ => env::panic_str("Property instance creation failed"),
        }
    }

    #[payable]
    pub fn mint_property(
        &mut self,
        title: String,
        description: String,
        media_uri: String, // IPFS link to image
        price: U128,
        is_for_sale: bool,
        lease_duration_months: Option<u64>,
    ) -> u64 {
        // 1. Calculate IDs
        let property_id = self.property_counter;
        self.property_counter = Self::checked_add_u64(self.property_counter, 1, "property_counter");
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
            metadata_uri: media_uri.clone(),
            is_for_sale,
            price: price.0,
            lease_duration_months,
            damage_escrow: 0, // Starts at 0 until leased
            active_lease: None,
            timestamp: env::block_timestamp(),
            sold: None,
        };

        // 5. Save Custom Data
        self.properties.insert(property_id, property);

        let mut owner_properties = self
            .property_per_owner
            .get(&owner_id)
            .cloned()
            .unwrap_or_default();
        owner_properties.push(property_id);
        self.property_per_owner
            .insert(owner_id.clone(), owner_properties);

        emit_event(
            "PropertyMinted",
            PropertyMintedEvent {
                token_id: property_id,
                owner_id: owner_id.clone(),
                metadata_uri: media_uri,
                price: price.0,
                is_for_sale,
                lease_duration_nanos: lease_duration_months.unwrap_or(0)
                    * 30
                    * 24
                    * 60
                    * 60
                    * 1_000_000_000,
                damage_escrow_amount: 0,
            },
        );

        // 6. Return the ID for the frontend
        property_id
    }

    //NOTE Placing a Bid
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        let bid_action: models::BidAction =
            serde_json::from_str::<models::BidAction>(&msg).expect("Invalid BidAction");
        let property_id = bid_action.property_id;
        let sender_id_guard = sender_id.clone();


        let property = self
            .properties
            .get(&property_id)
            .expect("Property not found");

        // Amount can be any value; price is advisory for frontends.

        require!(
            self.accepted_stablecoin
                .contains(&env::predecessor_account_id()),
            "StablecoinNotAccepted"
        );

        require!(
            bid_action.stablecoin_token == env::predecessor_account_id(),
            "StablecoinMismatch"
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
        self.bid_counter = Self::checked_add_u64(self.bid_counter, 1, "bid_counter");

        // Assuming Bid struct has fields: id, property_id, bidder, amount, etc.
        // Adjust based on actual Bid struct definition
        let expires_at = if self.bid_expiry_ns == 0 {
            None
        } else {
            Some(env::block_timestamp() + self.bid_expiry_ns)
        };

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
            expires_at,
            escrow_release_after: None,
            action: bid_action.action.clone(),
            stablecoin_token: env::predecessor_account_id(),
        };

        internal::lock_ft_on_transfer(self, property_id, &sender_id_guard);

        emit_event(
            "BidPlaced",
            BidPlacedEvent {
                token_id: property_id,
                bidder_id: bid.bidder.clone(),
                amount: bid.amount,
                created_at: bid.created_at,
            },
        );

        // Insert the bid into the bids map
        self.bids.entry(property_id).or_insert(Vec::new()).push(bid);

        //update stablecoin balance
        let current_balance = *self
            .stable_coin_balances
            .get(&env::predecessor_account_id())
            .unwrap_or(&0);

        self.stable_coin_balances.insert(
            env::predecessor_account_id(),
            Self::checked_add_u128(current_balance, amount.0, "bid deposit"),
        );

        // Returning 0 means: keep all tokens, no refund
        internal::unlock_ft_on_transfer(self, property_id, &sender_id_guard);
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

    #[payable]
    pub fn delist_property(&mut self, property_id: u64) {
        //ensure I own the property

        internal_delist_property(self, property_id);
    }

    #[payable]
    pub fn delete_property(&mut self, property_id: u64) {
        internal_delete_property(self, property_id);
    }

    pub fn raise_lease_dispute(&mut self, lease_id: u64) {
        internal_raise_dispute(self, lease_id, "".to_string());
    }

    pub fn raise_lease_dispute_with_reason(&mut self, lease_id: u64, reason: String) {
        internal_raise_dispute(self, lease_id, reason);
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

    #[payable]
    pub fn cron_check_leases(&mut self) -> bool {
        let now = env::block_timestamp();
        let expired_ids: Vec<u64> = self
            .leases
            .iter()
            .filter_map(|(id, lease)| {
                if lease.active && lease.end_time <= now {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        for lease_id in expired_ids {
            internal::internal_expire_lease(self, lease_id);
        }

        true
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

        require!(
            bid.status == BidStatus::Pending || bid.status == BidStatus::Rejected,
            "Bid is not claimable"
        );

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

        let claimable_at = match bid.action {
            crate::models::Action::Purchase => property
                .sold
                .as_ref()
                .map(|sold| sold.sold_at + self.lost_bid_claim_delay_ns),
            crate::models::Action::Lease => property
                .active_lease
                .as_ref()
                .map(|lease| lease.start_time + self.lost_bid_claim_delay_ns),
        };
        if let Some(unlock_at) = claimable_at {
            require!(
                env::block_timestamp() >= unlock_at,
                "Lost bid claim timelock not reached"
            );
        }

        internal::lock_bid(self, property_id, bid_id);

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
        self.stable_coin_balances.insert(
            bid.stablecoin_token.clone(),
            Self::checked_sub_u128(current_balance, bid.amount, "lost_bid refund"),
        );

        // Return promise and handle callback to update bid status only on success
        promise.then(
            Self::ext(env::current_account_id())
                .with_static_gas(near_sdk::Gas::from_tgas(20))
                .claim_lost_bid_callback(bid_id, property_id, bid.stablecoin_token.clone(), bid.amount)
        )
    }

    #[private]
    pub fn claim_lost_bid_callback(&mut self, bid_id: u64, property_id: u64, stablecoin_token: AccountId, amount: u128) {
        internal::unlock_bid(self, property_id, bid_id);
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                let bidder_id = self
                    .bids
                    .get(&property_id)
                    .and_then(|bids| bids.iter().find(|bid| bid.id == bid_id))
                    .map(|bid| bid.bidder.clone())
                    .unwrap_or_else(|| env::predecessor_account_id());
                if let Some(bids) = self.bids.get_mut(&property_id) {
                    let _ = internal::update_bid_in_list(bids, bid_id, |bid| {
                        bid.status = crate::models::BidStatus::Cancelled;
                        bid.updated_at = env::block_timestamp();
                    });
                }

                near_sdk::log!("Bid {} claimed and marked cancelled", bid_id);

                emit_event(
                    "LostBidClaimed",
                    LostBidClaimedEvent {
                        token_id: property_id,
                        bid_id,
                        bidder_id,
                        amount,
                    },
                );
            }
            near_sdk::PromiseResult::Failed => {
                // Revert the balance update if transfer failed
                let current_balance = *self
                    .stable_coin_balances
                    .get(&stablecoin_token)
                    .unwrap_or(&0);
                self.stable_coin_balances.insert(
                    stablecoin_token,
                    Self::checked_add_u128(current_balance, amount, "lost_bid revert"),
                );

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
