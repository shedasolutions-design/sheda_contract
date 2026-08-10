//! Shared sandbox harness: a real NEP-141 token and a funded bid.
//!
//! Every escrow path needs a bid that actually holds money, and a bid can only
//! be created through `ft_transfer_call` from a fungible token. Without one,
//! the tests could reach the "unknown bid" guards and nothing else — which is
//! why the cancellation, agreement-burn and ownership-transfer tests were all
//! ignored. This puts a token on the sandbox so they can run.

#![allow(dead_code)] // each integration test binary uses a different subset

use near_workspaces::types::NearToken;
use near_workspaces::{Account, Contract, Worker};
use serde_json::json;

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Plenty for the sandbox; amounts are 6-decimal like the real stablecoins.
pub const TOTAL_SUPPLY: u128 = 1_000_000_000_000;
pub const BID_AMOUNT: u128 = 250_000_000;

/// One yoctoNEAR, the standard NEP-141/NEP-171 confirmation deposit.
pub fn one_yocto() -> NearToken {
    NearToken::from_yoctonear(1)
}

/// `mint_property` charges storage; the client attaches this too.
pub fn mint_deposit() -> NearToken {
    NearToken::from_millinear(100)
}

pub struct Fixture {
    pub contract: Contract,
    pub ft: Contract,
    pub seller: Account,
    pub buyer: Account,
}

/// Deploys the FT first so the marketplace can be initialised already trusting
/// it — `ft_on_transfer` rejects any token not in `accepted_stablecoin`, so a
/// token added afterwards would just be refused.
pub async fn setup(worker: &Worker<near_workspaces::network::Sandbox>) -> TestResult<Fixture> {
    let root = worker.root_account()?;

    let ft_owner = root
        .create_subaccount("ftowner")
        .initial_balance(NearToken::from_near(20))
        .transact()
        .await?
        .into_result()?;

    let ft_wasm = near_workspaces::compile_project("./tests/fixtures/ft").await?;
    let ft = worker.dev_deploy(&ft_wasm).await?;
    ft.call("new")
        .args_json(json!({
            "owner_id": ft_owner.id(),
            "total_supply": TOTAL_SUPPLY.to_string(),
        }))
        .transact()
        .await?
        .into_result()?;

    let contract_wasm = near_workspaces::compile_project("./").await?;
    let contract = worker.dev_deploy(&contract_wasm).await?;
    contract
        .call("new")
        .args_json(json!({
            "media_url": "https://example.com/sheda-icon.png",
            "supported_stablecoins": [ft.id()],
        }))
        .transact()
        .await?
        .into_result()?;

    let seller = root
        .create_subaccount("seller")
        .initial_balance(NearToken::from_near(20))
        .transact()
        .await?
        .into_result()?;
    let buyer = root
        .create_subaccount("buyer")
        .initial_balance(NearToken::from_near(20))
        .transact()
        .await?
        .into_result()?;

    // NEP-141 balances need a registered account before they can be credited.
    // The marketplace needs one too — it holds the escrow itself.
    for account in [seller.id(), buyer.id(), contract.id()] {
        ft.call("storage_deposit")
            .args_json(json!({ "account_id": account }))
            .deposit(NearToken::from_millinear(10))
            .transact()
            .await?
            .into_result()?;
    }

    ft_owner
        .call(ft.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": buyer.id(),
            "amount": (BID_AMOUNT * 4).to_string(),
        }))
        .deposit(one_yocto())
        .transact()
        .await?
        .into_result()?;

    Ok(Fixture {
        contract,
        ft,
        seller,
        buyer,
    })
}

impl Fixture {
    /// Mints a property owned by the seller and returns its id.
    pub async fn mint_property(&self, is_for_sale: bool) -> TestResult<u64> {
        let result = self
            .seller
            .call(self.contract.id(), "mint_property")
            .args_json(json!({
                "title": "Test Property",
                "description": "A property for the sandbox",
                "media_uri": "https://example.com/property.png",
                "price": BID_AMOUNT.to_string(),
                "is_for_sale": is_for_sale,
                "lease_duration_months": if is_for_sale { None } else { Some(12u64) },
            }))
            .deposit(mint_deposit())
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        Ok(result.json::<u64>()?)
    }

    /// Funds a bid the only way one can be funded — `ft_transfer_call` into
    /// `ft_on_transfer`, with the `BidAction` as the message.
    pub async fn place_bid(&self, property_id: u64, purchase: bool) -> TestResult<u64> {
        let before = self.bid_counter().await?;

        self.buyer
            .call(self.ft.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": self.contract.id(),
                "amount": BID_AMOUNT.to_string(),
                "msg": json!({
                    "property_id": property_id,
                    "action": if purchase { "Purchase" } else { "Lease" },
                    "stablecoin_token": self.ft.id(),
                }).to_string(),
            }))
            .deposit(one_yocto())
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // ft_on_transfer allocates ids from bid_counter, so the bid just
        // created is the one the counter pointed at beforehand.
        assert_eq!(
            self.bid_counter().await?,
            before + 1,
            "ft_transfer_call did not create a bid — it was most likely refunded"
        );
        Ok(before)
    }

    pub async fn bid_counter(&self) -> TestResult<u64> {
        Ok(self.contract.view("get_bid_counter").await?.json::<u64>()?)
    }

    pub async fn ft_balance(&self, account_id: &near_workspaces::AccountId) -> TestResult<u128> {
        let raw = self
            .ft
            .view("ft_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json::<String>()?;
        Ok(raw.parse()?)
    }

    /// The status of a single bid, as the views report it.
    pub async fn bid_status(&self, property_id: u64, bid_id: u64) -> TestResult<Option<String>> {
        let bids = self
            .contract
            .view("get_bids_for_property")
            .args_json(json!({ "property_id": property_id }))
            .await?
            .json::<Vec<serde_json::Value>>()?;
        Ok(bids
            .into_iter()
            .find(|b| b["id"].as_u64() == Some(bid_id))
            .map(|b| b["status"].to_string().trim_matches('"').to_string()))
    }

    pub async fn property_owner(&self, property_id: u64) -> TestResult<Option<String>> {
        let property = self
            .contract
            .view("get_property_by_id")
            .args_json(json!({ "property_id": property_id }))
            .await?
            .json::<Option<serde_json::Value>>()?;
        Ok(property.map(|p| p["owner_id"].as_str().unwrap_or_default().to_string()))
    }
}
