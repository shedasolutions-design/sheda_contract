use crate::models::*;
use crate::{models::ContractError, ShedaContract,ShedaContractExt};
use near_sdk::{AccountId, Promise, env, log,near_bindgen};


#[near_bindgen]
impl ShedaContract {
    pub fn add_admin(&mut self, new_admin_id: AccountId) {
        //check caller is an admin
        assert!(
            !self.admins.contains(&env::signer_account_id()),
            "Admin already exists"
        );
        self.admins.insert(new_admin_id.clone());
        log!("Admin {} added", new_admin_id);
    }

    pub fn remove_admin(&mut self, admin_id: AccountId) {
        //check caller is the owner
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can remove admins"
        );
        self.admins.remove(&admin_id);
        log!("Admin {} removed", admin_id);
    }

    pub fn is_admin(&self, account_id: AccountId) -> bool {
        self.admins.contains(&account_id)
    }

    pub fn get_admins(&self)-> Vec<AccountId> {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        log!("Admin {}", env::signer_account_id());
        self.admins.iter().cloned().collect()
    }

    #[handle_result]
    pub fn resolve_dispute(&mut self, lease_id: u64) -> Result<(), ContractError> {
        let mut lease: Lease = self
            .leases
            .remove(&lease_id)
            .ok_or(ContractError::LeaseNotFound)?;

        if lease.dispute_status != DisputeStatus::Raised {
            self.leases.insert(lease_id, lease);
            return Err(ContractError::DisputeAlreadyRaised);
        };

        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );

        lease.dispute_status = DisputeStatus::Resolved;
        self.leases.insert(lease_id, lease);
        log!(
            "Dispute for lease {} resolved by admin {}",
            lease_id,
            env::signer_account_id()
        );

        Ok(())
    }

    pub fn get_leases_with_disputes(&self) -> Vec<Lease> {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        log!("Admin {}", env::signer_account_id());
        self.leases
            .values()
            .filter(|lease| lease.dispute_status == DisputeStatus::Raised)
            .cloned()
            .collect()
    }

    pub fn emergency_withdraw(&mut self, to_account: AccountId) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can perform emergency withdrawal"
        );

        let contract_balance = env::account_balance();
        
        Promise::new(to_account.clone()).transfer(contract_balance);
        log!(
            "Emergency withdrawal of {} yoctoNEAR to {} by owner {}",
            contract_balance.as_yoctonear(),
            to_account,
            env::signer_account_id()
        );
    }
}
