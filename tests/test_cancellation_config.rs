use near_workspaces::types::NearToken;
use serde_json::json;

const NS_PER_HOUR: u64 = 60 * 60 * 1_000_000_000;

/// Expected defaults, in the order `get_cancellation_windows` returns them:
/// path A, path B stages 1-3, dispute timelock, lease early termination.
const EXPECTED_DEFAULTS: [u64; 6] = [
    NS_PER_HOUR,
    24 * NS_PER_HOUR,
    48 * NS_PER_HOUR,
    24 * NS_PER_HOUR,
    72 * NS_PER_HOUR,
    7 * 24 * NS_PER_HOUR,
];

async fn deploy() -> Result<near_workspaces::Contract, Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let init = contract
        .call("new")
        .args_json(json!({
            "media_url": "https://example.com/sheda-icon.png",
            "supported_stablecoins": [],
        }))
        .transact()
        .await?;
    assert!(init.is_success(), "{:#?}", init.into_result().unwrap_err());

    Ok(contract)
}

/// A fresh contract reports v4 and the documented default windows.
///
/// `get_version` exists because there was previously no way to ask a deployed
/// contract which state layout it held — confirming whether a migration had
/// run meant calling view methods until one panicked. It also doubles as a
/// deserialization canary: it reads the state struct end to end, so it only
/// answers if the layout on chain matches the deployed code.
#[tokio::test]
async fn test_version_and_default_windows() -> Result<(), Box<dyn std::error::Error>> {
    let contract = deploy().await?;

    let version: u32 = contract.view("get_version").await?.json()?;
    assert_eq!(version, 4, "a freshly initialised contract should be v4");

    let windows: [u64; 6] = contract.view("get_cancellation_windows").await?.json()?;
    assert_eq!(
        windows, EXPECTED_DEFAULTS,
        "default cancellation windows drifted from the documented values"
    );

    Ok(())
}

/// The owner can retune individual windows; `None` leaves the rest alone.
#[tokio::test]
async fn test_set_cancellation_windows_partial_update() -> Result<(), Box<dyn std::error::Error>> {
    let contract = deploy().await?;

    let outcome = contract
        .call("set_cancellation_windows")
        .args_json(json!({
            "path_a_ns": 2 * NS_PER_HOUR,
            "path_b_stage1_ns": null,
            "path_b_stage2_ns": null,
            "path_b_stage3_ns": null,
            "dispute_timelock_ns": 96 * NS_PER_HOUR,
            "lease_early_termination_ns": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "{:#?}",
        outcome.into_result().unwrap_err()
    );

    let windows: [u64; 6] = contract.view("get_cancellation_windows").await?.json()?;
    assert_eq!(windows[0], 2 * NS_PER_HOUR, "path A should be updated");
    assert_eq!(
        windows[4],
        96 * NS_PER_HOUR,
        "dispute timelock should be updated"
    );
    assert_eq!(
        windows[1], EXPECTED_DEFAULTS[1],
        "an omitted window must keep its previous value, not reset to zero"
    );
    assert_eq!(
        windows[5], EXPECTED_DEFAULTS[5],
        "an omitted window must keep its previous value, not reset to zero"
    );

    Ok(())
}

/// Zero is rejected rather than stored.
///
/// A zero-length window would let a buyer cancel at any point with no time
/// bound — the opposite of what these gates exist to enforce — and it is also
/// the value an uninitialised field would hold, so storing it deliberately
/// would be indistinguishable from a migration that never ran.
#[tokio::test]
async fn test_zero_window_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let contract = deploy().await?;

    let outcome = contract
        .call("set_cancellation_windows")
        .args_json(json!({
            "path_a_ns": 0,
            "path_b_stage1_ns": null,
            "path_b_stage2_ns": null,
            "path_b_stage3_ns": null,
            "dispute_timelock_ns": null,
            "lease_early_termination_ns": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "a zero-length cancellation window should be rejected"
    );

    let windows: [u64; 6] = contract.view("get_cancellation_windows").await?.json()?;
    assert_eq!(
        windows, EXPECTED_DEFAULTS,
        "a rejected update must not partially apply"
    );

    Ok(())
}

/// Only the owner may retune the windows.
#[tokio::test]
async fn test_non_owner_cannot_set_windows() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let init = contract
        .call("new")
        .args_json(json!({
            "media_url": "https://example.com/sheda-icon.png",
            "supported_stablecoins": [],
        }))
        .transact()
        .await?;
    assert!(init.is_success(), "{:#?}", init.into_result().unwrap_err());

    let stranger = sandbox.dev_create_account().await?;
    let outcome = stranger
        .call(contract.id(), "set_cancellation_windows")
        .args_json(json!({
            "path_a_ns": 5 * NS_PER_HOUR,
            "path_b_stage1_ns": null,
            "path_b_stage2_ns": null,
            "path_b_stage3_ns": null,
            "dispute_timelock_ns": null,
            "lease_early_termination_ns": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "a non-owner should not be able to retune cancellation windows"
    );

    let windows: [u64; 6] = contract.view("get_cancellation_windows").await?.json()?;
    assert_eq!(windows, EXPECTED_DEFAULTS, "windows must be unchanged");

    Ok(())
}
