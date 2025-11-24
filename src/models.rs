use near_sdk::{AccountId, Timestamp, borsh::{BorshDeserialize, BorshSerialize}, near, serde::{Deserialize, Serialize}};

#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize,PartialEq, Debug)]
pub enum DisputeStatus {
    None,
    Raised,
    Resolved,
}

pub struct Property{
    pub owner_id: AccountId,
    pub description: String,
    pub metadata_uri:String,
    pub is_for_sale: bool,
    pub price: u128,
    pub lease_duration_nanos:u64,//0 if for sale
    pub damage_escrow:u128,//amount held in escrow for damages
    pub active_lease:Option<Lease>,
}

pub struct Bid{
    pub bidder_id: AccountId,
    pub bid_amount: u128,
    pub created_at:Timestamp,
}

pub struct Lease{
    pub property_id:u64,
    pub tenant_id:AccountId,
    pub start_time:Timestamp,
    pub end_time:Timestamp,
    pub active:bool,
    pub dispute_status:DisputeStatus,
    pub escrow_held:u128,

}

