# sheda_contract

A NEAR smart contract for a real estate platform implemented in Rust.

This repository contains the contract source, build settings for reproducible WASM, and tests.

## Quickstart

Prerequisites

- Rust (recommended via rustup)
- wasm32 target: `rustup target add wasm32-unknown-unknown`
- cargo-near: `cargo install cargo-near`
- NEAR CLI (optional, for interacting with chains): see https://near.cli.rs

Build (local)

```bash
cargo near build
```

Run tests

```bash
cargo test
```

Deploy

Deployment is automated via the repository's GitHub Actions CI/CD pipeline. To deploy manually, install `cargo-near` and then:

- For debugging/non-reproducible builds:

```bash
cargo near deploy build-non-reproducible-wasm <account-id>
```

- For production/reproducible builds:

```bash
cargo near deploy build-reproducible-wasm <account-id>
```

Replace `<account-id>` with your NEAR account name.

Notes

- The Cargo.toml is configured for reproducible builds using the `sourcescan/cargo-near` Docker image; see the `package.metadata.near.reproducible_build` section for details.
- NEP-0330 metadata is available for contracts built with `cargo-near`.

Useful links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract toolkit for Rust
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [near CLI](https://near.cli.rs)

License

This repository does not specify a license in the project files. If you intend to publish or share this project, add a LICENSE file to clarify usage permissions.

Repository

https://github.com/shedasolutions-design/sheda_contract
