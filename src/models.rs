use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId, Timestamp,
};

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, PartialEq, Debug, Clone)]
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
    pub lease_duration_nanos: Option<u64>, //None if not for lease
    pub damage_escrow: u128,       // Amount held for damages
    pub active_lease: Option<Lease>,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize,Clone)]
pub struct Bid {
    pub id: u64,
    pub bidder_id: AccountId,
    pub property_id: u64,
    pub bid_amount: u128,
    pub created_at: Timestamp,
}

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone)]
pub struct Lease {
    pub id: u64,
    pub property_id: u64,
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
        }
    }
}