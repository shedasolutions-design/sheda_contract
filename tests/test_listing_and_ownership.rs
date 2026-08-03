use near_workspaces::types::NearToken;
use near_workspaces::{Account, Contract};
use serde_json::json;

// mint_property mints an NFT through near-contract-standards, which charges
// the caller for the new token's storage. Excess is refunded.
const MINT_DEPOSIT: NearToken = NearToken::from_millinear(100);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

async fn deploy_initialized() -> Result<
    (
        near_workspaces::Worker<near_workspaces::network::Sandbox>,
        Contract,
    ),
    Box<dyn std::error::Error>,
> {
    let contract_wasm = near_workspaces::compile_project("./").await?;
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    contract
        .call("new")
        .args_json(json!({
            "media_url": "https://example.com/sheda-icon.png",
            "supported_stablecoins": [],
        }))
        .transact()
        .await?
        .into_result()?;

    Ok((sandbox, contract))
}

async fn mint(
    contract: &Contract,
    title: &str,
    price: &str,
    for_sale: bool,
) -> Result<u64, Box<dyn std::error::Error>> {
    let outcome = contract
        .call("mint_property")
        .args_json(json!({
            "title": title,
            "description": title,
            "media_uri": "https://example.com/property.png",
            "price": price,
            "is_for_sale": for_sale,
            "lease_duration_months": null,
        }))
        .deposit(MINT_DEPOSIT)
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "mint_property failed: {:#?}",
        outcome.clone().into_result().unwrap_err()
    );
    Ok(outcome.json::<u64>()?)
}

/// The core of the relist feature: an owner can put an already-minted
/// property back on the market without minting a second NFT.
///
/// Before `update_listing` existed, `is_for_sale` could only ever be set to
/// `true` inside `mint_property` — so the only way to re-list anything was to
/// mint a duplicate token for a property that already existed on-chain. The
/// `get_property_counter` assertion below is the one that actually pins that
/// down.
#[tokio::test]
async fn test_owner_can_relist_without_minting() -> Result<(), Box<dyn std::error::Error>> {
    let (_sandbox, contract) = deploy_initialized().await?;

    let property_id = mint(&contract, "Relistable Property", "1000000", false).await?;

    let counter_before: u64 = contract
        .view("get_property_counter")
        .args_json(json!({}))
        .await?
        .json()?;

    let before: serde_json::Value = contract
        .view("get_property_by_id")
        .args_json(json!({ "property_id": property_id }))
        .await?
        .json()?;
    assert_eq!(before["is_for_sale"], false, "sanity: minted unlisted");

    let outcome = contract
        .call("update_listing")
        .args_json(json!({
            "property_id": property_id,
            "price": "2500000",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "update_listing failed: {:#?}",
        outcome.clone().into_result().unwrap_err()
    );

    let after: serde_json::Value = contract
        .view("get_property_by_id")
        .args_json(json!({ "property_id": property_id }))
        .await?
        .json()?;
    assert_eq!(after["is_for_sale"], true, "property should now be listed");
    assert_eq!(after["price"], "2500000", "price should be updated");

    // The whole point: no second token was created.
    let counter_after: u64 = contract
        .view("get_property_counter")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(
        counter_before, counter_after,
        "relisting must reuse the existing NFT, not mint a new one"
    );

    Ok(())
}

/// update_listing doubles as the price/terms editor, and must also be able to
/// take a listing back down.
#[tokio::test]
async fn test_update_listing_can_unlist_and_edit_terms() -> Result<(), Box<dyn std::error::Error>> {
    let (_sandbox, contract) = deploy_initialized().await?;
    let property_id = mint(&contract, "Editable Property", "1000000", true).await?;

    contract
        .call("update_listing")
        .args_json(json!({
            "property_id": property_id,
            "price": "500000",
            "is_for_sale": false,
            "lease_duration_months": 12,
        }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?
        .into_result()?;

    let after: serde_json::Value = contract
        .view("get_property_by_id")
        .args_json(json!({ "property_id": property_id }))
        .await?
        .json()?;
    assert_eq!(after["is_for_sale"], false);
    assert_eq!(after["price"], "500000");
    // PropertyView exposes lease_duration_months as lease_duration_nanos.
    assert_eq!(after["lease_duration_nanos"], 12);

    Ok(())
}

/// Only the current owner may change a listing. This matters more after the
/// ownership-transfer fix: once a property is sold, the previous owner must
/// lose the ability to re-list or edit something they no longer hold.
#[tokio::test]
async fn test_non_owner_cannot_update_listing() -> Result<(), Box<dyn std::error::Error>> {
    let (sandbox, contract) = deploy_initialized().await?;
    let property_id = mint(&contract, "Guarded Property", "1000000", true).await?;

    let stranger: Account = sandbox.dev_create_account().await?;

    let outcome = stranger
        .call(contract.id(), "update_listing")
        .args_json(json!({
            "property_id": property_id,
            "price": "1",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "a non-owner must not be able to update the listing"
    );

    // And nothing changed.
    let after: serde_json::Value = contract
        .view("get_property_by_id")
        .args_json(json!({ "property_id": property_id }))
        .await?
        .json()?;
    assert_eq!(after["price"], "1000000", "price must be untouched");

    Ok(())
}

/// A listed property must carry a real price — otherwise it could be put on
/// the market for zero.
#[tokio::test]
async fn test_cannot_list_at_zero_price() -> Result<(), Box<dyn std::error::Error>> {
    let (_sandbox, contract) = deploy_initialized().await?;
    let property_id = mint(&contract, "Zero Price Property", "1000000", false).await?;

    let outcome = contract
        .call("update_listing")
        .args_json(json!({
            "property_id": property_id,
            "price": "0",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "listing at a zero price must be rejected"
    );

    Ok(())
}

/// `property_per_owner` is what `get_property_by_owner` reads, and therefore
/// what the app's wallet uses to decide which properties a user owns. This
/// pins the baseline so the ownership-transfer assertions below are
/// meaningful.
#[tokio::test]
async fn test_property_per_owner_index_tracks_minter() -> Result<(), Box<dyn std::error::Error>> {
    let (_sandbox, contract) = deploy_initialized().await?;
    let property_id = mint(&contract, "Indexed Property", "1000000", true).await?;

    let owned: Vec<serde_json::Value> = contract
        .view("get_property_by_owner")
        .args_json(json!({ "owner_id": contract.id().to_string() }))
        .await?
        .json()?;

    assert_eq!(owned.len(), 1, "minter should own exactly one property");
    assert_eq!(owned[0]["id"], property_id);
    assert_eq!(owned[0]["owner_id"], contract.id().to_string());

    Ok(())
}

// ---------------------------------------------------------------------------
// Ownership transfer on a completed purchase
// ---------------------------------------------------------------------------
//
// The fix in this PR makes a completed purchase move `Property.owner_id` and
// the `property_per_owner` index alongside the NEP-171 token, instead of
// leaving them pointing at the seller forever.
//
// Exercising it end-to-end needs a funded bid, which means deploying a real
// NEP-141 token, registering storage for buyer/seller/contract, and driving
// ft_transfer_call -> accept -> document release -> release_escrow. This repo
// has no FT fixture to deploy, so that setup has to be added before this can
// run — writing a version that silently skipped the funding step would assert
// nothing useful.
//
// Left ignored with the assertions spelled out, so it can be finished by
// dropping in an FT wasm rather than re-deriving what to check:
//
//   after release_escrow on a Purchase bid,
//     get_property_by_id(id).owner_id            == buyer
//     nft_token(id).owner_id                     == buyer      (must agree)
//     get_property_by_owner(buyer)               contains id
//     get_property_by_owner(seller)              does NOT contain id
//     get_property_by_id(id).sold                == null       (clean slate)
//     get_property_by_id(id).is_for_sale         == false
//   and then, proving the buyer really controls it:
//     buyer calling update_listing(id, ...)      succeeds
//     seller calling update_listing(id, ...)     fails
//
// Verified manually against testnet in the meantime — see the PR description.
#[tokio::test]
#[ignore = "needs a NEP-141 token fixture to fund a bid; assertions documented above"]
async fn test_ownership_transfers_to_buyer_on_purchase() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
