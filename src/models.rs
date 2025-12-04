use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId, Timestamp,
};

use schemars::JsonSchema;

#[derive(
    BorshDeserialize, BorshSerialize, Deserialize, Serialize, PartialEq, Debug, Clone, JsonSchema,
)]
pub enum DisputeStatus {
    None,
    Raised,
    Resolved,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone)]
pub struct Property {
    pub id: u64,
    pub owner_id: AccountId,
    pub description: String,
    pub metadata_uri: String,
    pub is_for_sale: bool,
    // Price in Stablecoin Atomic Units (e.g., 6 decimals for USDC)
    pub price: u128,
    pub lease_duration_months: Option<u64>, //None if not for lease
    pub damage_escrow: u128,                // Amount held for damages
    pub active_lease: Option<Lease>,
    pub timestamp: Timestamp,
    pub sold: Option<Sold>,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone)]
pub struct Sold {
    pub property_id: u64,
    pub buyer_id: AccountId,
    pub amount: u128,
    pub previous_owner_id: AccountId,
    pub sold_at: Timestamp,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone)]
pub struct Bid {
    pub id: u64,
    pub bidder: AccountId,
    pub property_id: u64,
    pub amount: u128,
    pub created_at: Timestamp,
    pub action: Action,
    pub stablecoin_token: AccountId,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct BidAction {
    pub property_id: u64,
    pub action: Action,
    pub stablecoin_token: AccountId,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum Action {
    Purchase,
    Lease,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone, JsonSchema)]
pub struct Lease {
    pub id: u64,
    pub property_id: u64,
    #[schemars(skip)]
    pub tenant_id: AccountId,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub active: bool,
    pub dispute_status: DisputeStatus,
    pub escrow_held: u128,
}

// Kept your error handling, it is clean.
#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, PartialEq, Debug, Clone)]
pub enum ContractError {
    PropertyNotFound,
    NotPropertyOwner,
    InvalidBidAmount,
    LeaseNotActive,
    UnauthorizedAccess,
    DisputeAlreadyRaised,
    LeaseNotFound,
    
    // Added for Stablecoin logic
    InvalidPaymentToken,
    IncorrectBidAmount { expected: u128, received: u128 },
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::PropertyNotFound => write!(f, "Property not found"),
            ContractError::NotPropertyOwner => write!(f, "Not property owner"),
            ContractError::InvalidBidAmount => write!(f, "Invalid bid amount"),
            ContractError::LeaseNotActive => write!(f, "Lease not active"),
            ContractError::UnauthorizedAccess => write!(f, "Unauthorized access"),
            ContractError::DisputeAlreadyRaised => write!(f, "Dispute already raised"),
            ContractError::LeaseNotFound => write!(f, "Lease not found"),
            ContractError::InvalidPaymentToken => write!(f, "Invalid payment token"),
            ContractError::IncorrectBidAmount { expected, received } => write!(
                f,
                "Incorrect bid amount: expected {}, received {}",
                expected, received
            ),
        }
    }
}

impl std::error::Error for ContractError {}

impl AsRef<str> for ContractError {
    fn as_ref(&self) -> &str {
        match self {
            ContractError::PropertyNotFound => "PropertyNotFound",
            ContractError::NotPropertyOwner => "NotPropertyOwner",
            ContractError::InvalidBidAmount => "InvalidBidAmount",
            ContractError::LeaseNotActive => "LeaseNotActive",
            ContractError::UnauthorizedAccess => "UnauthorizedAccess",
            ContractError::DisputeAlreadyRaised => "DisputeAlreadyRaised",
            ContractError::LeaseNotFound => "LeaseNotFound",
            ContractError::InvalidPaymentToken => "InvalidPaymentToken",
            ContractError::IncorrectBidAmount { .. } => "IncorrectBidAmount",
        }
    }
}

//SECTION -  View structs
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PropertyView {
    pub id: u64,
    pub owner_id: String,
    pub description: String,
    pub metadata_uri: String,
    pub is_for_sale: bool,
    pub price: u128,
    pub lease_duration_nanos: Option<u64>,
    pub damage_escrow: u128,
    pub active_lease: Option<LeaseView>,
    pub timestamp: Timestamp,
    pub sold: Option<SoldView>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LeaseView {
    pub id: u64,
    pub property_id: u64,
    pub tenant_id: String,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub active: bool,
    pub dispute_status: DisputeStatus,
    pub escrow_held: u128,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct BidView {
    pub id: u64,
    pub bidder_id: String,
    pub property_id: u64,
    pub bid_amount: u128,
    pub created_at: Timestamp,
    pub action: Action,
    pub stablecoin_token: String,
}

impl Property {
    pub fn to_view(&self) -> PropertyView {
        PropertyView {
            id: self.id,
            owner_id: self.owner_id.to_string(),
            description: self.description.clone(),
            metadata_uri: self.metadata_uri.clone(),
            is_for_sale: self.is_for_sale,
            price: self.price,
            lease_duration_nanos: self.lease_duration_months,
            damage_escrow: self.damage_escrow,
            active_lease: self.active_lease.as_ref().map(|l| l.to_view()),
            timestamp: self.timestamp,
            sold: self.sold.as_ref().map(|s| s.to_view()),
        }
    }
}

impl Lease {
    pub fn to_view(&self) -> LeaseView {
        LeaseView {
            id: self.id,
            property_id: self.property_id,
            tenant_id: self.tenant_id.to_string(),
            start_time: self.start_time,
            end_time: self.end_time,
            active: self.active,
            dispute_status: self.dispute_status.clone(),
            escrow_held: self.escrow_held,
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SoldView {
    pub property_id: u64,
    pub buyer_id: String,
    pub amount: u128,
    pub previous_owner_id: String,
    pub sold_at: Timestamp,
}

impl Sold {
    pub fn to_view(&self) -> SoldView {
        SoldView {
            property_id: self.property_id,
            buyer_id: self.buyer_id.to_string(),
            amount: self.amount,
            previous_owner_id: self.previous_owner_id.to_string(),
            sold_at: self.sold_at,
        }
    }
}
