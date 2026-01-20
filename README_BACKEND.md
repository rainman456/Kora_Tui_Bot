# Kora Rent-Reclaim Bot - Backend

Complete backend implementation for automated rent reclamation from Kora-sponsored Solana accounts.

## Features

✅ **Account Discovery**: Parses Solana blockchain transaction history to discover accounts sponsored by your Kora node  
✅ **Eligibility Checking**: Detects closed, empty, and inactive accounts based on configurable criteria  
✅ **Automated Reclaim**: Safely reclaims rent from eligible accounts (supports System and SPL Token accounts)  
✅ **Batch Processing**: Process multiple accounts with rate limiting to avoid RPC throttling  
✅ **Dry-Run Mode**: Test reclaim logic without sending transactions  
✅ **Safety Features**: Whitelists, blacklists, minimum inactive periods, confirmation prompts  
✅ **Comprehensive Logging**: Detailed logs with tracing for transparency  
✅ **Statistics & Reporting**: Track total reclaimed, view history, exportto JSON  

## Prerequisites

- Rust 1.70+  (install from [rust-lang.org](https://www.rust-lang.org/))
- Solana CLI tools (optional, for keypair generation)
- Access to a Solana RPC endpoint (devnet or mainnet)
- Your Kora node's fee payer public key
- Treasury wallet keypair (for receiving reclaimed SOL)

## Quick Start

### 1. Clone and Build

```bash
cd /path/to/korabot
cargo build --release
```

### 2. Configure

Copy the sample configuration and edit with your details:

```bash
cp config.toml my-config.toml
```

Edit `my-config.toml`:

```toml
[solana]
rpc_url = "https://api.devnet.solana.com"  # Use your RPC endpoint
network = "Devnet"  # Or "Mainnet"
commitment = "confirmed"

[kora]
# Your Kora node's fee payer pubkey (from signers.toml)
operator_pubkey = "YOUR_KORA_OPERATOR_PUBKEY_HERE"

# Treasury wallet where reclaimed SOL goes
treasury_wallet = "YOUR_TREASURY_WALLET_PUBKEY_HERE"

# Path to treasury keypair JSON file
treasury_keypair_path = "./treasury-keypair.json"

[reclaim]
# Minimum days account must be inactive before reclaim
min_inactive_days = 30

# Enable dry-run by default (safe for testing)
dry_run = true

# Batch processing settings
batch_size = 10
batch_delay_ms = 1000
```

### 3. Initialize Database

```bash
cargo run --release -- init
```

### 4. Test with Dry-Run

Scan for eligible accounts without sending transactions:

```bash
cargo run --release -- scan --verbose --dry-run
```

## Usage

### Scan for Eligible Accounts

```bash
# Basic scan
cargo run -- scan

# Verbose output with details
cargo run -- scan --verbose

# Limit number of transactions to scan
cargo run -- scan --limit 1000
```

### Reclaim from Specific Account

```bash
# With confirmation prompt
cargo run -- reclaim ACCOUNT_PUBKEY_HERE

# Auto-confirm (skip prompt)
cargo run --reclaim ACCOUNT_PUBKEY_HERE --yes

# Dry-run (simulate without sending)
cargo run -- reclaim ACCOUNT_PUBKEY_HERE --dry-run
```

### Run Automated Service

```bash
# Check every hour (3600 seconds)
cargo run -- auto --interval 3600

# Dry-run mode
cargo run -- auto --interval 3600 --dry-run
```

Press `Ctrl+C` to stop the service.

### View Statistics

```bash
# Table format (default)
cargo run -- stats

# JSON format
cargo run -- stats --format json
```

## Configuration Reference

### Solana Settings

- `rpc_url`: Solana RPC endpoint URL
- `network`: "Mainnet", "Devnet", or "Testnet"
- `commitment`: "processed", "confirmed", or "finalized"
- `rate_limit_delay_ms`: Delay between RPC calls (milliseconds)

### Kora Settings

- `operator_pubkey`: Public key of Kora fee payer (accounts sponsored by this wallet)
- `treasury_wallet`: Destination for reclaimed SOL
- `treasury_keypair_path`: Path to keypair JSON file (signs reclaim transactions)

### Reclaim Settings

- `min_inactive_days`: Minimum days account must be inactive (default: 30)
- `dry_run`: If true, simulate without sending transactions (default: true)
- `batch_size`: Number of accounts per batch (default: 10)
- `batch_delay_ms`: Delay between batches (default: 1000)
- `scan_interval_seconds`: How often to scan in auto mode (default: 3600)
- `whitelist`: Array of account pubkeys to NEVER reclaim (protected)
- `blacklist`: Array of account pubkeys to exclude

### Database Settings

- `path`: SQLite database file path (default: "./kora_reclaim.db")

## How It Works

1. **Discovery**: The bot scans your Kora operator's transaction history on Solana to find sponsored account creations
2. **Eligibility**: Checks each account against criteria:
   - Account is closed (doesn't exist) OR
   - Account is empty (no data, only rent-exempt balance) AND inactive (no recent transactions)
   - Account is not whitelisted or blacklisted
   - Minimum inactive period has passed
3. **Reclaim**: Builds appropriate transactions:
   - System accounts: Transfer all lamports to treasury
   - SPL Token accounts: Close account instruction
4. **Safety**: Dry-run mode, confirmation prompts, rate limiting, comprehensive logging

## Troubleshooting

**"Failed to load configuration"**  
- Ensure `config.toml` exists in the current directory
- Check TOML syntax is valid

**"Invalid operator pubkey"**  
- Verify the pubkey is a valid base58-encoded Solana public key

**"Failed to read keypair file"**  
- Ensure treasury keypair file exists at specified path
- File should be JSON array of 64 bytes (standard Solana keypair format)

**"RPC rate limit errors"**  
- Increase `rate_limit_delay_ms` in config
- Consider using a paid RPC service (Helius, QuickNode, etc.)

**"No eligible accounts found"**  
- Most accounts are likely still active or within minimum inactive period
- Try lowering `min_inactive_days` (with caution)
- Check that operator pubkey is correct

## Development

Build in debug mode:

```bash
cargo build
```

Run tests:

```bash
cargo test
```

Enable verbose logging:

```bash
RUST_LOG=kora_reclaim=debug cargo run -- scan
```

## Safety & Best Practices

1. **Always test on devnet first** before using on mainnet
2. **Enable dry-run mode** initially (`dry_run = true` in config)
3. **Use whitelists** to protect important accounts
4. **Set reasonable `min_inactive_days`** (30+ recommended)
5. **Monitor logs** for any unexpected behavior
6. **Backup your treasury keypair** securely

## Architecture

```
src/
├── solana/       # Solana RPC client, account discovery, rent calculations
├── kora/         # Kora account monitoring and types
├── reclaim/      # Eligibility checking, reclaim engine, batch processing
├── storage/      # SQLite database for tracking accounts and operations
├── cli/          # Command-line interface definitions
├── config.rs     # Configuration loading and validation
├── error.rs      # Error types and handling
├── utils.rs      # Utility functions (formatting, rate limiting, etc.)
└── main.rs       # Main application entry point and command implementations
```

## License

[Your License Here]

## Support

For issues or questions:
- Check the logs for detailed error information
- Review this documentation
- Open an issue on GitHub (if applicable)

---

**Important**: This bot directly interacts with Solana accounts and sends transactions. Always understand what it does before running on mainnet with real SOL.
