### How the Bot Identifies Kora-Sponsored Accounts

In simpler terms, the bot doesn't have a magical list handed to it— it has to actively "hunt" for accounts by looking at the blockchain history tied to your Kora node. Kora doesn't provide a built-in API or database that directly lists all sponsored accounts (based on the docs and code), so the bot relies on Solana's public ledger and some smart querying. Here's how this logic works, step by step, and how you'd implement it in your Rust bot.

#### 1. **Key Starting Point: The Kora Fee Payer Pubkey**
   - Every Kora node uses a specific Solana keypair (public key + private key) as the "fee payer" for sponsored transactions. This is configured in your `signers.toml` file when setting up Kora (e.g., the "KORA_SIGNER_ADDRESS" mentioned in integration guides).
   - When Kora sponsors a transaction (like creating an account), this fee payer key signs it and funds the rent. So, any account created in those transactions is "sponsored" by Kora.
   - In your bot: The operator (you) provides this pubkey in a config file (e.g., via env var or TOML). This is the anchor for everything.

#### 2. **Querying Transaction History via Solana RPC**
   - Solana's blockchain is transparent, so the bot uses JSON RPC calls to fetch all transactions where your Kora fee payer was involved.
   - Main RPC methods:
     - **`getSignaturesForAddress`**: This lists transaction signatures (IDs) for the fee payer pubkey. It shows all txns the fee payer signed (i.e., sponsored). You can paginate with `before`/`after` params to go back in time or fetch new ones periodically.
     - Limit: Up to 1,000 signatures per call, so the bot loops with pagination.
     - Example: Fetch txns since last check to avoid re-scanning everything.
   - Why this works: Sponsored txns are those paid by Kora's fee payer, including account creations.

#### 3. **Parsing Transactions for Created Accounts**
   - For each signature, call **`getTransaction`** to get the full txn details (instructions, accounts involved).
   - Scan the instructions for account creation:
     - **System Program creations**: Look for `system_instruction::create_account` or `allocate`. The new account pubkey is in the instruction args, and the funder is usually the fee payer (your Kora key).
     - **SPL Token Accounts**: Common in Kora (e.g., for user tokens). Check for `spl_token::instruction::initialize_account` or `spl_associated_token_account::create`. The new token account pubkey is extracted here.
     - **Other programs**: If Kora sponsors custom programs, parse their instructions if they create accounts.
   - Collect the new pubkeys: Add them to a list or database (e.g., in-memory vec or SQLite for persistence) as "sponsored accounts." Track metadata like creation timestamp.
   - Edge cases: Skip if the txn failed (check `meta.err` in getTransaction). Ignore if the account wasn't funded with rent (balance check).

#### 4. **Efficiency and Monitoring**
   - **Periodic Scans**: Run this as a cron job or loop (e.g., every hour) to fetch new txns only (use `until` param in getSignaturesForAddress for timestamps).
   - **Subscriptions for Real-Time**: Use WebSocket RPC like `accountSubscribe` on the fee payer, but for history, polling is key. For new creations, subscribe to logs or signatures.
   - **Filtering by Program**: For optimization, use **`getProgramAccounts`** with filters (e.g., memcmp on data offsets if accounts store creator info, like in stake or lookup tables). But for general sponsored accounts, txn parsing is more reliable.
   - **Kora-Specific Hooks**: Kora logs events (via tracing) and exposes Prometheus metrics for tx processing. Your bot could parse Kora's logs (if you redirect them to a file) for sponsored txn signatures, or scrape metrics for counts. Redis caching in Kora might help track usage, but not accounts directly.

#### 5. **Implementation in Your Rust Bot**
   - Using `solana-client` crate:
     ```rust
     use solana_client::rpc_client::RpcClient;
     use solana_sdk::{pubkey::Pubkey, signature::Signature};
     use std::str::FromStr;

     async fn get_sponsored_accounts(client: &RpcClient, fee_payer: &Pubkey) -> Vec<Pubkey> {
         let mut sponsored = Vec::new();
         let sigs = client.get_signatures_for_address(fee_payer).unwrap();  // Paginate in real code
         for sig in sigs {
             if let Ok(tx) = client.get_transaction(&Signature::from_str(&sig.signature).unwrap(), None) {
                 // Parse tx.message.instructions for create_account types
                 for instr in &tx.transaction.message.instructions {
                     if instr.program_id == solana_sdk::system_program::id() {
                         // Check if create_account (opcode 0), extract new account (accounts[1])
                         // Add to sponsored if matches
                     }
                     // Similar for SPL: if program_id == spl_token::id(), check initialize_account
                 }
             }
         }
         sponsored
     }
     ```
   - In the dashboard (Ratatui): Show a table of these accounts, with status (e.g., "Scanning txns... Found 42 sponsored accounts").
   - Safety: Cache the list to avoid re-querying everything. Handle rate limits (Solana RPC can throttle).

#### 6. **Challenges and Tips**
   - **Scale**: If your Kora node sponsors thousands of txns, parsing could be slow—use a database to store historical data and only fetch deltas.
   - **Accuracy**: Not every sponsored txn creates accounts; filter for those that do. Confirm rent was locked (check initial balance == rent-exempt min).
   - **Testing**: On devnet, sponsor some creations via Kora CLI/SDK, then run the bot to detect them.
   - **No Direct Kora API**: Docs don't mention a query endpoint for this, so Solana RPC is the way. If you modify Kora's code (it's open-source on GitHub), add custom logging for creations.

This approach makes the bot reliable and automated, fitting the bounty's goal of monitoring without manual work. If you hit limits, consider a paid RPC like Helius for faster queries.