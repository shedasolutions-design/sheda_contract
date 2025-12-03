use crate::models::*;
use crate::{ShedaContract, ShedaContractExt};
use near_sdk::{near_bindgen, AccountId};
use schemars::JsonSchema;

/// View structs for JSON serialization - separate from internal models

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct DisputeStatusView {
    pub status: String,
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct LeaseView {
    pub id: u64,
    pub property_id: u64,
    pub tenant_id: String,
    pub start_time: u64,
    pub end_time: u64,
    pub active: bool,
    pub dispute_status: DisputeStatusView,
    pub escrow_held: String, // u128 as string for JSON
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct PropertyView {
    pub id: u64,
    pub owner_id: String,
    pub description: String,
    pub metadata_uri: String,
    pub is_for_sale: bool,
    pub price: String, // u128 as string for JSON
    pub lease_duration_nanos: Option<u64>,
    pub damage_escrow: String, // u128 as string for JSON
    pub active_lease: Option<LeaseView>,
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct BidView {
    pub id: u64,
    pub bidder_id: String,
    pub property_id: u64,
    pub bid_amount: String, // u128 as string for JSON
    pub created_at: u64,
}

/// Conversion functions from internal models to view structs

impl From<&DisputeStatus> for DisputeStatusView {
    fn from(status: &DisputeStatus) -> Self {
        let status_str = match status {
            DisputeStatus::None => "none",
            DisputeStatus::Raised => "raised",
            DisputeStatus::Resolved => "resolved",
        };
        DisputeStatusView {
            status: status_str.to_string(),
        }
    }
}

impl From<&Lease> for LeaseView {
    fn from(lease: &Lease) -> Self {
        LeaseView {
            id: lease.id,
            property_id: lease.property_id,
            tenant_id: lease.tenant_id.to_string(),
            start_time: lease.start_time,
            end_time: lease.end_time,
            active: lease.active,
            dispute_status: (&lease.dispute_status).into(),
            escrow_held: lease.escrow_held.to_string(),
        }
    }
}

impl From<&Property> for PropertyView {
    fn from(property: &Property) -> Self {
        PropertyView {
            id: property.id,
            owner_id: property.owner_id.to_string(),
            description: property.description.clone(),
            metadata_uri: property.metadata_uri.clone(),
            is_for_sale: property.is_for_sale,
            price: property.price.to_string(),
            lease_duration_nanos: property.lease_duration_nanos,
            damage_escrow: property.damage_escrow.to_string(),
            active_lease: property.active_lease.as_ref().map(|lease| lease.into()),
        }
    }
}

impl From<&Bid> for BidView {
    fn from(bid: &Bid) -> Self {
        BidView {
            id: bid.id,
            bidder_id: bid.bidder.to_string(),
            property_id: bid.property_id,
            bid_amount: bid.amount.to_string(),
            created_at: bid.created_at,
        }
    }
}
#[near_bindgen]
impl ShedaContract {
    pub fn get_all_admins(&self) -> Vec<AccountId> {
        self.admins.iter().cloned().collect()
    }

    pub fn is_caller_admin(&self) -> bool {
        let caller: AccountId = near_sdk::env::signer_account_id();
        self.admins.contains(&caller)
    }

    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_property_counter(&self) -> u64 {
        self.property_counter
    }

    pub fn get_bid_counter(&self) -> u64 {
        self.bid_counter
    }

    pub fn get_lease_counter(&self) -> u64 {
        self.lease_counter
    }

    pub fn get_property_by_id(&self, property_id: u64) -> Option<PropertyView> {
        self.properties.get(&property_id).map(|p| p.into())
    }

    pub fn get_lease_by_id(&self, lease_id: u64) -> Option<LeaseView> {
        self.leases.get(&lease_id).map(|l| l.into())
    }

    pub fn get_bids_for_property(&self, property_id: u64) -> Vec<BidView> {
        self.bids
            .get(&property_id)
            .map(|bids| bids.iter().map(|bid| bid.into()).collect())
            .unwrap_or_default()
    }

    //paginate list of properties
    pub fn get_properties(&self, from_index: u64, limit: u64) -> Vec<PropertyView> {
        let mut result = Vec::new();
        let mut count = 0;

        for (_key, value) in self.properties.iter().skip(from_index as usize) {
            if count >= limit {
                break;
            }
            result.push(value.into());
            count += 1;
        }
        result
    }
    pub fn get_property_by_owner(&self, owner_id: AccountId) -> Vec<PropertyView> {
        let property_ids = self.property_per_owner.get(&owner_id);
        let mut properties = Vec::new();
        if let Some(ids) = property_ids {
            for id in ids {
                if let Some(property) = self.properties.get(&id) {
                    properties.push(property.into());
                }
            }
        }
        properties
    }
}
