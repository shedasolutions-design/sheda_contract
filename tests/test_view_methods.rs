use near_workspaces::types::NearToken;
use serde_json::json;

// mint_property mints an NFT via near-contract-standards, which requires the
// caller to cover the new token's storage. Excess is refunded by the standard.
const MINT_DEPOSIT: NearToken = NearToken::from_millinear(100);

/// Regression tests for view methods being callable as *views*.
///
/// near-sdk's codegen appends an `env::state_write` to any `&mut self`
/// method. `storage_write` is a prohibited host function during a read-only
/// call, so a `&mut self` signature makes the method panic with
/// `ProhibitedInView` for every client trying to query it for free — even
/// though the body only reads.
///
/// Five methods in `src/views.rs` were declared `&mut self` despite doing no
/// mutation at all. `get_bids_by_bidder` was fixed in #3; the remaining four
/// are fixed alongside these tests.
///
/// Each case below goes through `.view()` (not `.call()`) on purpose — that
/// is precisely the path that regresses if someone reintroduces `&mut self`.
/// `.call()` would mask the bug, since a change-method call is allowed to
/// write state.
#[tokio::test]
async fn test_view_methods_are_view_callable() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let init_outcome = contract
        .call("new")
        .args_json(json!({
            "media_url": "https://example.com/sheda-icon.png",
            "supported_stablecoins": [],
        }))
        .transact()
        .await?;
    assert!(
        init_outcome.is_success(),
        "{:#?}",
        init_outcome.into_result().unwrap_err()
    );

    // Give the contract some real state so these aren't all trivially-empty
    // reads that could pass for the wrong reason.
    let mint_outcome = contract
        .call("mint_property")
        .args_json(json!({
            "title": "View Test Property",
            "description": "Used to exercise the view methods",
            "media_uri": "https://example.com/property.png",
            "price": "1000000",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .deposit(MINT_DEPOSIT)
        .transact()
        .await?;
    assert!(
        mint_outcome.is_success(),
        "{:#?}",
        mint_outcome.into_result().unwrap_err()
    );

    // ---- takes an explicit account argument, so it is genuinely usable as a
    // ---- free query by any client (this is the one the mobile app needs)
    let bids_by_bidder: serde_json::Value = contract
        .view("get_bids_by_bidder")
        .args_json(json!({
            "bidder": contract.id().to_string(),
            "from_index": 0,
            "limit": 50,
        }))
        .await?
        .json()?;
    assert!(
        bids_by_bidder.is_array(),
        "get_bids_by_bidder must return an array from a view call, got: {bids_by_bidder}"
    );

    // ---- signer-based methods. `signer_account_id()` isn't populated in a
    // ---- view context, so these return empty rather than useful data, but
    // ---- they must still not panic: a ProhibitedInView here is the exact
    // ---- regression being guarded against.
    let my_properties: serde_json::Value = contract
        .view("get_my_properties")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(
        my_properties.is_array(),
        "get_my_properties must return an array from a view call, got: {my_properties}"
    );

    let bids_on_my_property: serde_json::Value = contract
        .view("get_bids_on_my_property")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(
        bids_on_my_property.is_array(),
        "get_bids_on_my_property must return an array from a view call, got: {bids_on_my_property}"
    );

    let my_bids: serde_json::Value = contract
        .view("get_my_bids")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(
        my_bids.is_array(),
        "get_my_bids must return an array from a view call, got: {my_bids}"
    );

    let my_leases: serde_json::Value = contract
        .view("get_my_leases")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(
        my_leases.is_array(),
        "get_my_leases must return an array from a view call, got: {my_leases}"
    );

    Ok(())
}

/// The bid-reading paths the mobile app depends on must stay view-callable
/// and must agree with each other.
///
/// This also pins down the behaviour that broke production: `get_all_bids`
/// and `get_bids_for_property` are what every client uses to render
/// transaction history, so a panic in either (for any reason — bad signature
/// or undeserializable stored state) makes bids invisible to both buyer and
/// seller even though they exist on-chain.
#[tokio::test]
async fn test_bid_read_paths_agree() -> Result<(), Box<dyn std::error::Error>> {
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

    contract
        .call("mint_property")
        .args_json(json!({
            "title": "Bid Read Path Property",
            "description": "Exercises the bid read paths",
            "media_uri": "https://example.com/property.png",
            "price": "1000000",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .deposit(MINT_DEPOSIT)
        .transact()
        .await?
        .into_result()?;

    // No bids placed yet, so every read path should agree on "empty" —
    // and, critically, none of them should panic.
    let all_bids: Vec<serde_json::Value> = contract
        .view("get_all_bids")
        .args_json(json!({ "from_index": 0, "limit": 50 }))
        .await?
        .json()?;

    let property_bids: Vec<serde_json::Value> = contract
        .view("get_bids_for_property")
        .args_json(json!({ "property_id": 0 }))
        .await?
        .json()?;

    let bidder_bids: Vec<serde_json::Value> = contract
        .view("get_bids_by_bidder")
        .args_json(json!({
            "bidder": contract.id().to_string(),
            "from_index": 0,
            "limit": 50,
        }))
        .await?
        .json()?;

    assert_eq!(all_bids.len(), 0, "expected no bids on a fresh contract");
    assert_eq!(property_bids.len(), 0);
    assert_eq!(bidder_bids.len(), 0);

    // The counter is the independent source of truth for "how many bids
    // should be readable". If this ever disagrees with get_all_bids().len()
    // on a contract that has bids, stored state can't be deserialized —
    // which is exactly the production failure mode this guards.
    let bid_counter: u64 = contract
        .view("get_bid_counter")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(
        bid_counter as usize,
        all_bids.len(),
        "bid_counter and get_all_bids disagree — stored bids are unreadable"
    );

    Ok(())
}
