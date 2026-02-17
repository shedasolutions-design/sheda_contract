use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{log, AccountId};

/// Event emitted when a property is minted
#[derive(Serialize, Deserialize)]
pub struct PropertyMintedEvent {
    pub token_id: u64,
    pub owner_id: AccountId,
    pub metadata_uri: String,
    pub price: u128,
    pub is_for_sale: bool,
    pub lease_duration_nanos: u64,
    pub damage_escrow_amount: u128,
}

/// Event emitted when a bid is placed
#[derive(Serialize, Deserialize)]
pub struct BidPlacedEvent {
    pub token_id: u64,
    pub bidder_id: AccountId,
    pub amount: u128,
    pub created_at: u64,
}

/// Event emitted when a bid is approved by the seller
#[derive(Serialize, Deserialize)]
pub struct BidApprovedEvent {
    pub token_id: u64,
    pub bidder_id: AccountId,
    pub seller_id: AccountId,
    pub amount: u128,
}

/// Event emitted when a bid is rejected by the seller
#[derive(Serialize, Deserialize)]
pub struct BidRejectedEvent {
    pub token_id: u64,
    pub bid_id: u64,
    pub bidder_id: AccountId,
    pub amount: u128,
}

/// Event emitted when a bidder cancels their bid
#[derive(Serialize, Deserialize)]
pub struct BidCancelledEvent {
    pub token_id: u64,
    pub bid_id: u64,
    pub bidder_id: AccountId,
    pub amount: u128,
}

/// Event emitted when a bid is refunded
#[derive(Serialize, Deserialize)]
pub struct BidRefundedEvent {
    pub token_id: u64,
    pub bid_id: u64,
    pub bidder_id: AccountId,
    pub amount: u128,
    pub reason: String,
}

/// Event emitted when a deal is finalized
#[derive(Serialize, Deserialize)]
pub struct DealFinalizedEvent {
    pub token_id: u64,
    pub buyer_id: AccountId,
    pub seller_id: AccountId,
    pub amount: u128,
    pub lease_duration_nanos: u64,
}

/// Event emitted when a dispute is raised
#[derive(Serialize, Deserialize)]
pub struct DisputeRaisedEvent {
    pub token_id: u64,
    pub tenant_id: AccountId,
    pub bond_amount: u128,
}

/// Event emitted when a dispute is resolved by admin
#[derive(Serialize, Deserialize)]
pub struct DisputeResolvedEvent {
    pub token_id: u64,
    pub admin_id: AccountId,
    pub winner_id: AccountId,
    pub escrow_returned: u128,
}

/// Event emitted when a lease expires automatically
#[derive(Serialize, Deserialize)]
pub struct LeaseExpiredEvent {
    pub token_id: u64,
    pub tenant_id: AccountId,
    pub escrow_returned: u128,
}

/// Event emitted when a lost bid is claimed
#[derive(Serialize, Deserialize)]
pub struct LostBidClaimedEvent {
    pub token_id: u64,
    pub bid_id: u64,
    pub bidder_id: AccountId,
    pub amount: u128,
}

/// Event emitted when an admin is added
#[derive(Serialize, Deserialize)]
pub struct AdminAddedEvent {
    pub admin_id: AccountId,
    pub added_by: AccountId,
}

/// Event emitted when an admin is removed
#[derive(Serialize, Deserialize)]
pub struct AdminRemovedEvent {
    pub admin_id: AccountId,
    pub removed_by: AccountId,
}

/// Event emitted during emergency withdrawal
#[derive(Serialize, Deserialize)]
pub struct EmergencyWithdrawalEvent {
    pub amount: u128,
    pub recipient: AccountId,
    pub initiated_by: AccountId,
}

/// Event emitted when owner withdraws stablecoin
#[derive(Serialize, Deserialize)]
pub struct StablecoinWithdrawnEvent {
    pub token_id: AccountId,
    pub amount: u128,
    pub recipient: AccountId,
}

/// Event emitted when a property is delisted by admin
#[derive(Serialize, Deserialize)]
pub struct PropertyDelistedEvent {
    pub token_id: u64,
    pub admin_id: AccountId,
}

/// Event emitted when a property is deleted by admin
#[derive(Serialize, Deserialize)]
pub struct PropertyDeletedEvent {
    pub token_id: u64,
    pub admin_id: AccountId,
}

/// Helper function to emit events in standardized JSON format
pub fn emit_event<T: Serialize>(event_type: &str, event: T) {
    log!(
        "EVENT_JSON:{{\"event_type\":\"{}\",\"data\":{}}}",
        event_type,
        near_sdk::serde_json::to_string(&event).unwrap_or_default()
    );
}
