// Find all our documentation at https://docs.near.org
use near_sdk::json_types::U128;
use near_sdk::{ext_contract, AccountId};

use crate::models::DisputeWinner;
use crate::TokenId;

// FT interface for cross-contract calls for near sdk
#[allow(dead_code)]
#[ext_contract(ft_contract)]
trait FT {
    fn ft_transfer(&self, receiver_id: AccountId, amount: U128);
}

// NFT interface for cross-contract calls for near sdk
#[allow(dead_code)]
#[ext_contract(nft_contract)]
trait NFT {
    fn nft_transfer(&self, receiver_id: AccountId, token_id: TokenId);
}

// Property instance interface for global contract factory
#[allow(dead_code)]
#[ext_contract(property_instance)]
trait PropertyInstance {
    fn new(owner_id: AccountId, property_id: u64, escrow_token: AccountId);
}

// Oracle interface for dispute resolution
#[allow(dead_code)]
#[ext_contract(dispute_oracle)]
trait DisputeOracle {
    fn resolve_dispute(&self, lease_id: u64, property_id: u64) -> DisputeWinner;
}
