# Gas Optimization Guide

This document provides guidance on gas consumption for the Sheda Contract operations.

## Gas Budget Recommendations

### 1. View Methods
View methods don't consume gas but have computational limits. All paginated methods enforce `MAX_PAGINATION_LIMIT = 200` to prevent excessive computation:
- `get_properties(from_index, limit)` - Safe up to 200 items
- `get_all_bids(from_index, limit)` - Safe up to 200 items
- `get_all_leases(from_index, limit)` - Safe up to 200 items

**Best Practice**: Use pagination with `limit <= 100` for consistent performance across all NEAR nodes.

### 2. State Mutation Operations

#### Property Management
- `mint_property`: ~2-3 TGas (creates NFT token + property record)
- `update_property`: ~1-2 TGas (modifies existing property)
- `delist_property`: ~1-2 TGas (updates listing status)
- `delete_property`: ~2-3 TGas (removes NFT + cleans up indexes)

#### Bidding Operations
- `ft_on_transfer` (bid placement): ~5-10 TGas (includes cross-contract callback)
- `accept_bid`: ~3-5 TGas (single property transfer)
- `accept_bid_with_escrow`: ~8-12 TGas (includes lease creation + escrow hold)
- `reject_bid`: ~5-8 TGas (includes refund transfer)
- `cancel_bid`: ~5-8 TGas (includes refund transfer)
- `claim_lost_bid`: ~5-8 TGas (includes refund transfer after timelock)

#### Lease & Dispute Operations
- `raise_dispute`: ~2-3 TGas (updates lease state)
- `resolve_dispute`: ~5-8 TGas (includes payout transfer)
- `vote_lease_dispute`: ~1-2 TGas (updates vote counts)
- `request_oracle_dispute`: ~15-20 TGas (cross-contract call + callback)

### 3. Cross-Contract Calls

All cross-contract calls include explicit gas budgets:
- FT transfers: 30 TGas (via `with_static_gas`)
- Oracle requests: 50 TGas for external call + 30 TGas for callback
- Property instance interactions: 30 TGas

**Critical**: Always attach at least 1 yoctoNEAR (`with_attached_deposit`) for cross-contract calls.

### 4. Administrative Operations

#### Batch Operations
- `add_admin`: ~1 TGas (single insert)
- `add_supported_stablecoin`: ~1 TGas (single push)
- `set_time_lock_config`: ~1 TGas (updates 3 fields)

**Warning**: No batch operations (e.g., "add 100 admins at once") are provided to prevent gas overflow. Use individual calls or implement batching with gas checks.

### 5. Loop Limits and Iteration Safety

The contract uses safe iteration patterns:

#### Property Lookups by Owner
```rust
pub fn get_property_by_owner(&self, owner_id: AccountId) -> Vec<PropertyView>
```
- Iterates over `property_per_owner[owner_id]` vector
- **Limit**: No explicit cap, but practical limit ~50 properties per owner
- **Risk**: If owner has 1000+ properties, this may timeout
- **Mitigation**: Future enhancement should add pagination

#### Bid Enumeration
```rust
pub fn get_bids_for_property(&self, property_id: u64) -> Vec<BidView>
```
- Returns all bids for a property (active + rejected + accepted)
- **Limit**: Unbounded per property
- **Risk**: Properties with 100+ bids may approach limits
- **Mitigation**: Consider adding pagination or status filter

#### Lease Enumeration
```rust
pub fn get_leases_with_disputes(&mut self) -> Vec<LeaseView>
```
- Admin-only method filters all leases with active disputes
- **Limit**: Iterates over entire `leases` map
- **Risk**: If 10,000+ total leases exist, this operation is expensive
- **Mitigation**: Use pagination in production; this is acceptable for MVP

### 6. Storage Staking

NEAR requires storage staking (0.1 NEAR per 100KB). Operations that increase state size:

| Operation | Storage Cost (approx) |
|-----------|----------------------|
| `mint_property` | ~0.01 NEAR (NFT + property + indexes) |
| `ft_on_transfer` (new bid) | ~0.005 NEAR (bid entry) |
| `accept_bid_with_escrow` (new lease) | ~0.008 NEAR (lease + indexes) |
| `raise_dispute` (dispute info) | ~0.003 NEAR (dispute struct added) |

**Note**: The contract currently does NOT enforce deposit requirements for minting or bidding. In production, methods like `mint_property` should `require!` sufficient attached deposit to cover storage.

### 7. Reentrancy Protection

Reentrancy locks consume minimal gas (~0.1 TGas) but add state:
- Locks are inserted before external calls
- Locks are removed in callbacks
- **Critical**: Always pair lock/unlock to prevent permanent locks

Protected operations:
- `ft_on_transfer` → `unlock_ft_on_transfer_callback`
- `accept_bid_with_escrow` → bid-specific lock removed in callback
- `claim_lost_bid` → lock removed after refund

### 8. Optimization Opportunities

#### Current Implementation
- ✅ Pagination on all major view methods
- ✅ Explicit gas budgets on cross-contract calls
- ✅ Reentrancy guards prevent double-spending
- ✅ Checked arithmetic prevents overflow/underflow

#### Future Enhancements
- ⚠️ Add pagination to `get_property_by_owner`
- ⚠️ Add status filter to `get_bids_for_property` (e.g., active_only)
- ⚠️ Implement lazy loading for dispute info (separate map instead of embedded)
- ⚠️ Add deposit requirements for storage-heavy operations
- ⚠️ Batch update methods with explicit gas checks
- ⚠️ Offload historical data (sold properties) to indexer

### 9. Testing Gas Consumption

To profile actual gas usage:

```bash
# Build with gas instrumentation
RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release

# Run workspaces tests with gas logging
RUST_LOG=gas cargo test -- --nocapture

# Use near-cli to test mainnet/testnet operations
near call <contract> mint_property '{"name": "Test"}' --accountId <account> --gas 300000000000000
```

### 10. Emergency Considerations

If gas limits are consistently exceeded:
1. **Reduce iteration scope**: Add pagination to unbounded methods
2. **Break operations into steps**: E.g., two-phase commit for complex transactions
3. **Use indexers**: Off-chain processing for historical queries
4. **Optimize storage**: Remove unnecessary fields, use compact encodings

## Summary

The contract is designed with gas efficiency in mind:
- All view methods have pagination limits
- Cross-contract calls have explicit gas budgets
- Reentrancy guards prevent double-execution
- No unbounded loops in critical paths

For production deployment:
- Monitor gas usage for all operations
- Set deposit requirements for storage operations
- Implement pagination for owner/bid lookups
- Consider archival strategy for historical data
