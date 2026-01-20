use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    config::Config,
    storage::{db::Database, models::SponsoredAccount},
    error::Result,
    solana::client::SolanaRpcClient,
    kora::monitor::KoraMonitor,
    reclaim::{
        engine::ReclaimEngine,
        eligibility::EligibilityChecker,
    },
};
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use solana_client::rpc_client::CommitmentConfig;

pub struct App {
    // UI State
    pub current_screen: Screen,
    pub input_mode: InputMode,
    pub should_quit: bool,
    pub stats: AppStats,
    pub accounts: Vec<SponsoredAccount>,
    pub selected_account_index: usize,
    pub logs: Vec<LogEntry>,
    pub operations: Vec<OperationEntry>,
    pub is_loading: bool,
    pub status_message: Option<String>,
    
    // Configuration
    pub config: Config,
    
    // Database
    pub db: Arc<Mutex<Database>>,
    
    // Bot Components (Core Integration)
    solana_client: SolanaRpcClient,
    kora_monitor: KoraMonitor,
    reclaim_engine: ReclaimEngine,
    eligibility_checker: EligibilityChecker,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let db = Arc::new(Mutex::new(Database::new(&config.database.path)?));
        
        //  Initialize Solana client
        let commitment = CommitmentConfig::confirmed();
        let solana_client = SolanaRpcClient::new(
            &config.solana.rpc_url,
            commitment,
        );
        
        //  Initialize Kora monitor
        let operator_pubkey = config.operator_pubkey()?;
        let kora_monitor = KoraMonitor::new(
            solana_client.clone(),
            operator_pubkey,
        );
        
        //  Initialize reclaim engine
        let treasury_wallet = config.treasury_wallet()?;
        // TODO: Load keypair from secure location
        let keypair = load_keypair()?;
        let reclaim_engine = ReclaimEngine::new(
            solana_client.clone(),
            treasury_wallet,
            keypair,
        );
        
        //  Initialize eligibility checker
        let eligibility_checker = EligibilityChecker::new(
            solana_client.clone(),
            config.clone(),
        );
        
        Ok(Self {
            current_screen: Screen::Dashboard,
            input_mode: InputMode::Normal,
            should_quit: false,
            config,
            db,
            stats: AppStats::default(),
            accounts: Vec::new(),
            selected_account_index: 0,
            logs: Vec::new(),
            operations: Vec::new(),
            is_loading: false,
            status_message: None,
            // Bot components
            solana_client,
            kora_monitor,
            reclaim_engine,
            eligibility_checker,
        })
    }
    
    // ═══════════════════════════════════════════════════════════
    //  BOT INTEGRATION METHODS
    // ═══════════════════════════════════════════════════════════
    
    /// Scan for Kora-sponsored accounts
    pub async fn scan_accounts(&mut self) -> Result<()> {
        self.is_loading = true;
        self.add_log(LogLevel::Info, "Scanning for sponsored accounts...".to_string());
        
        //  Call bot's KoraMonitor
        match self.kora_monitor.get_sponsored_accounts().await {
            Ok(sponsored_accounts) => {
                let count = sponsored_accounts.len();
                
                // Update app state
                let db = self.db.lock().await;
                for account in &sponsored_accounts {
                    // Save to database
                    let model = crate::storage::models::SponsoredAccount {
                        pubkey: account.pubkey.to_string(),
                        created_at: chrono::DateTime::from_timestamp(account.created_at, 0)
                            .unwrap_or(chrono::Utc::now()),
                        closed_at: None,
                        rent_lamports: account.rent_lamports,
                        data_size: account.data_size,
                        status: crate::storage::models::AccountStatus::Active,
                    };
                    db.save_account(&model)?;
                }
                
                self.add_log(
                    LogLevel::Success,
                    format!("Found {} sponsored accounts", count),
                );
            }
            Err(e) => {
                self.add_log(
                    LogLevel::Error,
                    format!("Scan failed: {}", e),
                );
            }
        }
        
        self.is_loading = false;
        self.refresh_data().await?;
        Ok(())
    }
    
    /// Check eligibility for all accounts
    pub async fn check_eligibility(&mut self) -> Result<()> {
        self.is_loading = true;
        self.add_log(LogLevel::Info, "Checking account eligibility...".to_string());
        
        let mut eligible_count = 0;
        
        for account in &self.accounts {
            let pubkey = Pubkey::try_from(account.pubkey.as_str())
                .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
            
            //  Call bot's EligibilityChecker
            let is_eligible = self.eligibility_checker
                .is_eligible(&pubkey, account.created_at)
                .await?;
            
            if is_eligible {
                eligible_count += 1;
            }
        }
        
        self.stats.eligible_accounts = eligible_count;
        
        self.add_log(
            LogLevel::Success,
            format!("Found {} eligible accounts for reclaim", eligible_count),
        );
        
        self.is_loading = false;
        Ok(())
    }
    
    /// Reclaim rent from selected account
    pub async fn reclaim_selected_account(&mut self) -> Result<()> {
        if self.accounts.is_empty() {
            self.add_log(LogLevel::Warning, "No accounts available".to_string());
            return Ok(());
        }
        
        let account = &self.accounts[self.selected_account_index];
        let pubkey = Pubkey::try_from(account.pubkey.as_str())
            .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
        
        self.is_loading = true;
        self.add_log(
            LogLevel::Info,
            format!("Reclaiming rent from {}...", account.pubkey),
        );
        
        //  Call bot's ReclaimEngine
        match self.reclaim_engine.reclaim_account(&pubkey).await {
            Ok(signature) => {
                // Save operation to database
                let db = self.db.lock().await;
                let operation = crate::storage::models::ReclaimOperation {
                    id: 0, // Auto-generated
                    account_pubkey: account.pubkey.clone(),
                    reclaimed_amount: account.rent_lamports,
                    tx_signature: signature.clone(),
                    timestamp: chrono::Utc::now(),
                    reason: "Manual reclaim via TUI".to_string(),
                };
                db.save_reclaim_operation(&operation)?;
                
                self.add_log(
                    LogLevel::Success,
                    format!("Reclaimed {} lamports. Tx: {}", account.rent_lamports, signature),
                );
                
                self.status_message = Some(format!("Reclaimed successfully: {}", signature));
            }
            Err(e) => {
                self.add_log(
                    LogLevel::Error,
                    format!("Reclaim failed: {}", e),
                );
                self.status_message = Some(format!("Reclaim failed: {}", e));
            }
        }
        
        self.is_loading = false;
        self.refresh_data().await?;
        Ok(())
    }
    
    /// Batch reclaim all eligible accounts
    pub async fn batch_reclaim_eligible(&mut self) -> Result<()> {
        self.is_loading = true;
        self.add_log(LogLevel::Info, "Starting batch reclaim...".to_string());
        
        let mut eligible_pubkeys = Vec::new();
        
        // Find eligible accounts
        for account in &self.accounts {
            let pubkey = Pubkey::try_from(account.pubkey.as_str())
                .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
            
            if self.eligibility_checker
                .is_eligible(&pubkey, account.created_at)
                .await?
            {
                eligible_pubkeys.push(pubkey);
            }
        }
        
        if eligible_pubkeys.is_empty() {
            self.add_log(LogLevel::Warning, "No eligible accounts found".to_string());
            self.is_loading = false;
            return Ok(());
        }
        
        self.add_log(
            LogLevel::Info,
            format!("Reclaiming {} accounts...", eligible_pubkeys.len()),
        );
        
        //  Call bot's batch reclaim
        let results = self.reclaim_engine.batch_reclaim(&eligible_pubkeys).await?;
        
        let mut success_count = 0;
        let mut failed_count = 0;
        
        let db = self.db.lock().await;
        for (pubkey, result) in results {
            match result {
                Ok(signature) => {
                    success_count += 1;
                    
                    // Save operation
                    let operation = crate::storage::models::ReclaimOperation {
                        id: 0,
                        account_pubkey: pubkey.to_string(),
                        reclaimed_amount: 0, // TODO: Get actual amount
                        tx_signature: signature.clone(),
                        timestamp: chrono::Utc::now(),
                        reason: "Batch reclaim via TUI".to_string(),
                    };
                    db.save_reclaim_operation(&operation)?;
                }
                Err(e) => {
                    failed_count += 1;
                    self.add_log(
                        LogLevel::Warning,
                        format!("Failed to reclaim {}: {}", pubkey, e),
                    );
                }
            }
        }
        
        self.add_log(
            LogLevel::Success,
            format!("Batch complete: {} succeeded, {} failed", success_count, failed_count),
        );
        
        self.is_loading = false;
        self.refresh_data().await?;
        Ok(())
    }
    
    /// Refresh account states from blockchain
    pub async fn sync_account_states(&mut self) -> Result<()> {
        self.is_loading = true;
        self.add_log(LogLevel::Info, "Syncing account states...".to_string());
        
        let db = self.db.lock().await;
        
        for account in &mut self.accounts {
            let pubkey = Pubkey::try_from(account.pubkey.as_str())
                .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
            
            //  Check if account is still active on-chain
            let is_active = self.solana_client.is_account_active(&pubkey)?;
            
            if !is_active && account.status == crate::storage::models::AccountStatus::Active {
                // Account was closed
                account.status = crate::storage::models::AccountStatus::Closed;
                account.closed_at = Some(chrono::Utc::now());
                
                // Update in database
                db.save_account(account)?;
                
                self.add_log(
                    LogLevel::Info,
                    format!("Account {} is now closed", account.pubkey),
                );
            }
        }
        
        self.add_log(LogLevel::Success, "Sync complete".to_string());
        self.is_loading = false;
        Ok(())
    }
    
    /// Get detailed eligibility reason for selected account
    pub async fn get_selected_eligibility_reason(&self) -> Result<String> {
        if self.accounts.is_empty() {
            return Ok("No account selected".to_string());
        }
        
        let account = &self.accounts[self.selected_account_index];
        let pubkey = Pubkey::try_from(account.pubkey.as_str())
            .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
        
        //  Get detailed reason from eligibility checker
        self.eligibility_checker
            .get_eligibility_reason(&pubkey, account.created_at)
            .await
    }
    
    // ... existing methods (refresh_data, add_log, etc.)
}

// Helper function to load keypair
fn load_keypair() -> Result<Keypair> {
    // TODO: Implement secure keypair loading
    // For now, return a placeholder
    use std::env;
    
    let private_key = env::var("KORA_WALLET_PRIVATE_KEY")
        .map_err(|_| crate::error::ReclaimError::Config(
            "KORA_WALLET_PRIVATE_KEY not set".to_string()
        ))?;
    
    let bytes = bs58::decode(private_key)
        .into_vec()
        .map_err(|e| crate::error::ReclaimError::Config(
            format!("Invalid private key: {}", e)
        ))?;
    
    Keypair::from_bytes(&bytes)
        .map_err(|e| crate::error::ReclaimError::Config(
            format!("Failed to load keypair: {}", e)
        ))
}