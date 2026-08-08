mod common;

use near_workspaces::types::NearToken;
use serde_json::json;

// Buyer cancellation stages
// ---------------------------------------------------------------------------
//
// `cancel_bid` only ever covered a `Pending` bid. Once a bid was accepted the
// buyer was committed with no exit, so a deal that stalled — seller goes
// quiet, agreement never arrives — left their funds in escrow indefinitely
// with only an admin refund to fall back on.
//
// Two exits, and one deliberate absence:
//
//   Accepted      -> buyer_cancel_accepted_bid            clean refund
//   DocsReleased  -> buyer_reject_documents_and_cancel    burn agreement, refund
//   DocsConfirmed -> (none)                               committed
//
// The asymmetry is the whole design. The agreement is the real-world contract
// the two sides settled on after their appointments; the seller mints it to
// the buyer as an NFT. Confirming it is the commitment, so there is no exit
// past that point — otherwise a buyer could accept the terms and walk. And
// rejecting it burns the agreement, so a buyer who backs out cannot keep the
// document they never paid for. Together those two rules are what make it safe
// for a seller to hand the agreement over at all.

/// The one guard reachable without a funded bid: the calls reject a property
/// that has no bids at all, rather than silently succeeding.
///
/// Everything past this point needs a bid in a specific non-`Pending` status,
/// which cannot be reached without real escrow — see the ignored test below.
#[tokio::test]
async fn test_cancel_stages_reject_unknown_bid() -> Result<(), Box<dyn std::error::Error>> {
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

    let buyer = sandbox.dev_create_account().await?;

    for method in [
        "buyer_cancel_accepted_bid",
        "buyer_reject_documents_and_cancel",
    ] {
        let outcome = buyer
            .call(contract.id(), method)
            .args_json(json!({ "bid_id": 1, "property_id": 1 }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?;
        assert!(
            outcome.is_failure(),
            "{} should reject a bid that does not exist",
            method
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
//
// Reaching `Accepted` / `DocsReleased` / `DocsConfirmed` requires a funded bid,
// which means deploying a real NEP-141 token, registering storage for
// buyer/seller/contract, and driving ft_transfer_call -> accept -> document
// release. This repo still has no FT fixture — the same gap that blocks the
// ownership-transfer test in test_listing_and_ownership.rs.
//
// Left ignored with the assertions spelled out, matching that test's
// convention, so it can be finished by dropping in an FT wasm rather than
// re-deriving what to check:
//
//   cancelling an Accepted bid, inside the window:
//     buyer_cancel_accepted_bid          succeeds
//     bid status                         == Cancelled
//     bidder's FT balance                refunded in full
//     get_stablecoin_balance             decremented by the bid amount
//     property.is_for_sale               == true   (released back to market)
//     BidCancelledByBuyer                stage "accepted", previous "Accepted"
//
//   rejecting the agreement, inside the window -- the case that matters most,
//   because it is the one where the seller has already given something up:
//     buyer_reject_documents_and_cancel  succeeds
//     nft_token(doc:<prop>:<bid>)        == null    (agreement destroyed)
//     bid.document_token_id              == null    (no dangling reference)
//     buyer no longer holds the document in nft_tokens_for_owner
//     bidder refunded in full, property back on the market
//     BidCancelledByBuyer                stage "documents_rejected"
//
//   the point of no return -- the invariant the seller relies on:
//     after confirm_document_receipt, the bid is DocsConfirmed and NO
//     cancellation entrypoint accepts it. buyer_cancel_accepted_bid and
//     buyer_reject_documents_and_cancel both fail on a status mismatch, and
//     the buyer keeps the agreement because the deal is still live.
//     If a future change adds an exit at DocsConfirmed, this assertion is what
//     should stop it.
//
//   past the window (sandbox fast_forward beyond bid.updated_at + window):
//     the call fails, error names the stage and how long ago it closed
//     bid status                         unchanged
//     no refund, and the agreement is NOT burned -- a buyer who missed the
//     window must not be able to destroy the seller's document on the way out
//
//   wrong stage for the bid's status:
//     buyer_reject_documents_and_cancel on an Accepted bid fails, and the
//     error names both actual and expected so the buyer knows which call to
//     use instead
//
//   wrong caller:
//     a non-bidder calling either entrypoint fails, even inside the window,
//     and in particular cannot burn someone else's agreement

// The paths below are now driven for real against an NEP-141 fixture
// (tests/common/mod.rs), so the escrow movement is asserted on balances
// rather than described.

/// The buyer's exit at `Accepted`: the bid is cancelled and the money comes
/// back. This is the stage where the appointment happens, and appointments
/// fall through for reasons the chain cannot see, so the exit is unconditional
/// — no window, no seller approval.
#[tokio::test]
async fn test_buyer_cancel_accepted_bid_refunds() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = fx.place_bid(property_id, true).await?;

    let balance_after_bid = fx.ft_balance(fx.buyer.id()).await?;

    let accept = fx
        .seller
        .call(fx.contract.id(), "accept_bid_with_escrow")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        accept.is_success(),
        "{:#?}",
        accept.into_result().unwrap_err()
    );
    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Accepted")
    );

    let cancel = fx
        .buyer
        .call(fx.contract.id(), "buyer_cancel_accepted_bid")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        cancel.is_success(),
        "{:#?}",
        cancel.into_result().unwrap_err()
    );

    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Cancelled"),
    );
    assert_eq!(
        fx.ft_balance(fx.buyer.id()).await?,
        balance_after_bid + common::BID_AMOUNT,
        "escrow was not returned to the buyer",
    );

    Ok(())
}

/// Only the bidder may take their own exit. A cancellation refunds escrow and,
/// one stage later, burns the seller's agreement — so letting a bystander call
/// it would hand them both someone else's money movement and a way to destroy
/// a document they have no claim on.
#[tokio::test]
async fn test_only_the_bidder_can_cancel() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = fx.place_bid(property_id, true).await?;

    fx.seller
        .call(fx.contract.id(), "accept_bid_with_escrow")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // The seller is the most plausible wrong caller: they have a real interest
    // in the bid, just not the right to withdraw it.
    let stolen = fx
        .seller
        .call(fx.contract.id(), "buyer_cancel_accepted_bid")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        stolen.is_failure(),
        "a non-bidder cancelled someone else's bid",
    );
    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Accepted"),
        "the failed call still moved the bid",
    );

    Ok(())
}

/// Escrow cannot be stranded by deleting the property out from under it.
///
/// This is the guard from the first half of this PR, and until now it had no
/// automated coverage at all — only a manual checklist.
#[tokio::test]
async fn test_delete_blocked_while_a_bid_holds_escrow() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = fx.place_bid(property_id, true).await?;

    let active = fx
        .contract
        .view("get_active_bids_for_property")
        .args_json(json!({ "property_id": property_id }))
        .await?
        .json::<Vec<serde_json::Value>>()?;
    assert_eq!(active.len(), 1, "the pending bid should read as blocking");

    let blocked = fx
        .seller
        .call(fx.contract.id(), "delete_property")
        .args_json(json!({ "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        blocked.is_failure(),
        "a property was deleted while a bid still held escrow against it",
    );

    // The error has to name the blocking bid — "cannot delete" alone leaves
    // the owner with no idea what to resolve.
    let message = format!("{:?}", blocked.into_result().unwrap_err());
    assert!(
        message.contains(&format!("bid #{}", bid_id)) && message.contains(fx.buyer.id().as_str()),
        "the panic did not identify the blocking bid: {message}",
    );

    // Once the bid is gone the same delete succeeds, so the guard is releasing
    // properly rather than wedging the property permanently.
    fx.buyer
        .call(fx.contract.id(), "cancel_bid")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let allowed = fx
        .seller
        .call(fx.contract.id(), "delete_property")
        .args_json(json!({ "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        allowed.is_success(),
        "delete still blocked after the bid was cancelled: {:#?}",
        allowed.into_result().unwrap_err(),
    );

    Ok(())
}
