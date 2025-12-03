pub use crate::ext::*;
use crate::models::*;
use crate::views::LeaseView;
use crate::{models::ContractError, ShedaContract, ShedaContractExt};
use near_sdk::json_types::U128;
use near_sdk::{AccountId, Gas, NearToken, env, log, near_bindgen};

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

    pub fn get_admins(&self) -> Vec<AccountId> {
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

    pub fn get_leases_with_disputes(&self) -> Vec<LeaseView> {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        log!("Admin {}", env::signer_account_id());
        self.leases
            .values()
            .filter(|lease| lease.dispute_status == DisputeStatus::Raised)
            .map(|lease| lease.into())
            .collect()
    }

    pub fn add_supported_stablecoin(&mut self, token_account: AccountId) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can add supported stablecoins"
        );
        if !self.accepted_stablecoin.contains(&token_account) {
            self.accepted_stablecoin.push(token_account.clone());
            log!(
                "Stablecoin {} added by owner {}",
                token_account,
                env::signer_account_id()
            );
        }
    }

    //withdraw supported stablecoin from contract
    #[payable]
    pub fn emergency_withdraw(&mut self, to_account: AccountId) {
        //get balances from contract struct
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can perform emergency withdrawal"
        );
        let supported_stables = self.accepted_stablecoin.clone();
        for token in supported_stables.iter() {
            let balance = *self.stable_coin_balances.get(token).unwrap_or(&0);
            assert!(balance > 0, "No balance for token {}", token);
            //cross contract call to transfer stablecoin to owner
            #[allow(unused_must_use)]
            ft_contract::ext(token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(Gas::from_tgas(30))
                .ft_transfer(to_account.clone(), U128(balance));
            //set balance to 0 after withdrawal
            self.stable_coin_balances.insert(token.clone(), 0);
            log!(
                "Emergency withdrawal of {} {} to {} by owner {}",
                balance,
                token,
                to_account,
                env::signer_account_id()
            );
        }
    }
    
    
    
    pub fn remove_supported_stablecoin(&mut self, token_account: AccountId) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can remove supported stablecoins"
        );
        if let Some(index) = self
            .accepted_stablecoin
            .iter()
            .position(|x| x == &token_account)
        {
            self.accepted_stablecoin.remove(index);
            log!(
                "Stablecoin {} removed by owner {}",
                token_account,
                env::signer_account_id()
            );
        }
    }
}
