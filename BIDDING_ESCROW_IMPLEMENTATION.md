# Bidding & Escrow System - Current Implementation

## Overview

The Sheda Contract implements a dual-path bidding and escrow system for property transactions (Purchase/Lease). Funds are committed to escrow when bids are placed, and released through different workflows depending on the transaction type.

---

## Architecture

### Core Components

1. **Bid Placement**: Via FT (Fungible Token) transfer using NEP-141 `ft_on_transfer`
2. **Escrow Holding**: Contract holds stablecoins in `stable_coin_balances`
3. **Acceptance Paths**: Two distinct workflows
   - Path A: `accept_bid` - Immediate payment and NFT transfer
   - Path B: `accept_bid_with_escrow` - Escrow holding with staged release
4. **Release Mechanisms**: Manual release with timelock protection

---

## Data Models

### Bid Structure
```rust
pub struct Bid {
    pub id: u64,
    pub bidder: AccountId,
    pub property_id: u64,
    pub amount: u128,                        // Stablecoin amount in atomic units
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub status: BidStatus,
    pub document_token_id: Option<String>,   // Document NFT reference
    pub escrow_release_tx: Option<String>,   // Release transaction marker
    pub dispute_reason: Option<String>,
    pub expires_at: Option<Timestamp>,       // Auto-expiry time
    pub escrow_release_after: Option<Timestamp>, // Timelock for release
    pub action: Action,                      // Purchase or Lease
    pub stablecoin_token: AccountId,         // Which token was used
}
```

### Bid Status Lifecycle
```
Pending → Accepted → DocsReleased → DocsConfirmed → PaymentReleased → Completed
   ↓          ↓            ↓               ↓
Cancelled  Rejected    Disputed        Disputed
```

### Action Types
```rust
pub enum Action {
    Purchase,  // Full ownership transfer
    Lease,     // Temporary usage with escrow
}
```

---

## Payment Flow Implementation

### 1. BID PLACEMENT (Escrow Commitment)

**Entry Point**: `ft_on_transfer(sender_id, amount, msg)`

**Process**:
```
Buyer                    Token Contract              Sheda Contract
  |                            |                            |
  |--ft_transfer_call(amount)-->|                            |
  |                            |--ft_on_transfer(amount)---->|
  |                            |                            |
  |                            |                      [Create Bid]
  |                            |                      [Lock Funds]
  |                            |                      [Status: Pending]
  |                            |<-------return U128(0)------|
  |<----callback success-------|                            |
```

**Implementation Details**:
```rust
pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
    // 1. Parse bid action (Purchase/Lease)
    let bid_action: BidAction = serde_json::from_str(&msg)?;
    
    // 2. Validate property exists and action matches availability
    let property = self.properties.get(&bid_action.property_id)?;
    match bid_action.action {
        Action::Purchase => require!(property.is_for_sale),
        Action::Lease => require!(property.lease_duration_months.is_some()),
    }
    
    // 3. Verify stablecoin is accepted
    require!(self.accepted_stablecoin.contains(&env::predecessor_account_id()));
    
    // 4. Create bid with expiry
    let expires_at = if self.bid_expiry_ns == 0 {
        None
    } else {
        Some(env::block_timestamp() + self.bid_expiry_ns)
    };
    
    let bid = Bid {
        id: bid_id,
        property_id,
        bidder: sender_id,
        amount: amount.0,
        status: BidStatus::Pending,
        expires_at,
        // ... other fields
    };
    
    // 5. Store bid
    self.bids.entry(property_id).or_insert(Vec::new()).push(bid);
    
    // 6. Track escrow balance
    let current_balance = self.stable_coin_balances.get(&token).unwrap_or(&0);
    self.stable_coin_balances.insert(token, current_balance + amount.0);
    
    // 7. Return 0 (keep all tokens in contract)
    U128(0)
}
```

**Key Features**:
- ✅ Immediate escrow commitment (funds locked at bid time)
- ✅ Validation of property availability
- ✅ Automatic bid expiration tracking
- ✅ Multi-stablecoin support
- ✅ Reentrancy protection via locks

**Configuration**:
- `bid_expiry_ns`: Default 7 days (7 * 24 * 60 * 60 * 1_000_000_000)
- Can be disabled by setting to 0

---

### 2. BID ACCEPTANCE - PATH A: Immediate Payment

**Entry Point**: `accept_bid(bid_id, property_id)`

**Use Case**: Simple purchase where payment and NFT transfer happen immediately

**Process**:
```
Seller                     Sheda Contract                Token Contract
  |                              |                              |
  |--accept_bid(bid_id)--------->|                              |
  |                              |                              |
  |                        [Update Status]                      |
  |                        [Status: Accepted]                   |
  |                              |                              |
  |                              |--ft_transfer(seller, amt)---->|
  |                              |                              |
  |                              |<----callback success---------|
  |                              |                              |
  |                        [Transfer NFT]                       |
  |                        [Status: Completed]                  |
  |                        [Refund other bids]                  |
  |<----promise resolved---------|                              |
```

**Implementation Details**:
```rust
pub fn internal_accept_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) -> Promise {
    // 1. Lock bid to prevent reentrancy
    lock_bid(contract, property_id, bid_id);
    
    // 2. Verify ownership
    let property = contract.properties.get(&property_id)?;
    require!(property.owner_id == env::predecessor_account_id());
    
    // 3. Check expiration
    if let Some(expires_at) = bid.expires_at {
        if env::block_timestamp() > expires_at {
            // Auto-reject expired bid
            internal_reject_bid(contract, property_id, bid_id);
            panic!("Bid expired");
        }
    }
    
    // 4. Update bid status
    let bid = update_bid_in_list(bids, bid_id, |bid| {
        require!(bid.status == BidStatus::Pending);
        bid.status = BidStatus::Accepted;
        bid.updated_at = env::block_timestamp();
    });
    
    // 5. Transfer stablecoin to seller
    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));
    
    // 6. Update internal balance tracking
    let current_balance = contract.stable_coin_balances.get(&bid.stablecoin_token)?;
    contract.stable_coin_balances.insert(
        bid.stablecoin_token,
        current_balance - bid.amount
    );
    
    // 7. Callback to finalize
    promise.then(
        ShedaContract::ext(env::current_account_id())
            .accept_bid_callback(property_id, bid_id)
    )
}
```

**Callback Handler**:
```rust
pub fn accept_bid_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    unlock_bid(contract, property_id, bid_id);
    
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            // Transfer NFT
            contract.tokens.internal_transfer(
                &property.owner_id,
                &bid.bidder,
                &property_id.to_string(),
                None,
                None
            );
            
            // Update bid status
            bid.status = BidStatus::Completed;
            bid.escrow_release_tx = Some(format!("block:{}", env::block_height()));
            
            // Handle action-specific logic
            match bid.action {
                Action::Purchase => {
                    // Mark property as sold
                    property.sold = Some(Sold { /* ... */ });
                    property.is_for_sale = false;
                }
                Action::Lease => {
                    // Create lease record
                    let lease = Lease {
                        escrow_held: bid.amount,
                        escrow_token: bid.stablecoin_token,
                        // ... other fields
                    };
                    property.active_lease = Some(lease);
                }
            }
            
            // Refund all other pending bids
            refund_other_bids(contract, property_id, bid_id);
        }
        PromiseResult::Failed => {
            // Revert balance update
            revert_balance_update(contract, &bid);
            bid.status = BidStatus::Pending;
            panic!("Payment transfer failed");
        }
    }
}
```

**Key Features**:
- ✅ Atomic payment and NFT transfer
- ✅ Automatic refund of competing bids
- ✅ Rollback on payment failure
- ✅ Action-specific finalization (Purchase vs Lease)

---

### 3. BID ACCEPTANCE - PATH B: Escrow with Staged Release

**Entry Point**: `accept_bid_with_escrow(bid_id, property_id)`

**Use Case**: Complex transactions requiring document verification and buyer approval before payment release

**Process**:
```
Seller                    Sheda Contract                    Buyer
  |                            |                              |
  |--accept_bid_with_escrow--->|                              |
  |                      [Status: Accepted]                   |
  |                      [Escrow held]                        |
  |                      [Refund other bids]                  |
  |                            |                              |
  |--confirm_document_release->|                              |
  |                      [Status: DocsReleased]               |
  |                      [Attach document NFT]                |
  |                            |                              |
  |                            |<--confirm_document_receipt---|
  |                            |                              |
  |                      [Status: DocsConfirmed]              |
  |                      [Set timelock]                       |
  |                            |                              |
  |                      [Wait: escrow_release_delay_ns]      |
  |                            |                              |
  |                            |<--release_escrow-------------|
  |                            |                              |
  |                      [Transfer to seller]                 |
  |                      [Transfer NFT to buyer]              |
  |                      [Status: PaymentReleased]            |
  |<---payment received--------|                              |
```

**Implementation Details**:

**Step 1: Accept with Escrow Holding**
```rust
pub fn internal_accept_bid_with_escrow(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    // 1. Verify ownership and bid validity
    // 2. Update bid status to Accepted
    
    // 3. Reject and refund all other pending bids
    for other_bid in bids.iter_mut() {
        if other_bid.id == bid_id || other_bid.status != BidStatus::Pending {
            continue;
        }
        
        // Refund stablecoin
        ft_contract::ext(other_bid.stablecoin_token.clone())
            .ft_transfer(other_bid.bidder.clone(), U128(other_bid.amount));
        
        // Update balance
        let current_balance = contract.stable_coin_balances.get(&other_bid.stablecoin_token)?;
        contract.stable_coin_balances.insert(
            other_bid.stablecoin_token,
            current_balance - other_bid.amount
        );
        
        other_bid.status = BidStatus::Rejected;
    }
    
    // 4. For Lease: Create lease immediately with escrow held
    if matches!(bid.action, Action::Lease) {
        let lease = Lease {
            id: contract.lease_counter,
            property_id,
            tenant_id: bid.bidder.clone(),
            start_time: env::block_timestamp(),
            end_time: calculate_lease_end_time(property),
            active: true,
            escrow_held: bid.amount,
            escrow_token: bid.stablecoin_token.clone(),
            // ...
        };
        property.active_lease = Some(lease);
    }
    
    // NOTE: For Purchase, escrow remains held until release_escrow()
    true
}
```

**Step 2: Document Release (Seller)**
```rust
pub fn internal_confirm_document_release(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    document_token_id: String,
) -> bool {
    let property = contract.properties.get(&property_id)?;
    require!(property.owner_id == env::predecessor_account_id());
    
    update_bid_in_list(bids, bid_id, |bid| {
        require!(bid.status == BidStatus::Accepted);
        bid.status = BidStatus::DocsReleased;
        bid.document_token_id = Some(document_token_id);
        bid.updated_at = env::block_timestamp();
    });
    
    true
}
```

**Step 3: Document Receipt Confirmation (Buyer)**
```rust
pub fn internal_confirm_document_receipt(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> bool {
    update_bid_in_list(bids, bid_id, |bid| {
        require!(bid.status == BidStatus::DocsReleased);
        require!(bid.bidder == env::predecessor_account_id());
        
        bid.status = BidStatus::DocsConfirmed;
        bid.updated_at = env::block_timestamp();
        
        // Set timelock for escrow release
        bid.escrow_release_after = Some(
            env::block_timestamp() + contract.escrow_release_delay_ns
        );
    });
    
    true
}
```

**Step 4: Escrow Release (Buyer)**
```rust
pub fn internal_release_escrow(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
) -> Promise {
    lock_bid(contract, property_id, bid_id);
    
    let bid = get_bid_from_list(bids, bid_id);
    require!(bid.bidder == env::predecessor_account_id());
    require!(bid.status == BidStatus::DocsConfirmed);
    
    // Verify timelock has passed
    if let Some(unlock_at) = bid.escrow_release_after {
        require!(env::block_timestamp() >= unlock_at, "Timelock not reached");
    }
    
    // Transfer payment to seller
    let promise = ft_contract::ext(bid.stablecoin_token.clone())
        .ft_transfer(property.owner_id.clone(), U128(bid.amount));
    
    // Update balance tracking
    let current_balance = contract.stable_coin_balances.get(&bid.stablecoin_token)?;
    contract.stable_coin_balances.insert(
        bid.stablecoin_token,
        current_balance - bid.amount
    );
    
    promise.then(
        ShedaContract::ext(env::current_account_id())
            .release_escrow_callback(property_id, bid_id)
    )
}
```

**Callback: Finalize Release**
```rust
pub fn release_escrow_callback(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    unlock_bid(contract, property_id, bid_id);
    
    match env::promise_result(0) {
        PromiseResult::Successful(_) => {
            // Update bid status
            bid.status = BidStatus::PaymentReleased;
            bid.escrow_release_tx = Some(format!("block:{}", env::block_height()));
            
            // Transfer NFT to buyer
            match bid.action {
                Action::Purchase => {
                    contract.tokens.internal_transfer(
                        &property.owner_id,
                        &bid.bidder,
                        &property_id.to_string(),
                        None,
                        None
                    );
                    
                    // Mark as sold
                    property.sold = Some(Sold { /* ... */ });
                    property.is_for_sale = false;
                }
                Action::Lease => {
                    // Transfer NFT temporarily
                    contract.tokens.internal_transfer(
                        &property.owner_id,
                        &bid.bidder,
                        &property_id.to_string(),
                        None,
                        None
                    );
                    
                    // Lease already created in accept_bid_with_escrow
                    // Escrow held in lease.escrow_held
                }
            }
        }
        PromiseResult::Failed => {
            // Revert balance and status
            revert_escrow_release(contract, property_id, bid_id);
            panic!("Escrow release failed");
        }
    }
}
```

**Key Features**:
- ✅ Staged verification process
- ✅ Document NFT reference tracking
- ✅ Timelock protection (`escrow_release_delay_ns` = 24 hours default)
- ✅ Buyer-controlled final release
- ✅ Escrow held separately for leases

---

## Comparison: Path A vs Path B

| Feature | `accept_bid` (Path A) | `accept_bid_with_escrow` (Path B) |
|---------|----------------------|----------------------------------|
| **Steps** | 2 (Accept → Complete) | 5+ (Accept → DocsRelease → DocsConfirm → Wait → Release) |
| **Payment Timing** | Immediate on acceptance | After buyer approval + timelock |
| **NFT Transfer** | Immediate | After escrow release |
| **Use Case** | Simple/trusted transactions | Complex/high-value transactions |
| **Dispute Window** | None | During DocsReleased/DocsConfirmed states |
| **Seller Control** | Full (auto-complete) | Partial (buyer must release) |
| **Buyer Protection** | Minimal | High (document review + timelock) |
| **Gas Cost** | Lower | Higher (multiple transactions) |

---

## Additional Mechanisms

### Bid Cancellation (Buyer)
```rust
pub fn internal_cancel_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let bid = get_bid_from_list(bids, bid_id);
    require!(bid.status == BidStatus::Pending);
    require!(bid.bidder == env::predecessor_account_id());
    
    // Ensure property not already transferred
    let property = contract.properties.get(&property_id)?;
    if let Some(sold) = &property.sold {
        require!(sold.buyer_id != bid.bidder, "Already sold to you");
    }
    
    // Refund stablecoin
    ft_contract::ext(bid.stablecoin_token.clone())
        .ft_transfer(bid.bidder.clone(), U128(bid.amount));
    
    // Update balance
    update_balance(contract, &bid.stablecoin_token, -bid.amount);
    
    // Update status
    bid.status = BidStatus::Cancelled;
}
```

### Bid Rejection (Seller)
```rust
pub fn internal_reject_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let property = contract.properties.get(&property_id)?;
    require!(property.owner_id == env::predecessor_account_id());
    
    let bid = get_bid_from_list(bids, bid_id);
    require!(bid.status == BidStatus::Pending);
    
    // Refund and update status (similar to cancel)
    refund_bid(contract, &bid);
    bid.status = BidStatus::Rejected;
}
```

### Dispute Raising
```rust
pub fn internal_raise_bid_dispute(
    contract: &mut ShedaContract,
    property_id: u64,
    bid_id: u64,
    reason: String,
) -> bool {
    let property = contract.properties.get(&property_id)?;
    let bid = get_bid_from_list(bids, bid_id);
    
    let caller = env::predecessor_account_id();
    require!(
        caller == bid.bidder || caller == property.owner_id,
        "Only buyer or seller can raise dispute"
    );
    
    // Can only dispute during active transaction states
    match bid.status {
        BidStatus::Accepted | BidStatus::DocsReleased | BidStatus::DocsConfirmed => {}
        _ => panic!("Bid not in disputable state"),
    }
    
    bid.status = BidStatus::Disputed;
    bid.dispute_reason = Some(reason);
    
    // Note: Resolution mechanism not fully implemented
    // Likely requires oracle_account_id intervention
    true
}
```

---

## Escrow Balance Tracking

The contract maintains internal accounting for all escrowed funds:

```rust
pub stable_coin_balances: IterableMap<AccountId, u128>
```

**Operations**:
- **Deposit** (ft_on_transfer): `balance += bid.amount`
- **Release** (accept_bid, release_escrow): `balance -= bid.amount`
- **Refund** (reject_bid, cancel_bid): `balance -= bid.amount`

**Invariant**:
```
Sum of all pending/accepted bid amounts == Sum of stable_coin_balances
```

**Safety**:
- Uses checked arithmetic (`checked_add_u128`, `checked_sub_u128`)
- Panic on overflow/underflow
- Balance updated before external calls (Checks-Effects-Interactions)

---

## Security Features

### Reentrancy Protection
```rust
pub reentrancy_locks: IterableSet<String>

// Lock bid during critical operations
fn lock_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let key = format!("bid:{}:{}", property_id, bid_id);
    require!(!contract.reentrancy_locks.contains(&key), "Bid locked");
    contract.reentrancy_locks.insert(key);
}

// Unlock after completion/failure
fn unlock_bid(contract: &mut ShedaContract, property_id: u64, bid_id: u64) {
    let key = format!("bid:{}:{}", property_id, bid_id);
    contract.reentrancy_locks.remove(&key);
}
```

### Mock Mode (Testing)
```rust
pub mock_transfers_enabled: bool

// Skip external FT transfers in test mode
if contract.mock_transfers_enabled {
    unlock_bid(contract, property_id, bid_id);
    finalize_accepted_bid(contract, property_id, bid_id);
    return Promise::new(env::current_account_id()).transfer(NearToken::from_yoctonear(0));
}
```

### Timelock Configuration
```rust
// Configurable delays
pub bid_expiry_ns: u64,                  // Default: 7 days
pub escrow_release_delay_ns: u64,        // Default: 24 hours
pub lost_bid_claim_delay_ns: u64,        // Default: 24 hours
```

---

## Lease-Specific Escrow

For lease transactions, escrow is handled differently:

```rust
pub struct Lease {
    pub id: u64,
    pub property_id: u64,
    pub tenant_id: AccountId,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub active: bool,
    pub dispute_status: DisputeStatus,
    pub dispute: Option<DisputeInfo>,
    pub escrow_held: u128,           // Held for damages/rent
    pub escrow_token: AccountId,     // Which stablecoin
}
```

**Escrow Holding**:
- Bid amount committed to `lease.escrow_held`
- Remains in contract until:
  - Lease expires successfully → Returned to tenant
  - Dispute raised → Oracle/admin resolution
  - Property damage → Partial/full payment to owner

**Current Implementation**:
- Escrow tracking in `Lease` struct separate from `Bid`
- Not automatically released on lease end (requires manual claim)
- Admin function exists for damage payout: `payout_damage_bond()`

---

## Events Emitted

### Bid Lifecycle
```rust
// Bid placed
BidPlacedEvent {
    token_id: property_id,
    bidder_id: AccountId,
    amount: u128,
    created_at: Timestamp,
}

// Bid approved
BidApprovedEvent {
    token_id: property_id,
    bidder_id: AccountId,
    seller_id: AccountId,
    amount: u128,
}

// Bid rejected/cancelled
BidRejectedEvent / BidCancelledEvent {
    token_id: property_id,
    bid_id: u64,
    bidder_id: AccountId,
    amount: u128,
}

// Deal finalized
DealFinalizedEvent {
    token_id: property_id,
    buyer_id: AccountId,
    seller_id: AccountId,
    amount: u128,
    lease_duration_nanos: u64,
}
```

---

## Configuration Variables

```rust
// Contract initialization
pub fn new(media_url: String, supported_stablecoins: Vec<AccountId>) -> Self {
    // ...
    bid_expiry_ns: 7 * 24 * 60 * 60 * 1_000_000_000,        // 7 days
    escrow_release_delay_ns: 24 * 60 * 60 * 1_000_000_000,  // 24 hours
    lost_bid_claim_delay_ns: 24 * 60 * 60 * 1_000_000_000,  // 24 hours
}
```

**Adjustable**:
- Bid expiration time
- Escrow release timelock
- Lost bid claim window
- Accepted stablecoin list

---

## Current Limitations & Design Choices

### ✅ Implemented
- Immediate escrow commitment on bid placement
- Dual-path acceptance (immediate vs staged)
- Timelock protection for buyer review
- Multi-stablecoin support
- Reentrancy guards
- Automatic bid expiration
- Document NFT tracking
- Lease escrow separation

### ⚠️ Partially Implemented
- Dispute resolution (flag exists, resolution mechanism incomplete)
- Automatic bid refunds on expiration (requires manual rejection)
- Lost bid claiming (function exists but not integrated)

### ❌ Not Implemented
- Automatic escrow release (always requires buyer action)
- Milestone-based payments
- Partial payments/installments
- Automated lease escrow return
- Multi-signature escrow
- Insurance/bonding mechanisms
- Price validation (any bid amount accepted)
- Bid ranking/ordering by amount

---

## State Machine Diagram

```
                    [Buyer places bid via ft_on_transfer]
                                    |
                                    v
                            BidStatus::Pending
                        (Escrow committed in contract)
                                    |
                    +---------------+---------------+
                    |               |               |
                    v               v               v
            Cancelled        Rejected          Accepted
         (buyer action)   (seller action)   (seller action)
                |               |               |
                |               |               +
                |               |               |
            [Refund]        [Refund]            +--→ Path A (accept_bid)
                                                |       |
                                                |       v
                                                |   [Pay seller]
                                                |   [Transfer NFT]
                                                |       |
                                                |       v
                                                |   Completed
                                                |
                                                +--→ Path B (accept_bid_with_escrow)
                                                        |
                                                        v
                                                  DocsReleased
                                                  (seller action)
                                                        |
                                                        v
                                                  DocsConfirmed
                                                  (buyer action)
                                                  [Timelock set]
                                                        |
                                                        v
                                                [Wait: escrow_release_delay_ns]
                                                        |
                                                        v
                                                  release_escrow()
                                                  (buyer action)
                                                        |
                                                        v
                                                  PaymentReleased
                                                  [Pay seller]
                                                  [Transfer NFT]
                                                        |
                                                        v
                                                    Completed

                    [Disputed state can be entered from Accepted/DocsReleased/DocsConfirmed]
```

---

## Code References

### Key Files
- **Bid Placement**: [src/lib.rs](src/lib.rs#L723-L816) - `ft_on_transfer()`
- **Path A Flow**: [src/internal.rs](src/internal.rs#L160-L282) - `internal_accept_bid()` + callback
- **Path B Flow**: [src/internal.rs](src/internal.rs#L428-L560) - `internal_accept_bid_with_escrow()`
- **Escrow Release**: [src/internal.rs](src/internal.rs#L767-L889) - `internal_release_escrow()` + callback
- **Models**: [src/models.rs](src/models.rs#L65-L115) - Bid, BidStatus, Lease structures

### Public API Methods
```rust
// Bid management
pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128
pub fn accept_bid(&mut self, bid_id: u64, property_id: u64) -> Promise
pub fn accept_bid_with_escrow(&mut self, bid_id: u64, property_id: u64) -> bool
pub fn reject_bid(&mut self, bid_id: u64, property_id: u64)
pub fn cancel_bid(&mut self, bid_id: u64, property_id: u64)

// Path B escrow workflow
pub fn confirm_document_release(&mut self, bid_id: u64, property_id: u64, document_token_id: String) -> bool
pub fn confirm_document_receipt(&mut self, bid_id: u64, property_id: u64) -> bool
pub fn release_escrow(&mut self, bid_id: u64, property_id: u64) -> Promise

// Dispute and completion
pub fn raise_dispute(&mut self, bid_id: u64, property_id: u64, reason: String) -> bool
pub fn complete_transaction(&mut self, bid_id: u64, property_id: u64) -> bool
```

---

## Summary

The current implementation uses an **immediate escrow commitment** model where buyers lock funds when placing bids. Sellers can choose between:

1. **Fast path** (`accept_bid`): Instant payment and ownership transfer - suitable for trusted/simple transactions
2. **Secure path** (`accept_bid_with_escrow`): Multi-stage verification with document exchange and buyer-controlled release - suitable for high-value/complex transactions

Both paths maintain funds in contract escrow with internal balance tracking, use reentrancy protection, support multiple stablecoins, and emit comprehensive events for off-chain monitoring.

---

**Last Updated**: 27 February 2026  
**Contract Version**: 2  
**Author**: Sheda Protocol Team
