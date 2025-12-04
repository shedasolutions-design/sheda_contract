# Sheda Contract

Sheda Contract is a NEAR Protocol smart contract designed for a decentralized property marketplace. It enables users to mint properties as NFTs, list them for sale or lease, and transact using supported stablecoins. The contract implements the NEP-171 standard for Non-Fungible Tokens.

## Table of Contents

- [Overview](#overview)
- [Execution Flow](#execution-flow)
  - [1. Initialization](#1-initialization)
  - [2. Property Management](#2-property-management)
  - [3. Bidding & Purchasing (ft_on_transfer)](#3-bidding--purchasing-ft_on_transfer)
  - [4. Accepting Bids](#4-accepting-bids)
  - [5. Leasing & Disputes](#5-leasing--disputes)
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
2.  **Listing**: Properties can be marked for sale or lease with a set price.
3.  **Bidding**: Buyers/Tenants send stablecoins to the contract to place bids.
4.  **Transacting**: Owners accept bids, transferring the NFT (or creating a lease) and receiving funds.
5.  **Dispute Resolution**: Admins can intervene in lease disputes.

## Execution Flow

### 1. Initialization
The contract must be initialized with a media URL for metadata and a list of supported stablecoin contract addresses.
- **Method**: `new(media_url: String, supported_stablecoins: Vec<AccountId>)`
- **Effect**: Sets the deployer as the owner and initializes storage.

### 2. Property Management
Users mint properties to tokenize real-world assets.
- **Method**: `mint_property(...)`
- **Effect**: Creates an NFT and a `Property` record. The caller becomes the owner.

### 3. Bidding & Purchasing (`ft_on_transfer`)
This is the core mechanism for placing bids. Instead of calling a method on this contract directly, users transfer stablecoins (e.g., USDC) to this contract via the stablecoin's `ft_transfer_call`.

**Flow:**
1.  **User** calls `ft_transfer_call` on the **Stablecoin Contract**.
    *   `receiver_id`: `sheda_contract.near`
    *   `amount`: The bid amount (must match property price).
    *   `msg`: A JSON string containing the `BidAction`.
        ```json
        {
          "property_id": 1,
          "action": "Purchase" // or "Lease"
        }
        ```
2.  **Stablecoin Contract** transfers tokens to `sheda_contract` and calls `ft_on_transfer`.
3.  **Sheda Contract** (`ft_on_transfer`):
    *   Parses `msg` to get `property_id` and `action`.
    *   Verifies the stablecoin is in the `accepted_stablecoin` list.
    *   Verifies the transferred `amount` matches the property's `price`.
    *   Verifies the property is listed for the requested action (Sale/Lease).
    *   Creates a `Bid` record stored in the contract.
    *   Updates internal `stable_coin_balances`.
    *   Returns `0` (keeps all tokens).

### 4. Accepting Bids
The property owner reviews bids and accepts one.
- **Method**: `accept_bid(bid_id, property_id)`
- **Flow**:
    1.  Verifies caller is the property owner.
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
-   The tenant or owner can `raise_dispute(lease_id)`.
-   Admins can `resolve_dispute(lease_id)`.
-   Once the duration passes, `expire_lease(lease_id)` can be called.

## Available Methods

### Initialization
-   `new(media_url: String, supported_stablecoins: Vec<AccountId>)`: Initialize the contract.

### Property Actions
-   `mint_property(title, description, media_uri, price, is_for_sale, lease_duration_months)`: Create a new property NFT.
-   `delist_property(property_id)`: Remove a property from sale/lease (must have no active bids).
-   `delete_property(property_id)`: Burn the NFT and remove the property record.

### Bidding Actions
-   `ft_on_transfer(sender_id, amount, msg)`: **(Called by Stablecoin Contract)** Handles incoming bid deposits.
-   `accept_bid(bid_id, property_id)`: Owner accepts a bid.
-   `reject_bid(bid_id, property_id)`: Owner rejects a bid (refunds bidder).
-   `cancel_bid(bid_id, property_id)`: Bidder cancels their bid (refunds bidder).
-   `claim_lost_bid(bid_id, property_id)`: Manually claim a refund if a bid was "lost" (e.g., due to gas limits during acceptance).

### Leasing Actions
-   `raise_dispute(lease_id)`: Flag a lease for admin review.
-   `expire_lease(lease_id)`: End a lease after its duration.

### Admin Actions
-   `add_admin(new_admin_id)`: Add a new admin.
-   `remove_admin(admin_id)`: Remove an admin.
-   `add_supported_stablecoin(token_account)`: Whitelist a stablecoin.
-   `remove_supported_stablecoin(token_account)`: Remove a stablecoin from whitelist.
-   `resolve_dispute(lease_id)`: Mark a dispute as resolved.
-   `emergency_withdraw(to_account)`: Withdraw all stablecoins to a specific account (Owner only).
-   `withdraw_stablecoin(token_account, amount)`: Withdraw specific amount (Owner only).
-   `refund_bids(property_id)`: Admin manually refunds bids for a property.
-   `admin_delist_property(property_id)`: Admin force-delist a property.

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
