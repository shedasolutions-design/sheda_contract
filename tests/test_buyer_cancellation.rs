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
#[tokio::test]
#[ignore = "needs an NEP-141 fixture to fund a bid; see comment above"]
async fn test_buyer_cancellation_stages_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
