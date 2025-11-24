use crate::{ShedaContract, models::ContractError};
use near_sdk::{AccountId,env, log};
use crate::models::DisputeStatus;

impl ShedaContract {
    pub fn add_admin(&mut self, new_admin_id: AccountId) {
        //check caller is an admin
        assert!(!self.admins.contains(&env::signer_account_id()), "Admin already exists");
        self.admins.insert(new_admin_id.clone());
        log!("Admin {} added", new_admin_id);
    }

    pub fn remove_admin(&mut self, admin_id: AccountId) {
        //check caller is the owner
        assert_eq!(env::signer_account_id(), self.owner_id, "Only owner can remove admins");
        self.admins.remove(&admin_id);
        log!("Admin {} removed", admin_id);
    }
    
    pub fn is_admin(&self, account_id: AccountId) -> bool {
        self.admins.contains(&account_id)
    }

    pub fn resolve_dispute(&mut self, lease_id: u64) -> Result<(), ContractError> {
        let mut lease = self.leases.get(&lease_id).ok_or(ContractError::LeaseNotActive)?;

        if lease.dispute_status != DisputeStatus::Raised {
            return Err(ContractError::DisputeAlreadyRaised);
        }

        // Only an admin can resolve disputes
        assert!(self.is_admin(env::signer_account_id()), "UnauthorizedAccess");

        lease.dispute_status = crate::models::DisputeStatus::Resolved;
        self.leases.insert(&lease_id, &lease);
        log!("Dispute for lease {} resolved by admin {}", lease_id, env::signer_account_id());
        Ok(())
    }
}