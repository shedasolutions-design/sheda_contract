use serde_json::json;

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;

    test_basics_on(&contract_wasm).await?;
    Ok(())
}

// Smoke test: deploy, initialize, mint a property, and read it back through
// the real public API. This replaces stale `set_greeting`/`get_greeting`
// boilerplate left over from the `cargo near` template — those methods
// don't exist anywhere in src/, so this test was failing (or simply not
// exercising the contract at all) regardless of any other change.
async fn test_basics_on(contract_wasm: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(contract_wasm).await?;

    // Initialize the contract as its own deployer account (becomes owner_id/admin).
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

    // Deployer should be registered as owner + admin.
    let is_owner_admin: bool = contract
        .view("is_admin")
        .args_json(json!({ "account_id": contract.id().to_string() }))
        .await?
        .json()?;
    assert!(is_owner_admin, "deployer should be an admin after init");

    // Property counter starts at zero.
    let counter_before: u64 = contract
        .view("get_property_counter")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(counter_before, 0);

    // Mint a property and confirm it's tracked correctly.
    let mint_outcome = contract
        .call("mint_property")
        .args_json(json!({
            "title": "Test Property",
            "description": "A test listing",
            "media_uri": "https://example.com/property.png",
            "price": "1000000",
            "is_for_sale": true,
            "lease_duration_months": null,
        }))
        .transact()
        .await?;
    assert!(
        mint_outcome.is_success(),
        "{:#?}",
        mint_outcome.into_result().unwrap_err()
    );

    let counter_after: u64 = contract
        .view("get_property_counter")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(counter_after, 1);

    let property: serde_json::Value = contract
        .view("get_property_by_id")
        .args_json(json!({ "property_id": 0 }))
        .await?
        .json()?;
    assert_eq!(property["owner_id"], contract.id().to_string());
    assert_eq!(property["is_for_sale"], true);

    Ok(())
}
