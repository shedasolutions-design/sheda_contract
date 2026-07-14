# Sheda Contract

Sheda Contract is a NEAR smart-contract suite (written in Rust) that powers Sheda Solutions' real-estate operations platform. It models properties as NFTs, supports purchase and lease workflows with on-contract escrow, dispute resolution, and optional per-property contract instances. This repository is the canonical public implementation of the Sheda on-chain marketplace and escrow logic.

## High-level summary

- Properties are represented as NEP-171 NFTs (token IDs == property IDs). Each property stores additional domain data (price, lease settings, escrow amounts, owner data).
- Buyers place bids by transferring supported stablecoins to the contract (ft_on_transfer). Bids lock funds in contract-managed escrow (internal balance tracking).
- Sellers choose between two acceptance flows:
  - Fast path: `accept_bid` — immediate payment + NFT transfer (suitable for trusted/quick sales).
  - Secure path: `accept_bid_with_escrow` — document exchange + timelocks + buyer-triggered release (multi-stage escrow suitable for higher-value deals).
- Leasing is supported with per-property Lease structs, escrows for damage, and lifecycle management (start/end/expiry handling).
- Admins and an owner account can perform governance actions: resolve disputes, set oracle account for automated dispute resolution, configure timelocks, and manage per-property instances or upgrades.
- The contract emits structured events for off-chain monitoring (see src/events.rs).

## Stack
- Language: Rust (edition 2021)
- Runtime: NEAR smart contracts (WASM)
- Notable libraries: near-sdk (NEAR Rust SDK), near-contract-standards (NFT interfaces), borsh (state serialization)

## How it's organized

```
Cargo.toml                 # crate manifest and reproducible-build metadata
README.md                  # this document
BIDDING_ESCROW_IMPLEMENTATION.md  # detailed design & state-machine diagrams for escrow flow
INTEGRATION_GUIDE.md       # integration notes for front-end/back-end
sheda_contract_abi.json    # ABI for client integrations
src/                       # contract source
  lib.rs                   # contract root: state, core NEP implementations, public API
  internal.rs              # internal helpers and locking primitives
  models.rs                # domain models: Property, Bid, Lease, BidStatus, etc.
  admin.rs                 # owner/admin functions and dispute resolution
  views.rs                 # readonly/view helper functions
  events.rs                # event structs & emit helpers
tests/                     # integration/unit tests and test utilities
```

How it fits together:
- lib.rs defines the on-chain state (ShedaContract) and implements standard NFT traits (NonFungibleTokenCore, Approval, Enumeration, Metadata). Business logic is split across modules: `internal` for helpers/locking, `admin` for governance, and `views` for read-only queries. Bids and leases are stored in IterableMap collections keyed by property id.

## Key concepts and data model

- Property (NFT): id, owner_id, description, metadata_uri, price, is_for_sale, lease settings, damage escrow, active_lease, sold info.
- Bid: id, bidder, property_id, amount, status (Pending, Accepted, Rejected, DocsReleased, DocsConfirmed, PaymentReleased, Completed, Disputed), document fields, escrow metadata, stablecoin token.
- Lease: id, property_id, tenant_id, start/end timestamps, active flag, dispute status, escrow_held and token.
- Admins: IterableSet of account IDs with elevated privileges. Owner: account that deployed/initialized contract.

See src/models.rs for exact struct fields and JSON schema annotations.

## Public API (high level)

Initialization:
- new(media_url: String, supported_stablecoins: Vec<AccountId>) — initialize contract and set supported stablecoins.
- migrate() / upgrade_self() / propose_upgrade() / apply_upgrade() — state migration and upgrade governance.

Property management:
- mint_property(title, description, media_uri, price: U128, is_for_sale, lease_duration_months) -> property_id
- delist_property(property_id)
- delete_property(property_id)
- create_property_instance(property_id) -> deploys a per-property subaccount and deploys provided global contract code

Bidding & escrow (core flows):
- ft_on_transfer(sender_id, amount: U128, msg: String) -> U128 — entrypoint called by supported FT tokens to place a bid. `msg` must be a JSON-encoded BidAction { property_id, action, stablecoin_token }.
- accept_bid(bid_id, property_id) -> Promise — fast path: immediately finalize sale/transfer (Path A).
- accept_bid_with_escrow(bid_id, property_id) -> bool — secure path using document exchange and timelocks (Path B).
- reject_bid / cancel_bid — reject or cancel bid and refund as appropriate.
- confirm_document_release / confirm_document_receipt — document exchange steps used in Path B.
- release_escrow(bid_id, property_id) -> Promise — trigger escrow release (buyer action or timelock-based).
- complete_transaction(bid_id, property_id) — finalizes the deal and transfers asset/funds.
- claim_lost_bid(bid_id, property_id) — bidder can claim refunded funds when a property is sold/leased to someone else and timelock passed.

Leases & disputes:
- raise_lease_dispute(lease_id) / raise_dispute(bid_id, property_id, reason)
- resolve_dispute(lease_id, winner, payout_amount) — admin-only to resolve disputes and pay out escrow.
- get_leases_with_disputes() — admin view to fetch active disputes.

Views / Read-only helpers (examples):
- get_property_by_id(property_id) -> Option<PropertyView>
- get_properties(from_index, limit) -> Vec<PropertyView>
- get_property_by_owner(owner_id) -> Vec<PropertyView>
- get_bids_for_property(property_id)
- get_stablecoin_balance(token_account)
- supported_stablecoins()
- get_time_lock_config(), get_upgrade_status(), get_oracle_account(), get_all_admins()

Most view functions live in src/views.rs — check there for exact return types and view-struct shapes.

## Flows & diagrams

The repository includes BIDDING_ESCROW_IMPLEMENTATION.md which documents both escrow flows in detail (Path A: fast acceptance; Path B: escrow with document verification, timelocks, and dispute paths). When integrating a client UI, follow that document closely — it contains the state machine, callbacks, and event mapping used by off-chain services.

## Integration notes & ABI

- sheda_contract_abi.json is provided for auto-generating client bindings or building type-safe calls from JavaScript/TypeScript.
- INTEGRATION_GUIDE.md contains front-end/back-end integration notes and recommended event listeners for indexing.

## Build, test and deploy (short)

Prerequisites
- Rust toolchain (use rust-toolchain.toml in repo)
- wasm32 target: `rustup target add wasm32-unknown-unknown`
- cargo-near: `cargo install cargo-near`
- NEAR CLI (optional): https://near.cli.rs

Build
```bash
cargo near build
```

Run unit tests
```bash
cargo test
```

Manual deploy
```bash
# debug / non-reproducible
cargo near deploy build-non-reproducible-wasm <account-id>
# reproducible / production
cargo near deploy build-reproducible-wasm <account-id>
```

CI: This repo is configured for reproducible builds — see `package.metadata.near.reproducible_build` in Cargo.toml.

## Events & off-chain indexing

The contract emits JSON-serializable events (src/events.rs) such as BidPlaced, BidApproved, DealFinalized, DisputeRaised, DisputeResolved, LeaseExpired, and more. Use these events for indexing and building an off-chain marketplace UI or notifications.

## Where to look next (developer checklist)
- Read `BIDDING_ESCROW_IMPLEMENTATION.md` for escrow state machines and diagrams.
- Inspect `src/lib.rs` for public entrypoints and standard trait implementations (NFT standard overrides).
- Inspect `src/internal.rs` for locking and helper primitives used to prevent reentrancy and coordinate async callbacks.
- Inspect `INTEGRATION_GUIDE.md` and `sheda_contract_abi.json` to wire frontends/agents.

## Contribution & license
-
