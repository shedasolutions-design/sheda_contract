use crate::models::*;
use crate::{ShedaContract, ShedaContractExt};
use near_sdk::{env, near_bindgen, AccountId};
use schemars::JsonSchema;

/// View structs for JSON serialization - separate from internal models

// Default pagination limit for view methods
const DEFAULT_PAGINATION_LIMIT: u64 = 100;

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
    pub dispute: Option<DisputeInfoView>,
    pub escrow_held: String, // u128 as string for JSON
    pub escrow_token: String,
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
    pub timestamp: u64,
    pub sold: Option<SoldView>,
    pub property_instance: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct SoldView {
    pub property_id: u64,
    pub buyer_id: String,
    pub amount: String, // u128 as string for JSON
    pub previous_owner_id: String,
    pub sold_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct BidView {
    pub id: u64,
    pub bidder_id: String,
    pub property_id: u64,
    pub bid_amount: String, // u128 as string for JSON
    pub created_at: u64,
    pub updated_at: u64,
    pub status: BidStatus,
    pub document_token_id: Option<String>,
    pub escrow_release_tx: Option<String>,
    pub dispute_reason: Option<String>,
    pub expires_at: Option<u64>,
    pub escrow_release_after: Option<u64>,
    pub action: Action,
    pub stablecoin_token: String,
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct UserStatsView {
    pub account_id: String,
    pub total_bids: u64,
    pub total_properties: u64,
    pub total_leases: u64,
    pub active_leases: u64,
}

/// Conversion functions from internal models to view structs

impl From<&DisputeStatus> for DisputeStatusView {
    fn from(status: &DisputeStatus) -> Self {
        let status_str = match status {
            DisputeStatus::None => "none",
            DisputeStatus::Raised => "raised",
            DisputeStatus::Resolved => "resolved",
            DisputeStatus::PendingTenantResponse => "pending_tenant_response",
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
            dispute: lease.dispute.as_ref().map(|info| info.into()),
            escrow_held: lease.escrow_held.to_string(),
            escrow_token: lease.escrow_token.to_string(),
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
            lease_duration_nanos: property.lease_duration_months,
            damage_escrow: property.damage_escrow.to_string(),
            active_lease: property.active_lease.as_ref().map(|lease| lease.into()),
            timestamp: property.timestamp,
            sold: property.sold.as_ref().map(|sold| sold.into()),
            property_instance: None,
        }
    }
}

impl From<&Sold> for SoldView {
    fn from(sold: &Sold) -> Self {
        SoldView {
            property_id: sold.property_id,
            buyer_id: sold.buyer_id.to_string(),
            amount: sold.amount.to_string(),
            previous_owner_id: sold.previous_owner_id.to_string(),
            sold_at: sold.sold_at,
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
            updated_at: bid.updated_at,
            status: bid.status.clone(),
            document_token_id: bid.document_token_id.clone(),
            escrow_release_tx: bid.escrow_release_tx.clone(),
            dispute_reason: bid.dispute_reason.clone(),
            expires_at: bid.expires_at,
            escrow_release_after: bid.escrow_release_after,
            action: bid.action.clone(),
            stablecoin_token: bid.stablecoin_token.to_string(),
        }
    }
}
#[near_bindgen]
impl ShedaContract {
    fn property_to_view(&self, property: &Property) -> PropertyView {
        let mut view: PropertyView = property.into();
        view.property_instance = self
            .property_instances
            .get(&property.id)
            .map(|account| account.to_string());
        view
    }

    pub fn get_all_admins(&self) -> Vec<AccountId> {
        self.admins.iter().cloned().collect()
    }

    pub fn is_caller_admin(&self, account_id: AccountId) -> bool {
        self.admins.contains(&account_id)
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
        self.properties
            .get(&property_id)
            .map(|p| self.property_to_view(p))
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
            result.push(self.property_to_view(value));
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
                    properties.push(self.property_to_view(property));
                }
            }
        }
        properties
    }

    pub fn supported_stablecoins(&self) -> Vec<AccountId> {
        self.accepted_stablecoin.clone()
    }

    pub fn get_property_instance(&self, property_id: u64) -> Option<AccountId> {
        self.property_instances.get(&property_id).cloned()
    }

    pub fn get_stablecoin_balance(&self, token_account: AccountId) -> String {
        let balance = self.stable_coin_balances.get(&token_account).unwrap_or(&0);
        balance.to_string()
    }

    pub fn get_time_lock_config(&self) -> (u64, u64, u64) {
        (
            self.bid_expiry_ns,
            self.escrow_release_delay_ns,
            self.lost_bid_claim_delay_ns,
        )
    }

    //NOTE Poperty Owner specific
    pub fn get_my_properties(&mut self) -> Vec<PropertyView> {
        let caller = env::signer_account_id();
        self.get_property_by_owner(caller)
    }

    pub fn get_bids_on_my_property(&mut self) -> Vec<BidView> {
        let caller = env::signer_account_id();
        let properties = self.get_property_by_owner(caller);
        let mut bids = Vec::new();
        for property in properties {
            let property_bids = self.get_bids_for_property(property.id);
            bids.extend(property_bids);
        }
        bids
    }

    // Paginated view to get all bids
    pub fn get_all_bids(&self, from_index: u64, limit: u64) -> Vec<BidView> {
        self.bids
            .iter()
            .flat_map(|(_property_id, bids)| bids.iter())
            .skip(from_index as usize)
            .take(limit as usize)
            .map(|bid| bid.into())
            .collect()
    }

    // Paginated view to get bids for a specific property
    pub fn get_bids_for_property_paginated(&self, property_id: u64, from_index: u64, limit: u64) -> Vec<BidView> {
        self.bids
            .get(&property_id)
            .map(|bids| {
                bids.iter()
                    .skip(from_index as usize)
                    .take(limit as usize)
                    .map(|bid| bid.into())
                    .collect()
            })
            .unwrap_or_default()
    }

    // Paginated view to get bids by a specific bidder
    #[payable]
    pub fn get_bids_by_bidder(&mut self, bidder: AccountId, from_index: u64, limit: u64) -> Vec<BidView> {
        self.bids
            .iter()
            .flat_map(|(_property_id, bids)| bids.iter())
            .filter(|bid| bid.bidder == bidder)
            .skip(from_index as usize)
            .take(limit as usize)
            .map(|bid| bid.into())
            .collect()
    }

    // Get my bids (bids I've made)
    pub fn get_my_bids(&mut self) -> Vec<BidView> {
        let caller = env::signer_account_id();
        self.get_bids_by_bidder(caller, 0, DEFAULT_PAGINATION_LIMIT)
    }

    // Alternative view methods (non-payable, read-only) to reduce gas costs for off-chain views

    pub fn view_is_admin(&self, account_id: AccountId) -> bool {
        self.admins.contains(&account_id)
    }

    pub fn view_bids_on_properties_of_owner(&self, owner_id: AccountId) -> Vec<BidView> {
        let properties = self.get_property_by_owner(owner_id);
        let mut bids = Vec::new();
        for property in properties {
            let property_bids = self.get_bids_for_property(property.id);
            bids.extend(property_bids);
        }
        bids
    }

    pub fn view_bids_by_bidder(&self, bidder: AccountId, from_index: u64, limit: u64) -> Vec<BidView> {
        self.bids
            .iter()
            .flat_map(|(_property_id, bids)| bids.iter())
            .filter(|bid| bid.bidder == bidder)
            .skip(from_index as usize)
            .take(limit as usize)
            .map(|bid| bid.into())
            .collect()
    }

    pub fn get_leases_by_tenant(&self, tenant_id: AccountId) -> Vec<LeaseView> {
        let lease_ids = self.lease_per_tenant.get(&tenant_id);
        let mut leases = Vec::new();
        if let Some(ids) = lease_ids {
            for id in ids {
                if let Some(lease) = self.leases.get(&id) {
                    leases.push(lease.into());
                }
            }
        }
        leases
    }

    pub fn get_my_leases(&mut self) -> Vec<LeaseView> {
        let caller = env::signer_account_id();
        self.get_leases_by_tenant(caller)
    }

    pub fn get_active_leases_count(&self) -> u64 {
        self.leases.values().filter(|lease| lease.active).count() as u64
    }

    pub fn get_user_stats(&self, account_id: AccountId) -> UserStatsView {
        let total_bids = self
            .bids
            .iter()
            .flat_map(|(_property_id, bids)| bids.iter())
            .filter(|bid| bid.bidder == account_id)
            .count() as u64;

        let total_properties = self
            .property_per_owner
            .get(&account_id)
            .map(|ids| ids.len() as u64)
            .unwrap_or(0);

        let leases = self.get_leases_by_tenant(account_id.clone());
        let total_leases = leases.len() as u64;
        let active_leases = leases.iter().filter(|lease| lease.active).count() as u64;

        UserStatsView {
            account_id: account_id.to_string(),
            total_bids,
            total_properties,
            total_leases,
            active_leases,
        }
    }
}
