# Sheda Contract

Sheda Contract is a production-ready NEAR Protocol smart contract designed for a decentralized property marketplace. It enables users to mint properties as NFTs, list them for sale or lease, and transact using supported stablecoins. The contract implements NEP-171/177/178 standards for Non-Fungible Tokens with comprehensive security features.

## 📚 Documentation

- **[User Guide](README.md)** - This file: execution flow and method reference
- **[Implementation Summary](IMPLEMENTATION_SUMMARY.md)** - Complete feature overview and technical specifications
- **[Gas Optimization Guide](GAS_OPTIMIZATION.md)** - Gas profiling and performance optimization
- **[Production Deployment Guide](PRODUCTION.md)** - Best practices for mainnet deployment

## Table of Contents

- [Overview](#overview)
- [Execution Flow](#execution-flow)
  - [1. Initialization](#1-initialization)
  - [2. Property Management](#2-property-management)
  - [3. Bidding & Purchasing (ft_on_transfer)](#3-bidding--purchasing-ft_on_transfer)
  - [4. Accepting Bids](#4-accepting-bids)
  - [5. Leasing & Disputes](#5-leasing--disputes)
  - [6. Transaction Lifecycle (Escrow + Docs)](#6-transaction-lifecycle-escrow--docs)
  - [7. Timelock Configuration](#7-timelock-configuration)
- [Available Methods](#available-methods)
  - [Initialization](#initialization)
  - [Property Actions](#property-actions)
  - [Bidding Actions](#bidding-actions)
  - [Leasing Actions](#leasing-actions)
  - [Admin Actions](#admin-actions)
  - [View Methods](#view-methods)

## Overview

The contract manages the lifecycle of property NFTs:
1.  **Minting**: Owners create digital representations of properties.
2.  **Listing**: Properties are flagged for sale or lease with a price.
3.  **Bidding**: Buyers/tenants transfer stablecoins to place bids.
4.  **Transaction**: Owners accept bids, transfer the NFT (or create a lease), and receive funds.
5.  **Disputes & Admins**: Admins resolve disputes and manage supported stablecoins.

## Execution Flow

### 1. Initialization
The contract must be initialized with a media URL for metadata and a list of supported stablecoin contract addresses.
- **Method**: `new(media_url: String, supported_stablecoins: Vec<AccountId>)`
- **Effect**: Sets the deployer as the owner and initializes storage.

### 2. Property Management
Users mint properties to tokenize real-world assets.
- **Method**: `mint_property(...)`
- **Effect**: Creates an NFT and a `Property` record. The caller becomes the owner and is indexed in `property_per_owner` for owner-based views.

### 3. Bidding & Purchasing (`ft_on_transfer`)
This is the core mechanism for placing bids. Instead of calling a method on this contract directly, users transfer stablecoins (e.g., USDC) to this contract via the stablecoin's `ft_transfer_call`.

**Flow:**
1.  **User** calls `ft_transfer_call` on the **Stablecoin Contract**.
    *   `receiver_id`: `sheda_contract.near`
    *   `amount`: The bid amount (any value).
    *   `msg`: A JSON string containing the `BidAction`.
        ```json
        {
          "property_id": 1,
          "action": "Purchase", // or "Lease"
          "stablecoin_token": "usdc.testnet"
        }
        ```
2.  **Stablecoin Contract** transfers tokens to `sheda_contract` and calls `ft_on_transfer`.
3.  **Sheda Contract** (`ft_on_transfer`):
    *   Parses `msg` to get `property_id` and `action`.
    *   Verifies the stablecoin is in the `accepted_stablecoin` list.
  *   Verifies the `stablecoin_token` in the message matches the token that called `ft_on_transfer`.
    *   Accepts any bid `amount` (price is advisory for frontends).
    *   Verifies the property is listed for the requested action (Sale/Lease).
    *   Creates a `Bid` record stored in the contract.
  *   Sets an expiry timestamp for the bid (configurable).
    *   Updates internal `stable_coin_balances`.
    *   Returns `0` (keeps all tokens).

### 4. Accepting Bids
The property owner reviews bids and accepts one.
- **Method**: `accept_bid(bid_id, property_id)`
- **Flow**:
    1.  Verifies caller is the property owner.
  2.  Verifies the bid has not expired. If expired, the bid is auto-rejected and refunded.
    2.  **Transfer Funds**: Sends the bid amount (stablecoins) from the contract to the owner.
    3.  **Callback**:
        *   **Success**:
            *   Transfers the NFT to the bidder.
            *   Refunds all *other* bidders for that property.
            *   Updates property status (Sold or Leased).
            *   Removes bids.
        *   **Failure**: Reverts the internal balance update.

### 5. Leasing & Disputes
If a bid is for a lease:
-   A `Lease` object is created on the property.
-   The tenant can `raise_lease_dispute(lease_id)` or `raise_lease_dispute_with_reason(lease_id, reason)`.
-   Admins can `resolve_dispute(lease_id, winner, payout_amount)` which pays escrow to the winner.
-   Admins can `vote_lease_dispute(lease_id, vote_for_tenant)` to record votes.
-   Optional oracle flow: `set_oracle_account(oracle_account)`, `request_oracle_dispute(lease_id)`, then `resolve_dispute_from_oracle(lease_id, payout_amount)`.
-   Once the duration passes, `expire_lease(lease_id)` can be called.
-   `cron_check_leases()` can be called by a keeper to expire any overdue leases in bulk.

### 6. Transaction Lifecycle (Escrow + Docs)
For purchase/lease flows that require document exchange and escrow release, use the new lifecycle methods below instead of `accept_bid`. This keeps funds in escrow until the buyer confirms documents and explicitly releases payment.

**New lifecycle states on Bid:**
`Pending` → `Accepted` → `DocsReleased` → `DocsConfirmed` → `PaymentReleased` → `Completed`

**Recommended flow:**
1. Seller calls `accept_bid_with_escrow(bid_id, property_id)` to mark the bid as `Accepted` and refund competing bids.
2. Seller mints the agreement NFT and transfers it to the buyer, then calls:
  `confirm_document_release(bid_id, property_id, document_token_id)`.
3. Buyer reviews the document and calls:
  `confirm_document_receipt(bid_id, property_id)`.
4. Buyer releases escrowed funds:
  `release_escrow(bid_id, property_id)`.
5. Either party finalizes:
  `complete_transaction(bid_id, property_id)`.

**Escrow timelock:**
- After `confirm_document_receipt`, escrow cannot be released until the configured delay has elapsed.
- Lost bid claims also respect a configurable delay after a sale/lease completes.

**Timeout refunds:**
- If the seller does not release documents after acceptance, or the buyer does not confirm receipt after release, a keeper or cron job can call:
  `refund_escrow_timeout(bid_id, property_id, timeout_nanos)`.
- The bid must be in `Accepted` or `DocsReleased` state, and the timeout is measured from `bid.updated_at`.

**Disputes:**
Either party can call `raise_dispute(bid_id, property_id, reason)` while the bid is `Accepted`, `DocsReleased`, or `DocsConfirmed`.

### Integration Notes
- The `document_token_id` is expected to reference an agreement NFT minted off-chain or by a supporting contract. The backend can store document metadata and links, while this contract only stores the token id.
- Escrow remains locked until the buyer calls `release_escrow`, so client apps should surface this step explicitly after document confirmation.

### 7. Timelock Configuration
Owners can update time-based parameters (in nanoseconds) that control bid expiry and escrow delays.
- **Method**: `set_time_lock_config(bid_expiry_ns, escrow_release_delay_ns, lost_bid_claim_delay_ns)`
- **View**: `get_time_lock_config()`

### 8. Upgrade Governance
Owners can stage upgrades with a configurable delay.
- **Method**: `set_upgrade_delay(delay_ns)`
- **Method**: `propose_upgrade(code)`
- **Method**: `apply_upgrade()`
- **View**: `get_upgrade_status()`

## Available Methods

### Initialization
-   `new(media_url: String, supported_stablecoins: Vec<AccountId>)`: Initialize the contract.

### Property Actions
-   `mint_property(title, description, media_uri, price, is_for_sale, lease_duration_months)`: Create a new property NFT.
-   `delist_property(property_id)`: Remove a property from sale/lease.
-   `delete_property(property_id)`: Burn the NFT and remove the property record.
-   `set_global_contract_code(code)`: Store global instance code for factory deployments (Owner only).
-   `create_property_instance(property_id)`: Deploy a per-property subaccount instance (Owner only).

### Bidding Actions
-   `ft_on_transfer(sender_id, amount, msg)`: **(Called by Stablecoin Contract)** Handles incoming bid deposits.
-   `accept_bid(bid_id, property_id)`: Owner accepts a bid.
-   `accept_bid_with_escrow(bid_id, property_id)`: Owner accepts a bid but keeps funds in escrow for the lifecycle flow.
-   `reject_bid(bid_id, property_id)`: Owner rejects a bid (refunds bidder).
-   `cancel_bid(bid_id, property_id)`: Bidder cancels their bid (refunds bidder).
-   `claim_lost_bid(bid_id, property_id)`: Manually claim a refund if a bid was "lost" after sale/lease (subject to timelock).
-   `confirm_document_release(bid_id, property_id, document_token_id)`: Seller releases the agreement NFT to buyer.
-   `confirm_document_receipt(bid_id, property_id)`: Buyer confirms receipt of agreement NFT.
-   `release_escrow(bid_id, property_id)`: Buyer releases escrowed stablecoin to seller (subject to timelock).
-   `complete_transaction(bid_id, property_id)`: Finalize the transaction once escrow is released.
-   `raise_dispute(bid_id, property_id, reason)`: Raise a dispute for the transaction lifecycle.
-   `refund_escrow_timeout(bid_id, property_id, timeout_nanos)`: Refund a stalled escrow after timeout.

### Leasing Actions
-   `raise_lease_dispute(lease_id)`: Flag a lease for admin review.
-   `raise_lease_dispute_with_reason(lease_id, reason)`: Flag a lease with a reason.
-   `expire_lease(lease_id)`: End a lease after its duration.
-   `cron_check_leases()`: Expire all overdue leases.

### Configuration Actions
-   `set_time_lock_config(bid_expiry_ns, escrow_release_delay_ns, lost_bid_claim_delay_ns)`: Update timelock settings (Owner only).
-   `set_upgrade_delay(delay_ns)`: Set upgrade delay (Owner only).
-   `propose_upgrade(code)`: Propose new code for upgrade (Owner only).
-   `apply_upgrade()`: Apply a pending upgrade after delay (Owner only).

### Admin Actions
-   `add_admin(new_admin_id)`: Add a new admin.
-   `remove_admin(admin_id)`: Remove an admin.
-   `add_supported_stablecoin(token_account)`: Whitelist a stablecoin.
-   `remove_supported_stablecoin(token_account)`: Remove a stablecoin from whitelist.
-   `resolve_dispute(lease_id, winner, payout_amount)`: Resolve a lease dispute and pay escrow to the winner.
-   `resolve_dispute_from_oracle(lease_id, payout_amount)`: Resolve using the oracle result.
-   `vote_lease_dispute(lease_id, vote_for_tenant)`: Admin vote tracking for disputes.
-   `set_oracle_account(oracle_account)`: Set the oracle contract (Owner only).
-   `request_oracle_dispute(lease_id)`: Request an oracle decision (Admin only).
-   `emergency_withdraw(to_account)`: Withdraw all stablecoins to a specific account (Owner only).
-   `withdraw_stablecoin(token_account, amount)`: Withdraw specific amount (Owner only).
-   `refund_bids(property_id)`: Admin manually refunds bids for a property.
-   `admin_delist_property(property_id)`: Admin force-delist a property.
 -   `admin_delete_property(property_id)`: Admin delete a property and burn the NFT.

### View Methods
-   `get_property_by_id(property_id)`
-   `get_properties(from_index, limit)`
-   `get_my_properties()`
-   `get_property_by_owner(owner_id)`
-   `get_bids_for_property(property_id)`
-   `get_my_bids()`
-   `get_all_bids(from_index, limit)`
-   `get_lease_by_id(lease_id)`
-   `supported_stablecoins()`
-   `get_all_admins()`
-   `view_is_admin(account_id)`
-   `get_time_lock_config()`
-   `get_active_leases_count()`
-   `get_user_stats(account_id)`
-   `get_property_instance(property_id)`
-   `get_oracle_account()`
-   `get_upgrade_status()`

## How to Build Locally?

Install [`cargo-near`](https://github.com/near/cargo-near) and run:

```bash
cargo near build
```

## How to Test Locally?

```bash
cargo test
```

## How to Deploy?

Deployment is automated with GitHub Actions CI/CD pipeline.
To deploy manually, install [`cargo-near`](https://github.com/near/cargo-near) and run:

If you deploy for debugging purposes:
```bash
cargo near deploy build-non-reproducible-wasm <account-id>
```

If you deploy production ready smart contract:
```bash
cargo near deploy build-reproducible-wasm <account-id>
```

## Useful Links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract development toolkit for Rust
- [near CLI](https://near.cli.rs) - Interact with NEAR blockchain from command line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://near.chat)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub), [Twitter](https://twitter.com/neardevhub)
