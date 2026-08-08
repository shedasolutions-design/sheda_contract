//! A minimal NEP-141 token, for tests only.
//!
//! Every interesting path in this contract moves money: a bid is funded by
//! `ft_transfer_call`, a cancellation refunds through `ft_transfer`, and a
//! completed sale pays the seller the same way. None of that can be reached
//! without a real fungible token on the sandbox, so the tests covering the
//! escrow paths were all `#[ignore]`d — which left the money-moving code as
//! the least tested part of the contract.
//!
//! This is the smallest thing that closes that gap. It is deliberately not a
//! production token: no minting controls, no burn, no metadata beyond what
//! `ft_metadata` must return. The supply is handed to one account at
//! construction and moved around from there.

use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::json_types::U128;
// NearToken is the one thing the impl_fungible_token_* macros need and do not
// bring in themselves: they import the standards traits they implement, but
// the near_sdk types appearing in those signatures — storage_withdraw takes an
// Option<NearToken> — have to already be in scope at the expansion site.
// Importing the traits as well collides with the macros' own imports.
use near_sdk::{near, AccountId, NearToken, PanicOnDefault};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct FtFixture {
    token: FungibleToken,
}

#[near]
impl FtFixture {
    #[init]
    pub fn new(owner_id: AccountId, total_supply: U128) -> Self {
        let mut this = Self {
            token: FungibleToken::new(b"a".to_vec()),
        };
        this.token.internal_register_account(&owner_id);
        this.token.internal_deposit(&owner_id, total_supply.into());
        this
    }
}

near_contract_standards::impl_fungible_token_core!(FtFixture, token);
near_contract_standards::impl_fungible_token_storage!(FtFixture, token);

#[near]
impl FungibleTokenMetadataProvider for FtFixture {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "Sheda Test Stablecoin".to_string(),
            symbol: "TUSD".to_string(),
            icon: None,
            reference: None,
            reference_hash: None,
            // Matches the stablecoins the app actually uses, so amounts in the
            // tests read the same way they do in the client.
            decimals: 6,
        }
    }
}
