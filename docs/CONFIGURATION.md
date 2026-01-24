# Kora Rent-Reclaim Bot — Backend

Backend service for automated discovery, eligibility analysis, and rent reclamation of Kora-sponsored Solana accounts.

## Overview

The Kora Rent-Reclaim Bot is a Rust-based backend system designed to identify Solana accounts sponsored by a Kora node, evaluate their eligibility for rent recovery, and safely reclaim unused SOL to a designated treasury wallet. The system operates using on-chain analysis and standard Solana RPC APIs and is intended for long-running, automated operation with built-in safety controls.

## Capabilities

* **Account Discovery**
  Identifies accounts sponsored by a Kora node by analyzing the transaction history of the Kora fee payer.

* **Eligibility Evaluation**
  Determines reclaim eligibility based on account state (closed or empty), inactivity duration, and configurable policy rules.

* **Rent Reclamation**
  Reclaims lamports from eligible System and SPL Token accounts using appropriate Solana instructions.

* **Batch-Oriented Processing**
  Processes accounts in configurable batches with rate limiting to avoid RPC throttling.

* **Dry-Run Execution**
  Supports simulation mode for validation without submitting on-chain transactions.

* **Safety Controls**
  Includes whitelisting, blacklisting, inactivity thresholds, and confirmation prompts.

* **Logging and Observability**
  Provides structured logging and tracing for auditability and debugging.

* **Statistics and Reporting**
  Tracks reclaimed amounts, maintains historical records, and supports JSON export.

## Requirements

* Rust 1.70 or later
* Solana CLI tools (optional, for keypair management)
* Access to a Solana RPC endpoint (devnet or mainnet)
* Kora node fee payer public key
* Treasury wallet keypair for receiving reclaimed SOL

## Build and Initialization

The project is built using Cargo. A release build is recommended for production usage.

```bash
cargo build --release
```

An SQLite database is used to track discovered accounts and reclaim history. The database must be initialized before first use.

```bash
cargo run --release -- init
```

## Configuration

Configuration is provided through a TOML file. A sample configuration can be copied and customized as needed.

```bash
cp config.toml my-config.toml
```

### Example Configuration

```toml
[solana]
rpc_url = "https://api.devnet.solana.com"
network = "Devnet"
commitment = "confirmed"

[kora]
operator_pubkey = "YOUR_KORA_OPERATOR_PUBKEY_HERE"
treasury_wallet = "YOUR_TREASURY_WALLET_PUBKEY_HERE"
treasury_keypair_path = "./treasury-keypair.json"

[reclaim]
min_inactive_days = 30
dry_run = true
batch_size = 10
batch_delay_ms = 1000
```

## Telegram Notifications

The backend supports optional Telegram notifications for operational events and alerts.

### Supported Events

* Completion of scan cycles
* Individual reclaim success or failure
* Batch processing summaries
* High-value reclaim alerts
* Critical error notifications

### Configuration

```toml
[telegram]
bot_token = "your_bot_token"
authorized_users = [your_telegram_user_id]
notifications_enabled = true
alert_threshold_sol = 0.1
```

Telegram user IDs can be obtained via the `@userinfobot` service.

## Operation Modes

### Scanning

The scan operation analyzes the Kora operator’s transaction history and updates the local database with discovered accounts and eligibility status.

```bash
cargo run -- scan
cargo run -- scan --verbose
cargo run -- scan --limit 1000
```

### Reclaiming

Rent can be reclaimed from a specific account or from eligible accounts discovered during scans.

```bash
cargo run -- reclaim ACCOUNT_PUBKEY
cargo run -- reclaim ACCOUNT_PUBKEY --yes
cargo run -- reclaim ACCOUNT_PUBKEY --dry-run
```

### Automated Service Mode

The bot can be run as a long-lived service that periodically scans and reclaims accounts.

```bash
cargo run -- auto --interval 3600
cargo run -- auto --interval 3600 --dry-run
```

### Statistics

Operational statistics and reclaim history can be queried at any time.

```bash
cargo run -- stats
cargo run -- stats --format json
```

## Configuration Reference

### Solana

* `rpc_url`: RPC endpoint URL
* `network`: Mainnet, Devnet, or Testnet
* `commitment`: processed, confirmed, or finalized
* `rate_limit_delay_ms`: Delay between RPC requests

### Kora

* `operator_pubkey`: Fee payer public key used by the Kora node
* `treasury_wallet`: Destination wallet for reclaimed lamports
* `treasury_keypair_path`: Keypair used to sign reclaim transactions

### Reclaim Policy

* `min_inactive_days`: Required inactivity period
* `dry_run`: Enable simulation mode
* `batch_size`: Number of accounts processed per batch
* `batch_delay_ms`: Delay between batches
* `scan_interval_seconds`: Interval for auto mode
* `whitelist`: Accounts that must never be reclaimed
* `blacklist`: Accounts excluded from processing

### Storage

* `path`: SQLite database path

## Internal Workflow

1. **Discovery**
   Transaction history for the Kora operator is scanned to identify sponsored account creation events.

2. **Eligibility Analysis**
   Each account is evaluated based on existence, balance, activity history, and policy constraints.

3. **Reclamation**

   * System accounts: lamports are transferred to the treasury wallet.
   * SPL Token accounts: accounts are closed using token program instructions.

4. **Safeguards**
   Dry-run mode, rate limiting, confirmations, and logging ensure safe operation.

## Troubleshooting

**Configuration load failures**
Verify the presence and syntax of the configuration file.

**Invalid operator public key**
Ensure the key is valid base58 and corresponds to the Kora fee payer.

**Keypair read errors**
Confirm the treasury keypair file exists and follows Solana’s standard JSON format.

**RPC rate limiting**
Increase delays or use a higher-capacity RPC provider.

**No eligible accounts detected**
Accounts may still be active or within the inactivity window. Confirm configuration values and operator key accuracy.

## Development

Debug builds:

```bash
cargo build
```

Test execution:

```bash
cargo test
```

Verbose logging:

```bash
RUST_LOG=kora_reclaim=debug cargo run -- scan
```

## Safety Considerations

* Validate behavior on devnet before mainnet deployment
* Enable dry-run mode during initial testing
* Use whitelists to protect critical accounts
* Configure conservative inactivity thresholds
* Monitor logs continuously
* Secure and back up treasury keypairs

## Architecture

```
src/
├── solana/       # RPC client, discovery logic, rent calculations
├── kora/         # Kora-related types and monitoring
├── reclaim/      # Eligibility checks and reclaim execution
├── storage/      # SQLite persistence layer
├── cli/          # Command-line interface
├── config.rs     # Configuration parsing and validation
├── error.rs      # Error definitions
├── utils.rs      # Shared utilities
└── main.rs       # Application entry point
```

## License

[Specify license]

## Support

For issues or questions:

* Review application logs
* Refer to this documentation
* Open a GitHub issue if applicable

---

**Note**: This application submits transactions to the Solana network and can move real funds. Ensure full understanding of its behavior before using it on mainnet.
