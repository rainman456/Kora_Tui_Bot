# Kora Rent-Reclaim Bot

Automated rent reclamation for Kora-sponsored Solana accounts.

## Overview

This bot monitors, tracks, and reclaims SOL locked as rent in accounts created through Kora transaction sponsorship. It provides operators with visibility into sponsored accounts and automates the recovery of rent from closed or inactive accounts.

**What it does:**

- Discovers all accounts sponsored by your Kora node
- Monitors account status and eligibility for rent reclamation
- Safely reclaims SOL from closed, empty, or inactive accounts
- Provides audit trails, statistics, and reporting
- Includes safety mechanisms: dry-run mode, whitelist/blacklist, minimum inactive periods

**Why it exists:**

Kora sponsors account creation on Solana by paying rent upfront. When sponsored accounts are closed or become unused, that rent remains locked unless explicitly reclaimed. Most operators don't track which accounts they sponsored or when accounts become eligible for reclamation, resulting in capital locked in forgotten accounts.

This bot solves that operational problem.

## How Kora Works

Kora enables apps to sponsor transactions and account creation on Solana. When Kora sponsors account creation:

1. The Kora fee payer wallet pays the rent-exempt minimum upfront
2. That SOL becomes locked in the created account
3. The SOL remains locked until the account is explicitly closed

On Solana, every account must maintain a minimum balance (rent-exempt minimum) calculated based on account data size. This rent is locked indefinitely until the account is closed.

For detailed explanations of Kora sponsorship mechanics and rent calculation, see:

- [BOTLOGIC.md](docs/BOTLOGIC.md) - Comprehensive bot logic and architecture
- [docs/](docs/) - Additional documentation and guides

## Build

```bash
cargo build --release
```

The binary will be available at `target/release/kora-reclaim`.

## Configuration

The bot is configured using `config.toml`. See [src/config.rs](src/config.rs) for field definitions and validation logic.

### Example Configuration

```toml
[solana]
rpc_url = "https://api.devnet.solana.com"
network = "Devnet"
commitment = "confirmed"
rate_limit_delay_ms = 100

[kora]
# Kora operator (fee payer) public key from your Kora node's signers.toml
operator_pubkey = "5VVJ18M8TTwCXDNpZRy2YmKEu3V6LSJSxZCBH3FqKkqP"

# Treasury wallet where reclaimed SOL will be sent
treasury_wallet = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"

# Path to treasury keypair JSON (must own this keypair to sign reclaim transactions)
treasury_keypair_path = "./treasury-keypair.json"

[reclaim]
# Minimum days an account must be inactive before reclaim
min_inactive_days = 30

# Enable automatic reclaim mode
auto_reclaim_enabled = false

# Number of accounts to process per batch
batch_size = 10

# Delay between batches (milliseconds)
batch_delay_ms = 1000

# Dry run mode: simulate reclaims without sending transactions
dry_run = true

# Protected accounts (never reclaim)
whitelist = []

# Excluded accounts
blacklist = []

[database]
path = "./kora_reclaim.db"

[telegram]
bot_token = "YOUR_BOT_TOKEN_HERE"
authorized_users = []
notifications_enabled = true
alert_threshold_sol = 0.01
```

### Configuration Notes

- **operator_pubkey**: The Kora fee payer public key. All accounts created in transactions signed by this key are considered sponsored accounts.
- **treasury_wallet**: Destination for reclaimed SOL. You must own the corresponding keypair.
- **treasury_keypair_path**: Path to the treasury keypair JSON file (Solana keypair format: array of 64 bytes).
- **dry_run**: Always start with `dry_run = true` to test without sending transactions.

## Testing the Bot

The bot includes a test client for creating sponsored accounts on a local validator or devnet.

### Test Flow

1. **Navigate to test client:**

```bash
cd kora-test/client
```

2. **Install dependencies:**

```bash
bun install
```

3. **Initialize test environment:**

```bash
bun init-env
```

This generates a Kora keypair and sets up the test environment.

4. **Create test accounts:**

```bash
bun simple
```

This creates sponsored accounts using the generated Kora keypair.

5. **Export Kora keypair:**

```bash
bun export
```

This outputs the Kora public key and private key JSON.

### Extracting Test Configuration

After running the test client:

1. **Get the Kora public key** from `bun export` output
2. **Get the Kora private key JSON** from `bun export` output
3. **Set a custom treasury public key** (your own wallet)
4. **Generate or export your treasury keypair JSON**

Update `config.toml` with these values:

```toml
[kora]
operator_pubkey = "<KORA_PUBLIC_KEY_FROM_EXPORT>"
treasury_wallet = "<YOUR_TREASURY_PUBLIC_KEY>"
treasury_keypair_path = "./treasury-keypair.json"
```

## Bot Logic & Internal Design

For detailed explanations of:

- Account discovery process
- Eligibility checking logic
- Rent reclamation mechanics
- Batch processing and rate limiting
- Safety mechanisms

See [BOTLOGIC.md](docs/BOTLOGIC.md).

For architectural documentation, see the [docs/](docs/) directory.

## Available Commands

### `tui`

Launch the interactive TUI dashboard.

```bash
kora-reclaim tui
```

Provides a visual interface for monitoring accounts, viewing statistics, and managing reclamation.

### `scan`

Scan for eligible accounts.

```bash
# Basic scan
kora-reclaim scan

# Verbose output (show all eligible accounts)
kora-reclaim scan --verbose

# Limit number of accounts to scan
kora-reclaim scan --limit 1000

# Dry-run mode
kora-reclaim scan --dry-run
```

Discovers sponsored accounts and checks eligibility for reclamation.

### `reclaim`

Reclaim rent from a specific account.

```bash
# Reclaim with confirmation prompt
kora-reclaim reclaim <ACCOUNT_PUBKEY>

# Auto-confirm (no prompt)
kora-reclaim reclaim <ACCOUNT_PUBKEY> --yes

# Dry-run (simulate without sending transactions)
kora-reclaim reclaim <ACCOUNT_PUBKEY> --dry-run
```

Executes rent reclamation for a single account.

### `auto`

Run automated reclaim service.

```bash
# Run with default interval (3600 seconds)
kora-reclaim auto

# Custom check interval
kora-reclaim auto --interval 1800

# Dry-run mode
kora-reclaim auto --dry-run
```

Continuously monitors and reclaims eligible accounts at specified intervals.

### `list`

List tracked accounts.

```bash
# List all accounts
kora-reclaim list

# Filter by status (active, closed, reclaimed, all)
kora-reclaim list --status closed

# Output as JSON
kora-reclaim list --format json

# Show detailed information
kora-reclaim list --detailed
```

Displays accounts tracked by the bot with filtering options.

### `stats`

Show statistics and reports.

```bash
# Display statistics table
kora-reclaim stats

# Output as JSON
kora-reclaim stats --format json

# Show only total reclaimed amount
kora-reclaim stats --total
```

Provides summary statistics on reclaimed accounts and SOL.

### `checkpoints`

Show checkpoint information and scanning state.

```bash
kora-reclaim checkpoints
```

Displays the current scanning checkpoint and state information.

### `reset`

Reset scanning checkpoints (force full rescan on next run).

```bash
# Reset with confirmation prompt
kora-reclaim reset

# Auto-confirm
kora-reclaim reset --yes
```

Clears scanning checkpoints to force a complete rescan.

### `passive-check`

Perform a passive eligibility check without reclaiming.

```bash
kora-reclaim passive-check
```

Checks account eligibility without executing reclamation transactions.

### `daily-summary`

Generate a daily summary report.

```bash
kora-reclaim daily-summary
```

Produces a summary of daily reclamation activity.

### `init`

Initialize database and configuration.

```bash
kora-reclaim init
```

Sets up the database and validates configuration. Run this before first use.

### `telegram`

Start Telegram bot interface.

```bash
kora-reclaim telegram
```

Launches the Telegram bot for remote monitoring and control.

## Global Options

All commands support:

- `--config <PATH>` - Path to configuration file (default: `config.toml`)

Example:

```bash
kora-reclaim --config my-config.toml scan
```
