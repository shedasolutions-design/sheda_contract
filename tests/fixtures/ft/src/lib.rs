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
use near_contract_standards::storage_management::{StorageBalance, StorageManagement};
use near_sdk::json_types::U128;
// PromiseOrValue appears in the signatures impl_fungible_token_core! expands
// to. That macro imports the traits it implements but not the near_sdk types
// in their signatures, so this has to be in scope at the expansion site —
// while importing the traits themselves would collide with its own imports.
use near_sdk::{near, AccountId, PanicOnDefault, PromiseOrValue};

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

    /// Registers an account so it can hold a balance.
    ///
    /// Written out by hand rather than via `impl_fungible_token_storage!`,
    /// which is broken in near-contract-standards 5.24.1: it expands
    /// `storage_withdraw` as `Option<U128>` while the `StorageManagement`
    /// trait it implements declares `Option<NearToken>`, so the expansion
    /// can't compile.
    ///
    /// Only the two calls the tests actually need are exposed. This is a
    /// fixture, not a token anyone holds value in, so the rest of NEP-145
    /// (withdraw, unregister, bounds) would be untested surface area.
    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

near_contract_standards::impl_fungible_token_core!(FtFixture, token);

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
