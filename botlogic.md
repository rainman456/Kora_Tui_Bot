### Identification of Kora-Sponsored Accounts

Kora does not expose a dedicated API or registry that enumerates all sponsored accounts. As a result, identification of such accounts must be derived from on-chain data. The bot determines which accounts were sponsored by Kora by analyzing Solana transaction history associated with the Kora fee payer. This approach relies entirely on publicly available ledger data and standard Solana RPC methods.

#### 1. Kora Fee Payer as the Source of Truth

Each Kora node is configured with a Solana keypair that acts as the fee payer for all sponsored transactions. This keypair is defined during Kora setup, typically in `signers.toml`, and is referenced in integration documentation as the Kora signer or fee payer address.

When Kora sponsors a transaction—such as account creation—the fee payer key signs the transaction and covers rent and fees. Any account created within a transaction funded by this key can therefore be attributed to Kora sponsorship. The bot treats this public key as the primary anchor for discovery and expects it to be provided via configuration (for example, through an environment variable or TOML file).

#### 2. Transaction Discovery via Solana RPC

The bot queries the Solana blockchain using JSON RPC to retrieve transactions involving the Kora fee payer.

The primary RPC method used is `getSignaturesForAddress`, which returns transaction signatures associated with the fee payer public key. These signatures correspond to transactions sponsored or signed by Kora. The RPC response is limited to a maximum of 1,000 signatures per request, so pagination is required for historical scans.

Pagination parameters such as `before` and `until` are used to traverse older transactions or to incrementally fetch new ones since the last scan. This allows the bot to avoid reprocessing previously analyzed data.

This method is effective because any sponsored account creation must occur within a transaction paid for by the Kora fee payer.

#### 3. Transaction Parsing and Account Extraction

For each transaction signature, the bot retrieves full transaction details using `getTransaction`. The transaction message and instruction set are then analyzed to identify account creation events.

Common patterns include:

* **System Program account creation**
  Instructions such as `create_account` or `allocate` from the System Program indicate the creation of a new account. The newly created account public key is contained within the instruction’s account list, and the funding account is typically the Kora fee payer.

* **SPL Token accounts**
  Sponsored transactions frequently create token accounts. These are identified by instructions such as `initialize_account` (SPL Token Program) or `create` (Associated Token Account Program). The initialized token account public key is extracted accordingly.

* **Custom program accounts**
  If Kora sponsors transactions for custom programs that create accounts, their instructions must be parsed according to program-specific layouts.

Each discovered account is recorded as a sponsored account, along with relevant metadata such as creation time and transaction signature. Transactions that failed (as indicated by a non-null `meta.err`) are ignored. Additional validation, such as verifying rent-exempt balances, can be applied to improve accuracy.

#### 4. Scanning Strategy and Performance Considerations

The bot is designed to operate incrementally rather than performing full historical scans on every run.

* **Periodic polling**
  The bot periodically queries for new signatures and processes only transactions that have not yet been analyzed.

* **Real-time monitoring**
  For near real-time detection, WebSocket subscriptions (such as log or signature subscriptions) may be used in addition to polling. Polling remains necessary for historical coverage.

* **Program-based filtering**
  While methods such as `getProgramAccounts` can be used to filter certain account types, transaction-level parsing remains the most reliable general approach for identifying sponsored accounts.

* **Kora runtime signals**
  Kora emits logs and Prometheus metrics related to transaction processing. If available, these can be used as auxiliary signals (for example, extracting transaction signatures from logs), but they are not sufficient on their own to enumerate sponsored accounts.

#### 5. Rust Implementation Overview

The bot uses the `solana-client` crate to interact with the Solana RPC API. At a high level, the implementation performs the following steps:

* Fetch transaction signatures for the Kora fee payer
* Retrieve and parse each transaction
* Identify account creation instructions
* Persist discovered account public keys for future reference

Example structure:

```rust
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::str::FromStr;

async fn get_sponsored_accounts(client: &RpcClient, fee_payer: &Pubkey) -> Vec<Pubkey> {
    let mut sponsored = Vec::new();
    let sigs = client.get_signatures_for_address(fee_payer).unwrap();

    for sig in sigs {
        if let Ok(tx) = client.get_transaction(
            &Signature::from_str(&sig.signature).unwrap(),
            None,
        ) {
            for instr in &tx.transaction.message.instructions {
                // Inspect program_id and instruction data
                // Detect account creation and extract new account pubkeys
            }
        }
    }

    sponsored
}
```

Discovered accounts can be stored in memory or persisted to disk using a lightweight database such as SQLite to support incremental scanning and restarts.

In the Ratatui dashboard, this data can be presented as a table showing the total number of sponsored accounts, recent discoveries, and scan status.

#### 6. Limitations and Considerations

* **Scalability**
  Large volumes of sponsored transactions can make full scans expensive. Persisting state and processing only deltas is essential for performance.

* **False positives**
  Not every sponsored transaction creates an account. Instruction-level filtering and balance checks help reduce noise.

* **Testing**
  Validation is best performed on devnet by sponsoring known account creations through Kora and verifying detection.

* **Lack of native Kora APIs**
  As of the current Kora documentation and source code, no endpoint exists for querying sponsored accounts directly. On-chain analysis via Solana RPC remains the authoritative method unless Kora is extended with custom logging or indexing.

This design enables automated and verifiable tracking of Kora-sponsored accounts using only public blockchain data, aligning with the requirements of long-running monitoring and minimal manual intervention.
