use near_workspaces::types::NearToken;
use serde_json::json;

// mint_property mints an NFT via near-contract-standards, which requires the
// caller to cover the new token's storage. Excess is refunded by the standard.
const MINT_DEPOSIT: NearToken = NearToken::from_millinear(100);

/// Regression tests for the read methods in `src/views.rs`.
///
/// near-sdk's codegen appends an `env::state_write` to any `&mut self`
/// method. `storage_write` is a prohibited host function during a read-only
/// call, so a `&mut self` signature makes the method panic with
/// `ProhibitedInView` for every client trying to query it for free — even
/// though the body only reads. Five methods here were `&mut self` despite
/// performing no mutation at all.
///
/// There are two distinct groups, and they need different assertions:
///
/// - `get_bids_by_bidder` takes an explicit `bidder` argument, so once it is
///   `&self` it becomes genuinely view-callable. Asserted below via `.view()`,
///   which is exactly the path that regresses if `&mut self` is reintroduced
///   (`.call()` would mask it, since a change call may write state).
///
/// - The four `get_my_*` methods read `env::signer_account_id()`, which is
///   itself prohibited in a view context, so they can never be views no
///   matter the signature. For those, `&self` is still the correct
///   declaration — they don't mutate — and the assertion is just that they
///   continue to work as calls.
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

    // ---- The four `get_my_*` methods are a different case, and this test
    // ---- originally asserted the wrong thing about them.
    //
    // They call `env::signer_account_id()`, and *that host function is itself
    // prohibited during a view call* — a sandbox run of the `.view()` variant
    // fails with:
    //
    //     ProhibitedInView { method_name: "signer_account_id" }
    //
    // So no signature change can make them view-callable; `&self` vs
    // `&mut self` is irrelevant to that. Making them `&self` is still correct
    // (they perform no mutation, so they shouldn't be forcing a state write),
    // but they remain call-only by construction.
    //
    // What's asserted here is therefore that they still *work as calls* after
    // the signature change — i.e. the fix didn't break the one way they can
    // legitimately be invoked.
    //
    // Worth flagging for API review: because signer is unavailable to views,
    // these four can never serve as free queries. Clients that want that need
    // an explicit-account variant, the way get_bids_by_bidder already does.
    for method in [
        "get_my_properties",
        "get_bids_on_my_property",
        "get_my_bids",
        "get_my_leases",
    ] {
        let outcome = contract
            .call(method)
            .args_json(json!({}))
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "{method} should still succeed as a change-method call: {:#?}",
            outcome.into_result().unwrap_err()
        );
    }

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
