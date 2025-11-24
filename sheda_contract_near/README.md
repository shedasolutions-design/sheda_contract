# NEAR Smart Contract

This is a NEAR Protocol smart contract written in Rust.

## Building

To build the contract, run:

```bash
./build.sh
```

Or using cargo-near (if installed):

```bash
cargo near build
```

## Testing

Run tests with:

```bash
cargo test
```

## Deploying

Deploy the contract using NEAR CLI:

```bash
near deploy --accountId <your-account>.testnet --wasmFile target/wasm32-unknown-unknown/release/sheda_contract_near.wasm
```
