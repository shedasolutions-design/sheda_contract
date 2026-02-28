# Sheda Contract Integration Guide

Comprehensive documentation for integrating with the Sheda smart contract - a property sales & lease escrow protocol with document management on NEAR.

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Core Entities](#core-entities)
3. [Purchase Flow (Complete Workflow)](#purchase-flow-complete-workflow)
4. [Lease Flow](#lease-flow)
5. [Document Release & Transfer](#document-release--transfer)
6. [Payment Release & Escrow](#payment-release--escrow)
7. [Dispute System](#dispute-system)
8. [Integration Examples](#integration-examples)
9. [Error Handling](#error-handling)
10. [Events & Monitoring](#events--monitoring)

---

## System Overview

The Sheda contract manages:
- **Properties**: Digital records of real property (land, buildings, etc.)
- **Bids**: Offers to purchase or lease properties with escrow backing
- **Leases**: Time-bound tenant arrangements with damage escrow
- **Documents**: NFTs representing transaction documents (deeds, rental agreements)
- **Escrow**: Multi-token stablecoin balance management for payment security

### Key Design Principles

1. **Two-path acceptance**: Bids can be accepted immediately (`accept_bid`) OR with escrow staging (`accept_bid_with_escrow`)
2. **Document-first**: Property documents are minted post-acceptance, not pre-stored
3. **Multi-stablecoin**: Contract accepts configurable list of accepted tokens (USDC, USDT, etc.)
4. **State machine**: Bids progress through defined status transitions with guards
5. **Reentrancy protection**: All critical state mutations protected with locks

### Supported Token Types

Address configured at deployment. Default examples:
- `v2-ref-finance.testnet` (Testnet)
- `usdc.testnet` (USDC on Testnet)

---

## Core Entities

### 1. Property

```javascript
{
  id: u64,                           // Unique property ID (auto-incremented)
  owner_id: string (AccountId),      // Current property owner
  description: string,                // Property details (address, features)
  metadata_uri: string,               // IPFS/external reference
  is_for_sale: boolean,               // Available for purchase
  price: u128,                        // Sale price in stablecoin atomic units
  lease_duration_months: u64 | null,  // If leasable: tenure length
  damage_escrow: u128,                // Escrow amount for lease damage
  active_lease: Lease | null,         // Current active lease (if any)
  timestamp: u64,                     // Creation time (nanoseconds)
  sold: Sold | null,                  // Historical sale record (if previously sold)
}
```

**State Transitions:**
- `is_for_sale = true` → accepts bids
- `is_for_sale = false` → rejects bids (can be re-enabled)
- `active_lease != null` → property is leased to tenant (can't accept bids)

### 2. Bid

```javascript
{
  id: u64,                              // Unique bid ID (auto-incremented)
  bidder: string (AccountId),           // Entity making the offer
  property_id: u64,                     // Target property
  amount: u128,                         // Offer in stablecoin atomic units
  created_at: u64,                      // Timestamp (nanoseconds)
  updated_at: u64,                      // Last state change time
  status: BidStatus,                    // See state machine below
  document_token_id: string | null,     // NFT ID after mint (format: "doc:PROP_ID:BID_ID")
  document_image_uri: string | null,    // IPFS/URL to document source image (v3)
  document_description: string | null,  // Document metadata (v3)
  escrow_release_tx: string | null,     // Hash of payment release tx
  dispute_reason: string | null,        // Dispute claim reason (if disputed)
  expires_at: u64 | null,               // Bid expiry timestamp (for accept_bid only)
  escrow_release_after: u64 | null,     // Escrow hold duration (for escrow bids)
  action: Action,                       // "Purchase" or "Lease"
  stablecoin_token: string (AccountId), // Token paid in (e.g., USDC address)
}
```

#### BidStatus State Machine

```
Pending
  ├─→ Accepted (via accept_bid)
  │    └─→ DocsReleased (seller mints documents)
  │         └─→ DocsConfirmed (buyer acknowledges docs)
  │              └─→ PaymentReleased (payment moves to owner)
  │                   └─→ Completed (final state)
  │
  ├─→ Accepted (via accept_bid_with_escrow)
  │    └─→ [held in escrow, time passes]
  │         └─→ PaymentReleased (automatic after delay)
  │              └─→ Completed
  │
  ├─→ Rejected (owner explicitly rejects)
  │    └─→ Cancelled (bidder claims refund)
  │
  └─→ Disputed (raised by either party during Accepted)
       └─→ [oracle resolves]
            └─→ Completed or Cancelled
```

**Status Descriptions:**
- **Pending**: Awaiting owner decision
- **Accepted**: Owner accepted; seller prep for handoff
- **DocsReleased**: Seller minted & transferred NFT document to buyer
- **DocsConfirmed**: Buyer acknowledged receipt of document NFT
- **PaymentReleased**: Payment moved from escrow to seller
- **Completed**: All obligations fulfilled
- **Rejected**: Owner declined; bid can be claimed/cancelled
- **Cancelled**: Bidder withdrew or claimed refund
- **Disputed**: Conflict raised; awaiting oracle resolution

### 3. Lease

```javascript
{
  id: u64,                         // Unique lease ID
  property_id: u64,                // Leased property
  tenant_id: string (AccountId),   // Tenant account
  start_time: u64,                 // Lease start (nanoseconds)
  end_time: u64,                   // Lease expiry (nanoseconds)
  active: boolean,                 // Currently in force
  dispute_status: DisputeStatus,   // Conflict state
  dispute: DisputeInfo | null,     // Detailed dispute (if any)
  escrow_held: u128,               // Damage deposit held
  escrow_token: string (AccountId), // Token of escrow
}
```

---

## Purchase Flow (Complete Workflow)

### Scenario: Alice (buyer) purchases Bob's (seller's) property

#### Phase 1: Property Setup (Bob)

Bob lists his property:

```bash
# Bob calls place_property
near call shedasolution.testnet place_property '{
  "description": "2BR house at 123 Main St, City, State",
  "metadata_uri": "ipfs://QmXXX...",
  "is_for_sale": true,
  "price": "500000000000000000000000",  # 500 USDC (6 decimals)
  "lease_duration_months": null,
  "damage_escrow": null,
  "stablecoin_token": "usdc.testnet"
}' --account_id bob.testnet
```

**Contract Response:**
- Property ID auto-generated (e.g., `1`)
- Status: `is_for_sale = true`
- Stored in `state.properties` map
- Event emitted: `PropertyMintedEvent`

#### Phase 2: Bid Placement (Alice)

Alice approves escrow payment and places bid:

```bash
# Step 1: Alice approves USDC contract to spend on behalf of Sheda contract
near call usdc.testnet ft_approve '{
  "receiver_id": "shedasolution.testnet",
  "amount": "500000000000000000000000"
}' --account_id alice.testnet --amount 1

# Step 2: Alice places bid on property
near call shedasolution.testnet place_bid '{
  "property_id": 1,
  "amount": "500000000000000000000000",
  "action": "Purchase",
  "stablecoin_token": "usdc.testnet"
}' --account_id alice.testnet
```

**Internally (contract):**
1. Transfers USDC from Alice → escrow contract
2. Creates new Bid record:
   - `id = 1` (auto-increment)
   - `status = Pending`
   - `bidder = alice.testnet`
   - `property_id = 1`
   - `amount = 500000000000000000000000`
3. Stores in `state.bids[property_id] = [Bid]`
4. Emits: `BidPlacedEvent`

**Bid State:**
```
Status: Pending
Owner: bob.testnet
Bidder: alice.testnet
Amount: 500 USDC (in escrow)
```

#### Phase 3: Bid Acceptance (Bob)

Bob accepts Alice's bid (immediate payment path):

```bash
# Bob accepts bid (immediate release)
near call shedasolution.testnet accept_bid '{
  "property_id": 1,
  "bid_id": 1
}' --account_id bob.testnet --amount 1
```

**Internally:**
1. Validates:
   - Bob is property owner ✓
   - Bid status == Pending ✓
   - Property is_for_sale == true ✓
2. Sets: `bid.status = Accepted`
3. Sets: `bid.updated_at = now()`
4. Updates: `property.is_for_sale = false` (prevent other bids)
5. Stores updated bid & property

**Bid State:**
```
Status: Accepted
Updated: [timestamp]
Next step: Seller (Bob) mints documents
```

#### Phase 4: Document Minting & Transfer (Bob)

Bob prepares and uploads deed document image, then mints NFT:

```bash
# Step 1: Bob uploads document image to IPFS or file server
# Returns: ipfs://QmDocument... or https://example.com/deed.jpg

# Step 2: Bob mints document as NFT and transfers to Alice
near call shedasolution.testnet confirm_document_release '{
  "property_id": 1,
  "bid_id": 1,
  "document_image_uri": "ipfs://QmDocument123...",
  "document_description": "Property Deed - 123 Main St"
}' --account_id bob.testnet --amount 1
```

**Internally:**
1. Validates:
   - Bob is property owner ✓
   - Bid status == Accepted ✓
   - No existing document token ✓ (prevent remint)
   - URI & description non-empty ✓
2. Generates token ID: `"doc:1:1"` (format: `doc:{property_id}:{bid_id}`)
3. Creates TokenMetadata:
   - Title: `"Purchase - Bid #1"`
   - Description: `"Property Deed - 123 Main St"`
   - Media: `ipfs://QmDocument123...`
4. **Mints** NFT to Bob's account
5. **Transfers** NFT to Alice (`alice.testnet`)
6. Updates bid:
   - `status = DocsReleased`
   - `document_token_id = "doc:1:1"`
   - `document_image_uri = "ipfs://QmDocument123..."`
   - `document_description = "Property Deed - 123 Main St"`
7. Emits: `DocumentReleasedEvent`

**Alice's View:**
- Receives NFT in her NEAR account
- Can view NFT metadata (deed image + description)
- Must acknowledge receipt to proceed

#### Phase 5: Document Acknowledgment (Alice)

Alice confirms receipt of deed NFT:

```bash
# Alice confirms she received the document
near call shedasolution.testnet confirm_document_receipt '{
  "property_id": 1,
  "bid_id": 1
}' --account_id alice.testnet
```

**Internally:**
1. Validates:
   - Alice is bid.bidder ✓
   - Bid status == DocsReleased ✓
   - Document token exists ✓
2. Sets: `bid.status = DocsConfirmed`
3. Emits: `DocumentConfirmedEvent`

**Bid State:**
```
Status: DocsConfirmed
Document NFT: Owned by alice.testnet
Next: Payment release to seller
```

#### Phase 6: Payment Release (Bob or Alice)

Either party triggers final payment transfer:

```bash
# Either Bob or Alice can call this
near call shedasolution.testnet release_payment '{
  "property_id": 1,
  "bid_id": 1
}' --account_id bob.testnet --amount 1
```

**Internally:**
1. Validates:
   - Bid status == DocsConfirmed ✓
   - Caller is owner OR bidder ✓
2. Transfers 500 USDC from escrow → Bob's account
3. Updates bid: `status = PaymentReleased`
4. Emits: `PaymentReleasedEvent`

**Fund Distribution:**
```
Escrow: 500 USDC →  Bob's account (seller)
```

#### Phase 7: Completion (Any)

Either party finalizes the transaction:

```bash
# Completes the bid
near call shedasolution.testnet complete_bid '{
  "property_id": 1,
  "bid_id": 1
}' --account_id alice.testnet
```

**Internally:**
1. Validates: `bid.status == PaymentReleased`
2. Sets: `bid.status = Completed`
3. Updates: `property.sold = Sold { buyer_id: alice.testnet, ... }`
4. Clears: `property.is_for_sale = false`
5. Emits: `BidCompletedEvent`

**Final State:**
```
Purchase complete!
- Alice: Owns property + holds document NFT
- Bob: Received 500 USDC
- Property: No longer for sale
- Bid: Complete
```

---

### Alternative: Escrow-Backed Acceptance

If Bob wants a 24-hour hold before payment release:

```bash
# Bob accepts with escrow delay
near call shedasolution.testnet accept_bid_with_escrow '{
  "property_id": 1,
  "bid_id": 1
}' --account_id bob.testnet --amount 1
```

**Flow Difference:**
1. Bid status: `Accepted` (same as immediate)
2. Sets internal timer: `escrow_release_after = now() + 24 hours`
3. Alice and Bob still mint/confirm documents normally
4. **After 24 hours**, payment automatically releases (can be triggered by anyone)
5. Completion proceeds as normal

---

## Lease Flow

### Scenario: Charlie (new owner) leases Bob's property to Diana (tenant)

#### Phase 1: Property Listed for Lease

Charlie updates property with lease terms:

```bash
# Charlie already owns property after purchase
# Now lists for lease (optional 2BR house example)

near call shedasolution.testnet place_property '{
  "description": "2BR house - Available for lease",
  "metadata_uri": "ipfs://...",
  "is_for_sale": false,
  "price": null,
  "lease_duration_months": 12,       # 1-year lease
  "damage_escrow": "50000000000000000000000",  # 50 USDC deposit
  "stablecoin_token": "usdc.testnet"
}' --account_id charlie.testnet
```

**Property State:**
```
Available for lease: Yes
Lease duration: 12 months
Damage escrow: 50 USDC
```

#### Phase 2: Lease Bid Placement (Diana)

Diana approves escrow and places lease bid:

```bash
# Step 1: Diana approves USDC for escrow + damage deposit
near call usdc.testnet ft_approve '{
  "receiver_id": "shedasolution.testnet",
  "amount": "50000000000000000000000"  # Damage escrow only (no rent in initial bid)
}' --account_id diana.testnet --amount 1

# Step 2: Diana places lease bid
near call shedasolution.testnet place_bid '{
  "property_id": 2,
  "amount": "50000000000000000000000",  # Damage deposit held
  "action": "Lease",
  "stablecoin_token": "usdc.testnet"
}' --account_id diana.testnet
```

**Bid Created:**
```
Type: Lease
Bidder: diana.testnet
Amount: 50 USDC (damage escrow)
Status: Pending
```

#### Phase 3: Lease Acceptance (Charlie)

Charlie accepts Diana as tenant:

```bash
near call shedasolution.testnet accept_bid '{
  "property_id": 2,
  "bid_id": 2
}' --account_id charlie.testnet --amount 1
```

**Internally:**
1. Creates active Lease:
   - `start_time = now()`
   - `end_time = now() + 12 months`
   - `tenant_id = diana.testnet`
   - `escrow_held = 50 USDC`
2. Stores lease in `property.active_lease`
3. Bid status: `Accepted`
4. Property: `active_lease != null` (blocks new bids)

#### Phase 4: Lease Operations (During Lease Term)

While lease is active:

**Monthly rent payment** (off-chain or via separate mechanism):
```bash
# Diana pays rent directly to Charlie or via escrow
# This is typically handled outside contract or in separate payment flow
```

**During-lease dispute** (if Diana damages property):
```bash
# Charlie raises dispute over damages
near call shedasolution.testnet raise_dispute '{
  "property_id": 2,
  "lease_id": 2,
  "reason": "Tenant damaged living room wall - repair cost ~$2000"
}' --account_id charlie.testnet --amount 1
```

**Internally:**
1. Creates DisputeInfo record
2. Sets `lease.dispute_status = Raised`
3. Can vote or use oracle to resolve

#### Phase 5: Lease Completion

After 12 months, lease expires:

```bash
# Either party can call to finalize
near call shedasolution.testnet complete_lease '{
  "property_id": 2,
  "lease_id": 2
}' --account_id charlie.testnet
```

**Internally:**
1. Validates: `lease.end_time <= now()` (expired)
2. If no active disputes:
   - Returns damage escrow to Diana
3. If disputes resolved:
   - Applies damage deduction, returns remainder
4. Sets: `property.active_lease = null`
5. Property is available for new bids

---

## Document Release & Transfer

### Full Document Workflow

Documents are **NFTs** (NEP-171 standard) representing transaction proof:

#### Sequence Diagram

```
Seller              Contract            Buyer
  |                   |                   |
  |-- mint doc ------>|                   |
  |                   |                   |
  |<--- transfer -----+---- transfer ---->|
  |                   |                   |
  |                (Buyer now holds NFT)  |
  |                   |                   |
  |                   |<-- confirm receipt|
  |                   |                   |
```

#### Step-by-step: Minting & Transferring

**1. Seller prepares document image**

```javascript
// Off-chain: Seller uploads deed/agreement image to IPFS
// Example: ipfs://QmXXXX...
const imageUri = "ipfs://QmDeeds123abc";
const description = "Property Deed - 2BR House at 123 Main St";
```

**2. Seller calls mint-and-transfer**

```bash
near call shedasolution.testnet confirm_document_release '{
  "property_id": 1,
  "bid_id": 1,
  "document_image_uri": "ipfs://QmDeeds123abc",
  "document_description": "Property Deed - 2BR House at 123 Main St"
}' --account_id seller.testnet --amount 1
```

**3. Internal minting process**

```rust
// Generated token ID (deterministic, prevents duplicates)
let token_id = format!("doc:{}:{}", property_id, bid_id);
// Example: "doc:1:1"

// Token metadata structure
let metadata = TokenMetadata {
    title: Some("Purchase - Bid #1"),
    description: Some("Property Deed - 2BR House at 123 Main St"),
    media: Some("ipfs://QmDeeds123abc"),
    media_hash: None,
    issued_at: Some(current_timestamp),
    expires_at: None,
    starts_at: Some(current_timestamp),
    updated_at: Some(current_timestamp),
    extra: None,
    reference: None,
    reference_hash: None,
};

// Mint: Creates NFT in Seller's account
internal_mint(seller_id, token_id, metadata);

// Transfer: Moves NFT to Buyer's account
internal_transfer(seller_id, buyer_id, token_id, Some(memo));
```

**4. Buyer receives & acknowledges**

```bash
# Buyer confirms receipt
near call shedasolution.testnet confirm_document_receipt '{
  "property_id": 1,
  "bid_id": 1
}' --account_id buyer.testnet
```

#### Retrieving Documents

**Query bid to find document token ID:**

```bash
# Off-chain: Call view function
near view shedasolution.testnet get_bid_view '{
  "property_id": 1,
  "bid_id": 1
}'

# Response:
{
  "id": 1,
  "bidder": "alice.testnet",
  "property_id": 1,
  "amount": "500000000000000000000000",
  "status": "DocsConfirmed",
  "document_token_id": "doc:1:1",
  "document_image_uri": "ipfs://QmDeeds123abc",
  "document_description": "Property Deed - 2BR House at 123 Main St",
  ...
}
```

**Query NFT from NEAR contract (standard NEP-171):**

```bash
# View if buyer owns the document
near view shedasolution.testnet nft_tokens_for_owner '{
  "account_id": "alice.testnet"
}'

# Response includes NFT "doc:1:1" with metadata
```

---

## Payment Release & Escrow

### Escrow System Architecture

**Purpose**: Hold funds securely until both parties fulfill obligations.

#### Fund Flow

```
1. Place Bid
   Buyer's Account (USDC) --ft_transfer--> Escrow Pool
                                               ↓
                                         state.stable_coin_balances

2. Accept Bid
   [Funds held in escrow during transaction]

3. Confirm Documents
   [Conditions met to release]

4. Release Payment
   Escrow Pool --ft_transfer--> Seller's Account
```

#### Two Payment Paths

##### Path 1: Immediate Release (`accept_bid`)

```bash
# Owner accepts immediately
near call shedasolution.testnet accept_bid '{
  "property_id": 1,
  "bid_id": 1
}' --account_id owner.testnet --amount 1

# Timeline:
# T+0:   Bid Accepted
# T+N:   Documents released (seller mints)
# T+M:   Buyer confirms documents
# T+K:   Payment released (can be immediate after docs confirmed)
#        → No delay, funds go to seller immediately
```

**Suits:** Trust-based transactions, known parties, instant handoff.

##### Path 2: Escrow Hold (`accept_bid_with_escrow`)

```bash
# Owner accepts WITH time delay
near call shedasolution.testnet accept_bid_with_escrow '{
  "property_id": 1,
  "bid_id": 1
}' --account_id owner.testnet --amount 1

# Timeline:
# T+0:   Bid Accepted (escrow_release_after set to now + 24 hours)
# T+N:   Documents released
# T+M:   Buyer confirms documents
# T+24h: Payment automatically releasable (or explicitly triggered)
#        → Funds go to seller after hold period
```

**Suits:** Larger transactions, risk mitigation, discovery period.

#### Configuration (Admin)

```bash
# Admin sets escrow delays for contract-wide defaults
near call shedasolution.testnet configure_contract_parameters '{
  "escrow_release_delay_ns": 86400000000000,  # 24 hours in nanoseconds
  "bid_expiry_ns": 604800000000000,           # 7 days
  "lost_bid_claim_delay_ns": 86400000000000   # 24 hours
}' --account_id admin.testnet --amount 1
```

---

## Dispute System

### When Disputes Arise

Disputes can be raised during:
- **Active lease** (damage claims, tenant violations)
- **Active bid** (disagreement on acceptance, document authenticity)

### Filing a Dispute

```bash
# Owner disputes during active lease
near call shedasolution.testnet raise_dispute '{
  "property_id": 2,
  "lease_id": 2,
  "reason": "Tenant caused water damage to kitchen - estimated repair $3000"
}' --account_id property_owner.testnet --amount 1
```

**Internally:**
1. Creates DisputeInfo:
   - `raised_by = property_owner.testnet`
   - `reason = "..."`
   - `dispute_status = Raised`
2. Stores in `lease.dispute`
3. Emits: `DisputeRaisedEvent`

### Resolution Paths

#### Path 1: Manual Agreement

```bash
# Parties agree off-chain, owner confirms resolution
near call shedasolution.testnet resolve_dispute '{
  "property_id": 2,
  "lease_id": 2,
  "resolution_notes": "Tenant paid $1500; Remainder refunded"
}' --account_id property_owner.testnet
```

**Internally:**
1. Sets: `dispute.resolved_by = owner`
2. Sets: `dispute.resolved_at = now()`
3. Updates: `dispute_status = Resolved`
4. Refunds escrow minus deduction
5. Emits: `DisputeResolvedEvent`

#### Path 2: Oracle Resolution (Automated)

```bash
# Owner requests oracle evaluation
near call shedasolution.testnet request_oracle_evaluation '{
  "property_id": 2,
  "lease_id": 2
}' --account_id property_owner.testnet --amount 0.01
```

**Oracle Process:**
1. Contract sends request to oracle account
2. Oracle evaluates evidence (off-chain)
3. Oracle calls contract with decision
4. Escrow adjusted and paid out accordingly

**Example Oracle Result:**
```bash
# Oracle calls contract
near call shedasolution.testnet resolve_dispute_with_oracle '{
  "property_id": 2,
  "lease_id": 2,
  "winner": "Tenant",  # Tenant not at fault
  "deduction_amount": "0"
}' --account_id oracle.testnet --amount 1
```

---

## Integration Examples

### Example 1: Complete Purchase Workflow (Frontend)

```javascript
// Using near-api-js

const CONTRACT_ID = "shedasolution.testnet";
const USDC_TOKEN = "usdc.testnet";

class ShedaIntegration {
  constructor(wallet) {
    this.wallet = wallet;
    this.contract = new Contract(
      wallet.account(),
      CONTRACT_ID,
      {
        viewMethods: [
          "get_property",
          "get_bid_view",
          "nft_metadata",
        ],
        changeMethods: [
          "place_bid",
          "accept_bid",
          "confirm_document_release",
          "confirm_document_receipt",
          "release_payment",
          "complete_bid",
        ],
      }
    );
  }

  // Step 1: Buyer approves funds
  async approveFunds(amount) {
    const tokenContract = new Contract(
      this.wallet.account(),
      USDC_TOKEN,
      {
        changeMethods: ["ft_approve"],
      }
    );

    await tokenContract.ft_approve({
      receiver_id: CONTRACT_ID,
      amount: amount, // In atomic units (e.g., "500000000000000000000000" for 500 USDC)
    });
  }

  // Step 2: Buyer places bid
  async placeBid(propertyId, amount, action = "Purchase") {
    const result = await this.contract.place_bid({
      property_id: propertyId,
      amount: amount,
      action: action,
      stablecoin_token: USDC_TOKEN,
    });
    return result;
  }

  // Step 3: Seller accepts bid
  async acceptBid(propertyId, bidId, useEscrow = false) {
    if (useEscrow) {
      return await this.contract.accept_bid_with_escrow({
        property_id: propertyId,
        bid_id: bidId,
      });
    } else {
      return await this.contract.accept_bid({
        property_id: propertyId,
        bid_id: bidId,
      });
    }
  }

  // Step 4: Seller mints & transfers document
  async releaseDocument(propertyId, bidId, imageUri, description) {
    return await this.contract.confirm_document_release({
      property_id: propertyId,
      bid_id: bidId,
      document_image_uri: imageUri,
      document_description: description,
    });
  }

  // Step 5: Buyer confirms document receipt
  async confirmDocumentReceipt(propertyId, bidId) {
    return await this.contract.confirm_document_receipt({
      property_id: propertyId,
      bid_id: bidId,
    });
  }

  // Step 6: Release payment to seller
  async releasePayment(propertyId, bidId) {
    return await this.contract.release_payment({
      property_id: propertyId,
      bid_id: bidId,
    });
  }

  // Step 7: Complete transaction
  async completeBid(propertyId, bidId) {
    return await this.contract.complete_bid({
      property_id: propertyId,
      bid_id: bidId,
    });
  }

  // Query helpers
  async getBidStatus(propertyId, bidId) {
    const bid = await this.contract.get_bid_view({
      property_id: propertyId,
      bid_id: bidId,
    });
    return bid.status;
  }

  async getDocumentNFT(propertyId, bidId) {
    const bid = await this.contract.get_bid_view({
      property_id: propertyId,
      bid_id: bidId,
    });
    if (bid.document_token_id) {
      return await this.contract.nft_metadata({
        token_id: bid.document_token_id,
      });
    }
    return null;
  }
}

// Usage example
async function purchaseFlow() {
  const wallet = new WalletConnection(window.near);
  const sheda = new ShedaIntegration(wallet);

  const propertyId = 1;
  const bidAmount = "500000000000000000000000"; // 500 USDC

  // Buyer's perspective
  console.log("1. Approving funds...");
  await sheda.approveFunds(bidAmount);

  console.log("2. Placing bid...");
  await sheda.placeBid(propertyId, bidAmount, "Purchase");

  // Wait for seller...

  // Seller's perspective
  console.log("3. Seller accepts bid...");
  await sheda.acceptBid(propertyId, 1, false); // No escrow

  console.log("4. Seller releases document...");
  await sheda.releaseDocument(
    propertyId,
    1,
    "ipfs://QmDocumentHash",
    "Property Deed"
  );

  // Buyer's perspective (continued)
  console.log("5. Buyer confirms document...");
  await sheda.confirmDocumentReceipt(propertyId, 1);

  console.log("6. Release payment...");
  await sheda.releasePayment(propertyId, 1);

  console.log("7. Complete transaction...");
  await sheda.completeBid(propertyId, 1);

  // Verify ownership
  const doc = await sheda.getDocumentNFT(propertyId, 1);
  console.log("Document NFT:", doc);
}
```

### Example 2: Lease Setup (CLI)

```bash
#!/bin/bash

PROPERTY_ID=3
CONTRACT="shedasolution.testnet"
OWNER="owner.testnet"
TENANT="tenant.testnet"
USDC="usdc.testnet"
ESCROW_AMOUNT="50000000000000000000000"

# Owner lists property for lease
echo "Listing property for lease..."
near call $CONTRACT place_property '{
  "description": "3BR apartment with gym access",
  "metadata_uri": "ipfs://QmApartment",
  "is_for_sale": false,
  "price": null,
  "lease_duration_months": 12,
  "damage_escrow": "'$ESCROW_AMOUNT'",
  "stablecoin_token": "'$USDC'"
}' --accountId $OWNER

# Tenant approves damage escrow
echo "Tenant approving escrow..."
near call $USDC ft_approve '{
  "receiver_id": "'$CONTRACT'",
  "amount": "'$ESCROW_AMOUNT'"
}' --accountId $TENANT --amount 1

# Tenant places lease bid
echo "Tenant placing lease bid..."
near call $CONTRACT place_bid '{
  "property_id": '$PROPERTY_ID',
  "amount": "'$ESCROW_AMOUNT'",
  "action": "Lease",
  "stablecoin_token": "'$USDC'"
}' --accountId $TENANT

# Owner accepts
echo "Owner accepts lease..."
near call $CONTRACT accept_bid '{
  "property_id": '$PROPERTY_ID',
  "bid_id": 1
}' --accountId $OWNER --amount 1

# Lease is now active
echo "Lease activated successfully!"
```

---

## Error Handling

### Common Errors & Solutions

#### Error 1: "InvalidPaymentToken"

```
Contract call failed: InvalidPaymentToken
```

**Cause**: Stablecoin address not in accepted list.

**Solution**:
```bash
# Check accepted tokens
near view shedasolution.testnet get_config

# If needed, admin whitelist:
near call shedasolution.testnet add_accepted_stablecoin '{
  "stablecoin_address": "dai.testnet"
}' --accountId admin.testnet
```

#### Error 2: "PropertyNotFound"

```
Contract call failed: PropertyNotFound
```

**Cause**: Property ID doesn't exist.

**Solution**:
```bash
# List all properties
near view shedasolution.testnet get_properties '{"from_index": 0, "limit": 10}'
```

#### Error 3: "NotPropertyOwner"

```
Contract call failed: NotPropertyOwner
```

**Cause**: Only property owner can accept bids.

**Solution**: Verify caller account is property `owner_id`.

#### Error 4: "InvalidBidAmount"

```
Contract call failed: InvalidBidAmount
```

**Cause**: Bid less than property price.

**Solution**:
```bash
# Check property price
near view shedasolution.testnet get_property_view '{"property_id": 1}'

# Ensure bid amount >= property price
```

#### Error 5: "IncorrectBidAmount"

```
Contract call failed: IncorrectBidAmount { expected: 500000000000000000000000, received: 400000000000000000000000 }
```

**Cause**: Approval amount doesn't match bid.

**Solution**:
```bash
# Approve exact amount needed
near call usdc.testnet ft_approve '{
  "receiver_id": "shedasolution.testnet",
  "amount": "500000000000000000000000"
}' --accountId buyer.testnet --amount 1
```

#### Error 6: "Reentrancy Lock Active"

```
Contract call failed: Reentrancy [operation] already in progress
```

**Cause**: Recursive call detected.

**Solution**: This is a security feature. Ensure sequential operations, not nested calls.

---

## Events & Monitoring

### Emitted Events

#### PropertyMintedEvent

```json
{
  "property_id": 1,
  "owner_id": "bob.testnet",
  "description": "2BR house at 123 Main St",
  "metadata_uri": "ipfs://QmXXX",
  "is_for_sale": true,
  "price": "500000000000000000000000"
}
```

**When**: `place_property()` called

#### BidPlacedEvent

```json
{
  "bid_id": 1,
  "bidder": "alice.testnet",
  "property_id": 1,
  "amount": "500000000000000000000000",
  "action": "Purchase",
  "stablecoin_token": "usdc.testnet"
}
```

**When**: `place_bid()` called

#### BidAcceptedEvent

```json
{
  "property_id": 1,
  "bid_id": 1,
  "accepted_at": 1234567890000000000
}
```

**When**: `accept_bid()` or `accept_bid_with_escrow()` called

#### DocumentReleasedEvent

```json
{
  "property_id": 1,
  "bid_id": 1,
  "document_token_id": "doc:1:1",
  "document_image_uri": "ipfs://QmDocumentHash",
  "released_at": 1234567890000000000
}
```

**When**: `confirm_document_release()` called

#### DocumentConfirmedEvent

```json
{
  "property_id": 1,
  "bid_id": 1,
  "confirmed_by": "alice.testnet",
  "confirmed_at": 1234567890000000000
}
```

**When**: `confirm_document_receipt()` called

#### PaymentReleasedEvent

```json
{
  "property_id": 1,
  "bid_id": 1,
  "amount": "500000000000000000000000",
  "released_to": "bob.testnet",
  "released_at": 1234567890000000000
}
```

**When**: `release_payment()` called

#### BidCompletedEvent

```json
{
  "property_id": 1,
  "bid_id": 1,
  "completed_at": 1234567890000000000
}
```

**When**: `complete_bid()` called

### Monitoring Integration (Indexer)

To monitor events in real-time:

```javascript
// Using NEAR Indexer API (example)

async function watchBidActivity(propertyId) {
  const query = `
    SELECT 
      receipt_id,
      block_timestamp,
      args::"$3"::json as event_data
    FROM events
    WHERE contract = 'shedasolution.testnet'
      AND method = 'place_bid'
      AND event_data->'property_id' = ${propertyId}
    ORDER BY block_timestamp DESC
    LIMIT 100
  `;

  // Execute via NEAR Indexer GraphQL or similar
  const results = await fetch("https://indexer.near.org/api", {
    body: query,
  }).then(r => r.json());

  return results;
}
```

---

## Additional Resources

### Contract Addresses
- **Testnet**: `shedasolution.testnet`
- **Mainnet**: `sheda.near` (deployment TBD)

### Token Addresses (Examples)
- **USDC (Testnet)**: `usdc.testnet`
- **USDT (Testnet)**: `usdt.testnet`
- **v2-ref (Testnet)**: `v2-ref-finance.testnet`

### View Functions (Read-only)

```bash
# Get property details
near view shedasolution.testnet get_property_view '{"property_id": 1}'

# Get bid status
near view shedasolution.testnet get_bid_view '{"property_id": 1, "bid_id": 1}'

# Get all properties (paginated)
near view shedasolution.testnet get_properties '{"from_index": 0, "limit": 10}'

# Get all bids for property
near view shedasolution.testnet get_bids_for_property '{"property_id": 1}'

# Get NFT metadata
near view shedasolution.testnet nft_metadata '{"token_id": "doc:1:1"}'

# Check active lease
near view shedasolution.testnet get_lease '{"property_id": 2}'
```

### Testing Checklist

- [ ] Deploy contract to testnet
- [ ] List property with `place_property`
- [ ] Place bid with sufficient funds
- [ ] Accept bid with `accept_bid`
- [ ] Mint & transfer document with `confirm_document_release`
- [ ] Confirm receipt with `confirm_document_receipt`
- [ ] Release payment with `release_payment`
- [ ] Complete bid with `complete_bid`
- [ ] Verify property NFT transferred to buyer
- [ ] Test escrow path with `accept_bid_with_escrow`
- [ ] Test lease scenario with `Action::Lease`
- [ ] Test dispute raise/resolve cycle

---

## Summary

The Sheda contract implements a **complete property transaction lifecycle**:

1. **Listing**: Owner creates property record
2. **Bidding**: Buyers place offers with escrow funds
3. **Acceptance**: Owner accepts bid (triggers document prep)
4. **Documentation**: Seller mints NFT deed; buyer receives
5. **Acknowledgment**: Buyer confirms receipt
6. **Payment**: Funds release to seller
7. **Completion**: Transaction finalized; property transferred

All flows are protected by:
- State machine validation (status guards)
- Reentrancy protection
- Multi-stablecoin support
- Oracle dispute resolution
- Escrow time locks

This design ensures **trustless transactions** between parties who may not know each other, leveraging NEAR's blockchain for transparency and NFTs for proof-of-ownership.

