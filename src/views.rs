use near_sdk::{near_bindgen, AccountId};
use crate::{ShedaContract, ShedaContractExt};

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

    pub fn get_property_by_id(&self, property_id: u64) -> Option<crate::models::Property> {
        self.properties.get(&property_id).cloned()
    }

    pub fn get_lease_by_id(&self, lease_id: u64) -> Option<crate::models::Lease> {
        self.leases.get(&lease_id).cloned()
    }

    pub fn get_bids_for_property(&self, property_id: u64) -> Vec<crate::models::Bid> {
        self.bids.get(&property_id).cloned().unwrap_or_default()
    }

    //paginate list of properties
    pub fn get_properties(&self, from_index: u64, limit: u64) -> Vec<crate::models::Property> {
        let mut result = Vec::new();
        let mut count = 0;
        
        for (key, value) in self.properties.iter().skip(from_index as usize) {
            if count >= limit {
                break;
            }
            result.push(value.clone());
            count += 1;
        }
        result
    }





    

}