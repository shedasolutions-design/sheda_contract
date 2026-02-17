pub use crate::ext::*;
use crate::internal::extract_base_uri;
#[allow(unused_imports)]
use crate::{HasNew, models::*};
use crate::views::LeaseView;
use crate::events::{
    emit_event, AdminAddedEvent, AdminRemovedEvent, DisputeResolvedEvent,
    EmergencyWithdrawalEvent,
};
use crate::{models::ContractError, ShedaContract, ShedaContractExt};
use near_contract_standards::non_fungible_token::metadata::NFTContractMetadata;
use near_sdk::json_types::U128;
use near_sdk::{env, log, near_bindgen, AccountId, Gas, NearToken, PromiseResult};

fn checked_add_u128(left: u128, right: u128, label: &str) -> u128 {
    left.checked_add(right)
        .unwrap_or_else(|| env::panic_str(&format!("Overflow in {}", label)))
}

fn checked_sub_u128(left: u128, right: u128, label: &str) -> u128 {
    left.checked_sub(right)
        .unwrap_or_else(|| env::panic_str(&format!("Underflow in {}", label)))
}

#[near_bindgen]
impl ShedaContract {

    #[payable]
    pub fn add_admin(&mut self, new_admin_id: AccountId) {
        //check caller is an admin
        assert!(
            self.admins.contains(&env::signer_account_id()),
            "account is not an admin"
        );
        self.admins.insert(new_admin_id.clone());
        log!("Admin {} added", new_admin_id);
        emit_event(
            "AdminAdded",
            AdminAddedEvent {
                admin_id: new_admin_id,
                added_by: env::signer_account_id(),
            },
        );
    }

    #[payable]
    pub fn remove_admin(&mut self, admin_id: AccountId) {
        //check caller is the owner
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can remove admins"
        );
        self.admins.remove(&admin_id);
        log!("Admin {} removed", admin_id);
        emit_event(
            "AdminRemoved",
            AdminRemovedEvent {
                admin_id: admin_id.clone(),
                removed_by: env::signer_account_id(),
            },
        );
    }

    pub fn is_admin(&self, account_id: AccountId) -> bool {
        self.admins.contains(&account_id)
    }

    #[handle_result]
    #[payable]
    pub fn resolve_dispute(
        &mut self,
        lease_id: u64,
        winner: DisputeWinner,
        payout_amount: U128,
    ) -> Result<(), ContractError> {
        let mut lease = self
            .leases
            .get(&lease_id)
            .cloned()
            .ok_or(ContractError::LeaseNotFound)?;

        if lease.dispute_status != DisputeStatus::Raised {
            return Err(ContractError::DisputeAlreadyRaised);
        };

        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );

        let recipient = match winner {
            DisputeWinner::Tenant => lease.tenant_id.clone(),
            DisputeWinner::Owner => self
                .properties
                .get(&lease.property_id)
                .expect("Property not found")
                .owner_id
                .clone(),
        };

        let payout = payout_amount.0.min(lease.escrow_held);
        let escrow_token = lease.escrow_token.clone();

        lease.dispute_status = DisputeStatus::Resolved;
        if let Some(info) = lease.dispute.as_mut() {
            info.oracle_result = Some(winner.clone());
            info.resolved_by = Some(env::signer_account_id());
            info.resolved_at = Some(env::block_timestamp());
        }
        self.leases.insert(lease_id, lease);
        log!(
            "Dispute for lease {} resolved by admin {}",
            lease_id,
            env::signer_account_id()
        );

        let current_balance = *self
            .stable_coin_balances
            .get(&escrow_token)
            .unwrap_or(&0);
        self.stable_coin_balances.insert(
            escrow_token.clone(),
            checked_sub_u128(current_balance, payout, "resolve_dispute payout"),
        );

        #[allow(unused_must_use)]
        ft_contract::ext(escrow_token)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(Gas::from_tgas(30))
            .ft_transfer(recipient.clone(), U128(payout));

        emit_event(
            "DisputeResolved",
            DisputeResolvedEvent {
                token_id: lease_id,
                admin_id: env::signer_account_id(),
                winner_id: recipient,
                escrow_returned: payout,
            },
        );

        Ok(())
    }


    #[payable]
    pub fn get_leases_with_disputes(&mut self) -> Vec<LeaseView> {
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

    #[payable]
    pub fn add_supported_stablecoin(&mut self, token_account: AccountId) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can add supported stablecoins"
        );
        if !self.accepted_stablecoin.contains(&token_account) {
            self.accepted_stablecoin.push(token_account.clone());
            self.stable_coin_balances
                .insert(token_account.clone(), 0);
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
            if balance > 0 {
                // Optimistically set balance to 0
                self.stable_coin_balances.insert(token.clone(), 0);

                //cross contract call to transfer stablecoin to owner
                #[allow(unused_must_use)]
                ft_contract::ext(token.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(Gas::from_tgas(30))
                    .ft_transfer(to_account.clone(), U128(balance))
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(Gas::from_tgas(10))
                            .withdraw_callback(token.clone(), U128(balance))
                    );
                
                log!(
                    "Emergency withdrawal of {} {} to {} by owner {}",
                    balance,
                    token,
                    to_account,
                    env::signer_account_id()
                );
                emit_event(
                    "EmergencyWithdrawal",
                    EmergencyWithdrawalEvent {
                        amount: balance,
                        recipient: to_account.clone(),
                        initiated_by: env::signer_account_id(),
                    },
                );
            }
        }
    }

    #[private]
    pub fn withdraw_callback(&mut self, token: AccountId, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                log!("Withdrawal of {} {} successful", amount.0, token);
            }
            PromiseResult::Failed => {
                log!("Withdrawal of {} {} failed, reverting balance", amount.0, token);
                let current_balance = *self.stable_coin_balances.get(&token).unwrap_or(&0);
                self.stable_coin_balances.insert(
                    token,
                    checked_add_u128(current_balance, amount.0, "withdraw revert"),
                );
            }
        }
    }

    pub fn remove_supported_stablecoin(&mut self, token_account: AccountId) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can remove supported stablecoins"
        );
        let balance = *self.stable_coin_balances.get(&token_account).unwrap_or(&0);
        assert_eq!(balance, 0, "Stablecoin balance must be zero to remove");
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

    pub fn withdraw_stablecoin(&mut self, token_account: AccountId, amount: u128) {
        assert_eq!(
            env::signer_account_id(),
            self.owner_id,
            "Only owner can withdraw stablecoins"
        );
        let balance = *self.stable_coin_balances.get(&token_account).unwrap_or(&0);
        assert!(balance >= amount, "Insufficient balance for withdrawal");
        
        // Optimistically update balance
        self.stable_coin_balances.insert(
            token_account.clone(),
            checked_sub_u128(balance, amount, "withdraw"),
        );

        //cross contract call to transfer stablecoin to owner
        #[allow(unused_must_use)]
        ft_contract::ext(token_account.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(Gas::from_tgas(30))
            .ft_transfer(env::signer_account_id(), U128(amount))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .withdraw_callback(token_account.clone(), U128(amount))
            );

        log!(
            "Withdrawal of {} {} by owner {}",
            amount,
            token_account,
            env::signer_account_id()
        );
    }

    #[payable]
    pub fn refund_bids(&mut self, property_id: u64) {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        if let Some(bids) = self.bids.get_mut(&property_id) {
            for bid in bids.iter_mut() {
                if bid.status != BidStatus::Pending {
                    continue;
                }

                let bidder = bid.bidder.clone();
                let amount = bid.amount;
                let stablecoin_token = bid.stablecoin_token.clone();

                //update stablecoin balance optimistically
                let current_balance = *self
                    .stable_coin_balances
                    .get(&stablecoin_token)
                    .unwrap_or(&0);
                self.stable_coin_balances.insert(
                    stablecoin_token.clone(),
                    checked_sub_u128(current_balance, amount, "refund_bids"),
                );

                //cross contract call to transfer stablecoin back to bidder
                #[allow(unused_must_use)]
                ft_contract::ext(stablecoin_token.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(Gas::from_tgas(30))
                    .ft_transfer(bidder.clone(), U128(amount))
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(Gas::from_tgas(10))
                            .withdraw_callback(stablecoin_token.clone(), U128(amount))
                    );

                bid.status = BidStatus::Cancelled;
                bid.updated_at = env::block_timestamp();

                log!(
                    "Refunded {} to bidder {} for property {} by admin {}",
                    amount,
                    bidder,
                    property_id,
                    env::signer_account_id()
                );
            }
        }
    }

    pub fn admin_delist_property(&mut self, property_id: u64) {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        //Check that property is not sold or leased
        let mut property = self
            .properties
            .get(&property_id)
            .expect("Property not found")
            .clone();
        assert!(!property.is_for_sale, "Property is currently for sale");
        assert!(
            property.active_lease.is_none(),
            "Property is currently leased"
        );

        assert!(
            self.bids.get(&property_id).is_none(),
            "Cannot delist property with active bids"
        );

        assert!(
            !property.sold.is_none(),
            "Cannot delist property that has been sold"
        );
        property.is_for_sale = false;
        self.properties.insert(property_id, property);
        log!(
            "Property {} delisted by admin {}",
            property_id,
            env::signer_account_id()
        );
    }

    #[payable]
    pub fn admin_delete_property(&mut self, property_id: u64) {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        let property = self
            .properties
            .get(&property_id)
            .expect("Property not found")
            .clone();

        assert!(
            property.active_lease.is_none(),
            "Cannot delete a property with an active lease"
        );

        assert!(property.sold.is_none(), "Cannot delete a sold property");

        assert!(
            self.bids.get(&property_id.clone()).is_none(),
            "Cannot delete property with active bids"
        );

        self.properties.remove(&property_id.clone());
        log!(
            "Property {} deleted by admin {}",
            property_id,
            env::signer_account_id()
        );

        //burn the NFT
        crate::internal::burn_nft(self, property_id.to_string());
    }

    #[payable]
    pub fn admin_change_nft_metadata(&mut self, image_url: String, name: String, symbol:String) {
        assert!(
            self.is_admin(env::signer_account_id()),
            "UnauthorizedAccess"
        );
        let new_metadata = NFTContractMetadata {
            spec: "nft-1.0.0".to_string(),
            name: name,
            symbol: symbol,
            icon: Some(image_url.clone()),
            base_uri: Some(extract_base_uri(&image_url)),
            reference: None,
            reference_hash: None,
        };
        self.metadata.set(&new_metadata);
        log!(
            "NFT metadata changed by admin {}",
            env::signer_account_id()
        );
    }
    
}
