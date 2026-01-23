### Simplified Explanation of the Task

 Imagine you're running a service called Kora on the Solana blockchain. Kora helps apps pay for users' transactions and create accounts (like digital wallets or storage spots) so users don't have to pay themselves—this makes things smoother and more user-friendly.

But here's the catch: When Kora creates these accounts, it has to "lock up" some SOL (Solana's cryptocurrency) as "rent." Think of rent like a deposit you pay to keep the account alive on the blockchain. Over time, many of these accounts get closed, abandoned, or just aren't needed anymore. If you (as the Kora operator) don't notice and reclaim that locked SOL, it's like losing money quietly because it's stuck there forever.

The task is to build a smart "bot" (an automated program) that:
- Watches all the accounts your Kora node (your server running Kora) has created and sponsored.
- Spots when an account is dead (closed) or safe to clean up (no longer in use).
- Automatically grabs back the locked SOL rent and sends it to your main wallet (called the "operator treasury").

The goal isn't just to automate this—it's also to make it transparent. The bot should show you reports like "Hey, we reclaimed X SOL from this account because it was closed," so you understand what's happening and don't lose money accidentally.





### What Exactly Are We Building

Based on the requirements, here's the core of what we're building:
1. **The Bot Core (in Rust)**: This is the brain. It needs to:
   - Connect to your Kora node and Solana blockchain to list and monitor sponsored accounts.
   - Check each account's status (e.g., is it closed? Is it empty and unused?).
   - If eligible, send a transaction to reclaim the rent SOL safely back to your treasury wallet.
   - Handle safety: Don't reclaim active accounts by mistake—maybe use filters or whitelists.
   - Log everything clearly: What was reclaimed, why, and when.

2. **Dashboard (using Ratatui)**: A terminal user interface (TUI) where you can see real-time info, like:
   - Total locked rent vs. reclaimed rent.
   - List of sponsored accounts with their status (active, closed, reclaimable).
   - Buttons or keys to trigger scans, reclaims, or view logs.
   - Alerts for big idle rents or issues.

3. **Additional Must-Haves**:
   - **Open-Source Code**: Put it on GitHub or similar, with a license like MIT.
   - **README.md**: Explain simply how Kora works (sponsoring txns/accounts), where rent gets locked (during account creation), and how to set up/use your bot.
   - **Prototype**: Get it running on devnet (test network) or mainnet (real one). Test with fake accounts.
   - **Deep-Dive Content**: A blog post, README section, or video walking through your code and why you did things a certain way.
   - **Solo Build**: No team help.

Bonuses if you add: Nice logs, email/Telegram alerts, a report generator, or handling weird edge cases (like network failures or partial closures).

You don't need a fancy web frontend—just the TUI is fine. Focus on making it reliable, safe, and easy to understand.

### How You're Going to Build This (Step-by-Step Plan)

Since you want Rust + Ratatui, we'll use that. Rust is great for blockchain stuff because it's fast, safe, and has good Solana libraries. Ratatui is for building interactive terminal apps (like a dashboard in your command line). Assume you have Rust installed (via rustup.rs). We'll use Solana's Rust SDK for blockchain interactions.

#### Step 1: Set Up Your Project
- Create a new Rust project: `cargo new kora-rent-reclaim-bot`
- Add dependencies in `Cargo.toml`:
  ```toml
  [dependencies]
  solana-client = "1.18"  # For connecting to Solana RPC
  solana-sdk = "1.18"     # For accounts, transactions, rent calcs
  solana-program = "1.18" # If needed for program logic
  ratatui = "0.26"        # For the TUI dashboard
  crossterm = "0.27"      # Backend for Ratatui (handles terminal input/output)
  tokio = { version = "1", features = ["full"] }  # For async (network calls)
  serde = { version = "1", features = ["derive"] }  # For JSON handling
  log = "0.4"             # For logging
  env_logger = "0.10"     # For env-based logging
  # Optional: telegram-bot-raw if you add Telegram alerts later
  ```
- Run `cargo build` to fetch them.

- Read the resources to understand Kora/Solana:
  - Kora docs: Explain how Kora nodes sponsor accounts (they pay rent via a special program).
  - Solana docs: Accounts need minimum rent-exempt balance (calculated via `get_minimum_balance_for_rent_exemption`). When closed, rent is reclaimable via `CloseAccount` instruction.

#### Step 2: Understand Key Concepts (Quick Prep)
- **Solana Rent**: Accounts must hold enough SOL to be "rent-exempt" (no ongoing fees). Use RPC to query account balance and lamports.
- **Kora Sponsorship**: Kora nodes create accounts for users. You'll need your node's sponsor key or API to list sponsored accounts (check Kora docs for endpoints).
- **Reclaiming**: Use Solana's `system_instruction::close_account` to close and transfer lamports back if the account is zero-balance and unused.
- Fetch data via JSON RPC: e.g., `getAccountInfo` for status, `getProgramAccounts` to list accounts owned by Kora program.

#### Step 3: Build the Bot Logic (Core Functionality)
- In `src/main.rs`, set up an async main with Tokio.
- **Monitor Accounts**:
  - Connect to Solana RPC (devnet: "https://api.devnet.solana.com").
  - Use Kora API/docs to get a list of sponsored accounts (e.g., query by sponsor pubkey).
  - Loop periodically: Fetch each account's info (balance, owner, data length).
- **Detect Eligibility**:
  - Closed: Account doesn't exist or balance == 0.
  - Unused: Balance == rent-exempt min, no data, not owned by active programs.
  - Add filters: Skip if recent activity (use `getSignaturesForAddress` for tx history).
- **Reclaim Rent**:
  - Build a transaction: Use `solana_sdk::system_instruction::transfer` or close if applicable.
  - Sign with your treasury keypair (load from file/env).
  - Send via `client.send_and_confirm_transaction`.
- **Safety**: Add config for whitelists (e.g., ignore certain accounts). Log every step. Use dry-run mode to simulate without real txns.
- **Logging/Reporting**: Use `log` crate to output to file/console. Track totals: Locked SOL = sum of rent-exempt mins; Reclaimed = sum of recovered.

Example skeleton code:
```rust
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signer}, system_instruction};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let client = RpcClient::new("https://api.devnet.solana.com");
    let sponsor_pubkey = Pubkey::from_str("YOUR_SPONSOR_PUBKEY")?;
    // Fetch sponsored accounts (implement based on Kora API)
    let accounts = get_sponsored_accounts(&client, &sponsor_pubkey).await?;
    
    for acc in accounts {
        if is_eligible_for_reclaim(&client, &acc).await? {
            reclaim_rent(&client, &acc, &your_treasury_keypair).await?;
            log::info!("Reclaimed from {}", acc);
        }
    }
    Ok(())
}

// Implement helper functions: get_sponsored_accounts, is_eligible_for_reclaim, reclaim_rent
```

#### Step 4: Add the Ratatui Dashboard
- Create a TUI loop: Use Crossterm for events (key presses).
- Display sections: Table for accounts, gauges for locked/reclaimed SOL, log pane.
- Handle inputs: 's' to scan, 'r' to reclaim, 'q' to quit.

Example:
```rust
use ratatui::{backend::CrosstermBackend, Terminal, widgets::{Block, Borders, Table}};
use crossterm::{event::{self, KeyCode}, terminal::{enable_raw_mode, EnterAlternateScreen}};

fn run_dashboard() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            // Draw table of accounts, gauges, etc.
            let block = Block::default().title("Kora Rent Dashboard").borders(Borders::ALL);
            f.render_widget(block, f.size());
            // Add more widgets...
        })?;

        if let event::Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('q') { break; }
            // Handle other keys: scan, reclaim
        }
    }
    Ok(())
}
```
- Integrate: Run the bot logic in a background thread, update dashboard state.

#### Step 5: Test and Polish
- Test on devnet: Create fake sponsored accounts, close some, run bot.
- Add config: TOML file for RPC URL, keys, intervals.
- README: Explain setup (e.g., `cargo run -- --config config.toml`), Kora/rent basics.
- Deep-Dive: Write a Markdown file or record a 5-min video.
- Edge Cases: Handle RPC errors, partial reclaims, large account lists.

#### Step 6: Deploy and Submit
- Run as cron job: `crontab -e` to schedule `cargo run`.
- Make it open-source: GitHub repo with code, README.
- Prototype: Show it working on devnet (screenshots/video).
- Submit: Follow bounty instructions (not specified here, but probably via form/email).

This should take 1-2 weeks if you're familiar with Rust/Solana. If you're new, start with Solana Rust tutorials. If stuck on specifics (e.g., exact Kora API calls), check the docs or ask in Solana communities. Good luck—this sounds like a useful tool!