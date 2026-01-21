This is a **Kora Rent-Reclaim Bot** — a Rust tool for automatically reclaiming rent from Solana accounts sponsored by a Kora node.

### What It Does

The bot identifies and recovers SOL locked as rent in accounts created/sponsored by your Kora node. Here's the workflow:

1. **Account Discovery**: Queries the Solana blockchain transaction history of your Kora operator's fee-payer wallet to find all accounts it sponsored (created via system program or SPL token instructions)

2. **Eligibility Checking**: Filters accounts by criteria:
   - Account is closed/empty 
   - Inactive for a minimum period (configurable, default 30 days)
   - Not whitelisted/blacklisted

3. **Automated Reclaim**: Sends transactions to close eligible accounts and transfer rent to your treasury wallet (with dry-run testing and confirmation prompts for safety)

4. **Batch Processing**: Handles multiple accounts with rate limiting to avoid RPC throttling

### How It Works

The bot uses Solana RPC calls to:
- Call `getSignaturesForAddress` on your Kora fee-payer to fetch all sponsored transactions
- Parse each transaction with `getTransaction` to extract created account pubkeys
- Track these in a SQLite database
- Periodically scan for eligibility and execute reclaim transactions

### Key Features

- ✅ Dry-run mode for safe testing
- ✅ Comprehensive CLI and TUI (terminal UI using Ratatui)
- ✅ Automatic service with configurable intervals
- ✅ Statistics/reporting with JSON export
- ✅ Rate limiting, logging, and safety features

The codebase is organized into modules: CLI commands, Solana client interactions, reclaim logic, storage (database), and a full terminal UI dashboard.