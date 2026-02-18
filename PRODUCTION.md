# Production Deployment Best Practices

This guide provides recommendations for deploying the Sheda Contract to NEAR mainnet in a production environment.

## Pre-Deployment Checklist

### 1. Security Audit
- [ ] **External Security Audit**: Engage a professional smart contract auditor (e.g., Trail of Bits, Halborn, Oak Security)
- [ ] **Code Review**: Have at least 2 independent developers review the entire codebase
- [ ] **Reentrancy Testing**: Verify all reentrancy locks work correctly in adversarial scenarios
- [ ] **Overflow Testing**: Confirm all arithmetic operations use checked variants
- [ ] **Access Control**: Verify owner/admin permissions are properly enforced

### 2. Testing & QA
- [ ] **Unit Tests**: All unit tests pass (`cargo test`)
- [ ] **Integration Tests**: All integration tests pass (`cargo test -- --include-ignored`)
- [ ] **Testnet Deployment**: Deploy to testnet and perform full lifecycle testing
- [ ] **Load Testing**: Test with high transaction volumes to identify bottlenecks
- [ ] **Edge Case Testing**: Verify behavior with extreme values (0, u128::MAX, etc.)

### 3. Gas Optimization
- [ ] **Profile Gas Usage**: Measure gas consumption for all operations
- [ ] **Pagination Limits**: Verify all view methods enforce reasonable limits
- [ ] **Cross-Contract Calls**: Ensure sufficient gas is allocated (see [GAS_OPTIMIZATION.md](./GAS_OPTIMIZATION.md))
- [ ] **Storage Costs**: Calculate storage requirements and enforce deposits where needed

### 4. Configuration & Parameters
- [ ] **Timelock Delays**: Set appropriate delays for escrow release and bid claims
- [ ] **Upgrade Delay**: Configure reasonable upgrade delay (e.g., 7 days) for governance
- [ ] **Stablecoin Whitelist**: Add only verified stablecoin contracts
- [ ] **Oracle Account**: Set trusted oracle account for dispute resolution
- [ ] **Admin Accounts**: Add initial admin accounts (multisig recommended)

### 5. Documentation
- [ ] **User Guide**: Provide clear instructions for all user-facing methods
- [ ] **API Reference**: Document all public methods with parameters and return types
- [ ] **Event Schema**: Document all events for indexer integration
- [ ] **Admin Runbook**: Create operational guide for admin actions

## Deployment Process

### Step 1: Build Optimized WASM
```bash
# Install wasm-opt for size optimization
cargo install wasm-opt

# Build release WASM
cargo near build

# Optimize WASM size
wasm-opt -Oz -o target/near/sheda_contract.wasm target/near/sheda_contract.wasm

# Verify WASM size (should be < 500KB for efficient deployment)
ls -lh target/near/sheda_contract.wasm
```

### Step 2: Deploy to Testnet
```bash
# Create testnet account
near create-account sheda-test.testnet --masterAccount your-account.testnet --initialBalance 50

# Deploy contract
near deploy sheda-test.testnet target/near/sheda_contract.wasm

# Initialize contract
near call sheda-test.testnet new '{"media_url": "https://example.com/logo.png", "supported_stablecoins": ["usdc.testnet", "usdt.testnet"]}' --accountId your-account.testnet
```

### Step 3: Testnet Validation
1. **Mint Test Properties**: Create 10-20 properties with various configurations
2. **Place Test Bids**: Simulate bids from multiple accounts
3. **Accept/Reject Bids**: Verify all bid lifecycle operations
4. **Create Leases**: Test lease creation with escrow
5. **Raise Disputes**: Test dispute lifecycle including voting and resolution
6. **Admin Operations**: Test all admin methods (add/remove admin, set oracle, etc.)
7. **Upgrade Flow**: Test upgrade proposal and application with timelock

### Step 4: Deploy to Mainnet
```bash
# Create mainnet account (recommended: use a descriptive name)
near create-account sheda.near --masterAccount your-mainnet-account.near --initialBalance 100

# Deploy contract
near deploy sheda.near target/near/sheda_contract.wasm --force

# Initialize with production values
near call sheda.near new '{
  "media_url": "https://sheda.io/metadata.json",
  "supported_stablecoins": [
    "usdc.near",
    "usdt.near",
    "dai.near"
  ]
}' --accountId your-mainnet-account.near
```

### Step 5: Post-Deployment Configuration
```bash
# Set upgrade delay (7 days in nanoseconds)
near call sheda.near set_upgrade_delay '{"delay_ns": "604800000000000"}' --accountId sheda.near

# Set timelock configuration
near call sheda.near set_time_lock_config '{
  "bid_expiry_ns": "2592000000000000",
  "escrow_release_delay_ns": "604800000000000",
  "lost_bid_claim_delay_ns": "1209600000000000"
}' --accountId sheda.near

# Set oracle account
near call sheda.near set_oracle_account '{"oracle_id": "dispute-oracle.near"}' --accountId sheda.near --deposit 0.000000000000000000000001

# Add initial admin accounts
near call sheda.near add_admin '{"new_admin_id": "admin1.near"}' --accountId sheda.near
near call sheda.near add_admin '{"new_admin_id": "admin2.near"}' --accountId sheda.near
```

## Operational Best Practices

### Access Control

#### Owner Account Security
- **Use Multisig**: Deploy owner account as multisig (e.g., 2-of-3 or 3-of-5)
- **Key Management**: Store owner keys in hardware wallets or secure key management systems
- **Rotation Policy**: Rotate multisig members periodically (e.g., annually)

#### Admin Account Management
- **Separate Responsibilities**: Different admins for different roles (dispute resolution, property management, etc.)
- **Audit Trail**: Log all admin actions with timestamps and account IDs
- **Regular Review**: Audit admin list quarterly and remove inactive admins

### Upgrade Governance

#### Upgrade Process
1. **Code Review**: All upgrades must pass independent code review
2. **Testnet Testing**: Deploy upgrade to testnet and test for at least 1 week
3. **Community Notice**: Notify users 7+ days before upgrade proposal
4. **Propose Upgrade**: Owner proposes upgrade with new WASM code
5. **Timelock Delay**: Wait for configured delay period (e.g., 7 days)
6. **Apply Upgrade**: Owner applies upgrade after delay expires
7. **Verification**: Verify upgrade succeeded and state migrated correctly

#### Rollback Plan
- Maintain previous WASM versions in version control
- Test rollback procedure on testnet before mainnet deployment
- Have emergency upgrade process for critical security fixes

### Monitoring & Alerting

#### Key Metrics to Monitor
- **Transaction Volume**: Track daily/weekly transaction counts
- **Failed Transactions**: Alert on elevated failure rates
- **Gas Usage**: Monitor for unexpected gas spikes
- **Storage Growth**: Track state size growth over time
- **Stablecoin Balances**: Monitor contract token holdings vs. expected values

#### Monitoring Tools
- **NEAR Explorer**: https://explorer.near.org
- **Contract Logs**: Index and analyze emitted events
- **Custom Indexer**: Deploy indexer for real-time monitoring (see [NEAR Indexer](https://docs.near.org/tools/indexer-for-explorer))
- **Alerting**: Set up PagerDuty/Opsgenie for critical issues

### Incident Response

#### Security Incident Protocol
1. **Detection**: Monitor for suspicious activity (large withdrawals, unusual patterns)
2. **Assessment**: Determine severity and impact
3. **Containment**: If critical, consider emergency pause (requires emergency admin method)
4. **Investigation**: Analyze transaction history and logs
5. **Resolution**: Deploy patch if vulnerability confirmed
6. **Communication**: Notify users of incident and resolution
7. **Post-Mortem**: Document incident and improve processes

#### Emergency Procedures
- **Emergency Withdrawal**: Owner can invoke `emergency_withdraw` to recover funds
- **Emergency Upgrade**: Fast-track critical security fixes (bypass normal timelock with multisig consensus)
- **Contact List**: Maintain 24/7 contact list for owner and admins

## Integration Best Practices

### Frontend Integration

#### Web3 Wallet Connection
```javascript
// Use wallet-selector for multi-wallet support
import { setupWalletSelector } from "@near-wallet-selector/core";
import { setupModal } from "@near-wallet-selector/modal-ui";
import { setupNearWallet } from "@near-wallet-selector/near-wallet";

const selector = await setupWalletSelector({
  network: "mainnet",
  modules: [setupNearWallet()],
});

const modal = setupModal(selector, {
  contractId: "sheda.near",
});
```

#### Method Call Examples
```javascript
// Mint property
await wallet.signAndSendTransaction({
  receiverId: "sheda.near",
  actions: [
    {
      type: "FunctionCall",
      params: {
        methodName: "mint_property",
        args: {
          title: "Beach House",
          description: "Beautiful beach house",
          media_uri: "ipfs://QmXxx",
          price: "1000000",
          is_for_sale: true,
          lease_duration_months: 12,
        },
        gas: "30000000000000",
        deposit: "10000000000000000000000", // 0.01 NEAR
      },
    },
  ],
});

// Place bid via stablecoin transfer
await wallet.signAndSendTransaction({
  receiverId: "usdc.near",
  actions: [
    {
      type: "FunctionCall",
      params: {
        methodName: "ft_transfer_call",
        args: {
          receiver_id: "sheda.near",
          amount: "1000000",
          msg: JSON.stringify({
            property_id: 0,
            action: "Purchase",
            stablecoin_token: "usdc.near",
          }),
        },
        gas: "100000000000000",
        deposit: "1",
      },
    },
  ],
});
```

### Indexer Integration

#### Event Monitoring
The contract emits NEP-297 compliant events for all state changes:
```json
{
  "standard": "nep297",
  "version": "1.0.0",
  "event": "PropertyMinted",
  "data": {
    "token_id": "0",
    "owner_id": "alice.near",
    "base_uri": "https://example.com/property/0"
  }
}
```

#### Indexer Setup (NEAR Lake)
```rust
// Use NEAR Lake for indexing events
use near_lake_framework::{LakeConfigBuilder, near_indexer_primitives};

let config = LakeConfigBuilder::default()
    .mainnet()
    .start_block_height(12345)
    .build()?;

let stream = near_lake_framework::streamer(config);
tokio::pin!(stream);

while let Some(streamer_message) = stream.next().await {
    // Process events from sheda.near contract
    for shard in streamer_message.shards {
        for outcome in shard.receipt_execution_outcomes {
            if outcome.receipt.receiver_id == "sheda.near" {
                // Parse and store events
            }
        }
    }
}
```

## Cost Analysis

### Storage Costs
| Operation | Storage Added | Cost (NEAR @ 0.1 NEAR/100KB) |
|-----------|---------------|------------------------------|
| Mint Property | ~500 bytes | ~0.0005 NEAR |
| Place Bid | ~300 bytes | ~0.0003 NEAR |
| Create Lease | ~400 bytes | ~0.0004 NEAR |
| Raise Dispute | ~200 bytes | ~0.0002 NEAR |

**Recommendation**: Require users to attach sufficient deposit for storage costs:
- Mint property: 0.01 NEAR
- Place bid: 0.005 NEAR
- Create lease: 0.005 NEAR

### Transaction Costs
| Operation | Gas (TGas) | Cost (NEAR @ 0.0001 NEAR/TGas) |
|-----------|------------|--------------------------------|
| Mint Property | 3 | ~0.0003 NEAR |
| ft_on_transfer | 8 | ~0.0008 NEAR |
| Accept Bid | 5 | ~0.0005 NEAR |
| Accept Bid with Escrow | 10 | ~0.001 NEAR |

**Total User Cost** (mint + bid + accept): ~0.012 NEAR (~$0.05 at $4/NEAR)

## Compliance & Legal

### Regulatory Considerations
- **KYC/AML**: Consider integrating KYC provider for high-value transactions
- **Property Rights**: Ensure legal framework for tokenized property ownership
- **Dispute Resolution**: Establish clear legal process for dispute resolution
- **Terms of Service**: Require users to accept ToS before using platform
- **Privacy**: Implement GDPR-compliant data handling if serving EU users

### Disclaimers
> **Important**: Smart contracts are immutable and audits do not guarantee security. Deploy at your own risk. This contract is provided as-is without warranty of any kind.

## Support & Resources

### Developer Resources
- **NEAR Docs**: https://docs.near.org
- **NEP Standards**: https://github.com/near/NEPs
- **SDK Reference**: https://docs.rs/near-sdk

### Community Support
- **Discord**: NEAR Protocol Discord
- **Forum**: https://gov.near.org
- **GitHub**: File issues at https://github.com/near/near-sdk-rs

### Professional Services
- **Auditors**: Trail of Bits, Halborn, Oak Security, Kudelski Security
- **Development**: Pagoda, Proximity Labs, Mintbase
- **Infrastructure**: Kitwallet, LedgerHQ for hardware wallet integration

## Conclusion

Production deployment requires careful planning, testing, and ongoing monitoring. Follow this guide to minimize risks and ensure a successful launch. Always prioritize security and user safety over speed-to-market.

For questions or support, contact the Sheda development team or engage with the NEAR developer community.
