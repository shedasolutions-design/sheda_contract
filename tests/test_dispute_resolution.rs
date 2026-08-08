mod common;

use near_workspaces::types::NearToken;
use serde_json::json;

// A disputed bid used to be a dead end.
// ---------------------------------------------------------------------------
//
// `raise_transaction_dispute` could set a bid to `Disputed`, but nothing in
// the contract ever moved one out again — no refund, no payout, no expiry. The
// buyer's stablecoins sat in the contract permanently. And because a disputed
// bid still holds a claim on its property, the seller could not delete or
// delist it either. Both sides stuck, with the money frozen between them.
//
// `admin_resolve_bid_dispute` is the way out:
//
//   BuyerWins   full refund, deal unwound, property stays with the seller
//   Split       escrow halved, deal unwound; odd unit to the buyer
//   SellerWins  bid returns to DocsConfirmed so the ordinary release path
//               finishes the deal — no second copy of the completion logic

/// Drives a bid to `Disputed` and returns its id.
async fn disputed_bid(fx: &common::Fixture, property_id: u64) -> common::TestResult<u64> {
    let bid_id = fx.place_bid(property_id, true).await?;

    fx.seller
        .call(fx.contract.id(), "accept_bid_with_escrow")
        .args_json(json!({ "bid_id": bid_id, "property_id": property_id }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    fx.buyer
        .call(fx.contract.id(), "raise_transaction_dispute")
        .args_json(json!({
            "bid_id": bid_id,
            "property_id": property_id,
            "reason": "Seller stopped responding after the viewing",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Disputed"),
    );
    Ok(bid_id)
}

#[tokio::test]
async fn test_buyer_wins_refunds_in_full() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = disputed_bid(&fx, property_id).await?;
    let buyer_before = fx.ft_balance(fx.buyer.id()).await?;

    let resolved = fx
        .contract
        .call("admin_resolve_bid_dispute")
        .args_json(json!({
            "property_id": property_id,
            "bid_id": bid_id,
            "resolution": "BuyerWins",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        resolved.is_success(),
        "{:#?}",
        resolved.into_result().unwrap_err()
    );

    assert_eq!(
        fx.ft_balance(fx.buyer.id()).await?,
        buyer_before + common::BID_AMOUNT,
    );
    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Cancelled"),
    );

    // The property was never the seller's to lose, and with the bid settled it
    // no longer blocks them acting on it.
    assert_eq!(
        fx.property_owner(property_id).await?.as_deref(),
        Some(fx.seller.id().as_str()),
    );

    Ok(())
}

#[tokio::test]
async fn test_split_halves_the_escrow_and_rounds_to_the_buyer() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = disputed_bid(&fx, property_id).await?;

    let buyer_before = fx.ft_balance(fx.buyer.id()).await?;
    let seller_before = fx.ft_balance(fx.seller.id()).await?;

    fx.contract
        .call("admin_resolve_bid_dispute")
        .args_json(json!({
            "property_id": property_id,
            "bid_id": bid_id,
            "resolution": "Split",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let seller_half = common::BID_AMOUNT / 2;
    let buyer_half = common::BID_AMOUNT - seller_half;

    assert_eq!(
        fx.ft_balance(fx.buyer.id()).await?,
        buyer_before + buyer_half
    );
    assert_eq!(
        fx.ft_balance(fx.seller.id()).await?,
        seller_before + seller_half,
    );
    // Nothing may be created or destroyed in a settlement.
    assert_eq!(buyer_half + seller_half, common::BID_AMOUNT);

    Ok(())
}

#[tokio::test]
async fn test_seller_wins_returns_the_bid_to_the_normal_path() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = disputed_bid(&fx, property_id).await?;

    let buyer_before = fx.ft_balance(fx.buyer.id()).await?;
    let seller_before = fx.ft_balance(fx.seller.id()).await?;

    fx.contract
        .call("admin_resolve_bid_dispute")
        .args_json(json!({
            "property_id": property_id,
            "bid_id": bid_id,
            "resolution": "SellerWins",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // Deliberately pays no one: the escrow stays put and the deal finishes
    // down the ordinary release path, which is what knows to move ownership.
    assert_eq!(fx.ft_balance(fx.buyer.id()).await?, buyer_before);
    assert_eq!(fx.ft_balance(fx.seller.id()).await?, seller_before);
    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("DocsConfirmed"),
    );

    Ok(())
}

#[tokio::test]
async fn test_only_admins_can_resolve_and_undisputed_bids_are_rejected() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = disputed_bid(&fx, property_id).await?;

    // Settling a dispute moves someone else's money, so it cannot be open to
    // the parties themselves — the buyer would simply always win.
    let by_buyer = fx
        .buyer
        .call(fx.contract.id(), "admin_resolve_bid_dispute")
        .args_json(json!({
            "property_id": property_id,
            "bid_id": bid_id,
            "resolution": "BuyerWins",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(by_buyer.is_failure(), "a bidder settled their own dispute");
    assert_eq!(
        fx.bid_status(property_id, bid_id).await?.as_deref(),
        Some("Disputed"),
    );

    // And a bid that was never disputed has nothing to settle — resolving one
    // would be a way to pull escrow out of a live deal.
    let live_bid = fx.place_bid(property_id, true).await?;
    let not_disputed = fx
        .contract
        .call("admin_resolve_bid_dispute")
        .args_json(json!({
            "property_id": property_id,
            "bid_id": live_bid,
            "resolution": "BuyerWins",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    assert!(
        not_disputed.is_failure(),
        "an undisputed bid was settled as if it were disputed",
    );

    Ok(())
}

#[tokio::test]
async fn test_disputed_bids_are_listed_for_admins() -> common::TestResult {
    let worker = near_workspaces::sandbox().await?;
    let fx = common::setup(&worker).await?;

    let property_id = fx.mint_property(true).await?;
    let bid_id = disputed_bid(&fx, property_id).await?;

    // Without this an admin would have to scan every property to find what
    // needs settling.
    let disputed = fx
        .contract
        .view("get_disputed_bids")
        .await?
        .json::<Vec<serde_json::Value>>()?;

    assert_eq!(disputed.len(), 1);
    assert_eq!(disputed[0]["id"].as_u64(), Some(bid_id));

    Ok(())
}
