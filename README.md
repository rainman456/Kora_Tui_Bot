
The Kora Rent-Reclaim Bot is a Rust-based utility for identifying and reclaiming rent from Solana accounts sponsored by a Kora node.

## Purpose

The system is designed to recover SOL locked as rent in accounts created or funded by a Kora node. It operates by analyzing on-chain transaction data associated with the Kora fee payer and applying configurable eligibility rules before performing any reclaim actions.

## Functional Overview

The bot follows a multi-stage workflow:

1. **Account Discovery**
   The Solana transaction history of the Kora operator’s fee payer is queried to identify transactions that resulted in account creation. Accounts created via the System Program or SPL Token instructions are recorded as Kora-sponsored accounts.

2. **Eligibility Evaluation**
   Discovered accounts are evaluated against a set of criteria, including:

   * The account is closed or contains no meaningful data
   * The account has been inactive for a configurable minimum period (default: 30 days)
   * The account is not present in whitelist or blacklist rules

3. **Rent Reclamation**
   For accounts that meet all eligibility conditions, the bot constructs and submits transactions to close the account and transfer any recoverable lamports to a configured treasury wallet. Safety mechanisms such as dry-run execution and confirmation prompts are supported.

4. **Batch-Oriented Processing**
   Reclaim operations are executed in batches with configurable delays to reduce the risk of RPC rate limiting.

## Implementation Details

The bot relies on standard Solana RPC interfaces to perform its operations:

* `getSignaturesForAddress` is used to retrieve transactions associated with the Kora fee payer
* `getTransaction` is used to inspect transaction instructions and extract created account public keys
* Discovered accounts and their metadata are persisted in a local SQLite database
* Periodic scans re-evaluate stored accounts and trigger reclaim actions when eligibility criteria are met

## Notable Capabilities

* Dry-run execution for non-destructive testing
* Command-line interface and terminal-based dashboard built with Ratatui
* Automated service mode with configurable scan intervals
* Statistics and reporting with optional JSON export
* Rate limiting, structured logging, and safety controls

## Code Organization

The codebase is structured into logical modules covering command-line interfaces, Solana RPC interactions, eligibility and reclaim logic, persistent storage, and a terminal user interface for monitoring and control.
