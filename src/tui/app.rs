use crate::{
    config::Config,
    storage::Database,
    solana::SolanaRpcClient,
    kora::KoraMonitor,
    reclaim::{EligibilityChecker, ReclaimEngine, BatchProcessor},
    error::Result,
};
use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Dashboard,
    Accounts,
    Operations,
    Settings,
}

pub struct App {
    // UI State
    pub current_screen: Screen,
    pub should_quit: bool,
    pub selected_index: usize,
    pub status_message: String,
    pub is_loading: bool,
    
    // Data
    pub total_accounts: usize,
    pub eligible_accounts: usize,
    pub total_locked: u64,
    pub total_reclaimed: u64,
    pub accounts: Vec<AccountDisplay>,
    pub operations: Vec<OperationDisplay>,
    pub logs: Vec<String>,
    
    // Backend
    pub config: Config,
    rpc_client: SolanaRpcClient,
    monitor: KoraMonitor,
    eligibility_checker: EligibilityChecker,
    reclaim_engine: Option<ReclaimEngine>,
    db: Database,

    // Telegram
    pub telegram_enabled: bool,
    pub telegram_configured: bool,
    pub telegram_status: String,
    telegram_notifier: Option<crate::telegram::AutoNotifier>,
}

#[derive(Clone)]
pub struct AccountDisplay {
    pub pubkey: String,
    pub balance: u64,
    pub created: DateTime<Utc>,
    pub status: String,
    pub eligible: bool,
}

#[derive(Clone)]
pub struct OperationDisplay {
    pub timestamp: DateTime<Utc>,
    pub account: String,
    pub amount: u64,
    pub signature: String,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize RPC client
        let rpc_client = SolanaRpcClient::new(
            &config.solana.rpc_url,
            config.commitment_config(),
            config.solana.rate_limit_delay_ms,
        );
        
        // Initialize monitor
        let operator_pubkey = config.operator_pubkey()?;
        let monitor = KoraMonitor::new(rpc_client.clone(), operator_pubkey);
        
        // Initialize eligibility checker
        let eligibility_checker = EligibilityChecker::new(rpc_client.clone(), config.clone());
        
        // Initialize database
        let db = Database::new(&config.database.path)?;
        
        // Try to load reclaim engine (optional - might fail if no keypair)
        let reclaim_engine = match config.load_treasury_keypair() {
            Ok(keypair) => {
                let treasury = config.treasury_wallet()?;
                Some(ReclaimEngine::new(
                    rpc_client.clone(),
                    treasury,
                    keypair,
                    config.reclaim.dry_run,
                ))
            }
            Err(_) => None,
        };
        
        // Initialize Telegram notifier
        let telegram_notifier = crate::telegram::AutoNotifier::new(&config);
        let telegram_configured = config.telegram.is_some();
        let telegram_enabled = telegram_notifier.is_some();
        let telegram_status = if telegram_configured {
            if telegram_enabled {
                "Active".to_string()
            } else {
                "Disabled".to_string()
            }
        } else {
            "Not configured".to_string()
        };
        
        Ok(Self {
            current_screen: Screen::Dashboard,
            should_quit: false,
            selected_index: 0,
            status_message: "Ready".to_string(),
            is_loading: false,
            total_accounts: 0,
            eligible_accounts: 0,
            total_locked: 0,
            total_reclaimed: 0,
            accounts: Vec::new(),
            operations: Vec::new(),
            logs: Vec::new(),
            telegram_enabled,
            telegram_configured,
            telegram_status,
            telegram_notifier,
            config,
            rpc_client,
            monitor,
            eligibility_checker,
            reclaim_engine,
            db,
        })
    }
    
    // Navigation
    pub fn next_screen(&mut self) {
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Accounts,
            Screen::Accounts => Screen::Operations,
            Screen::Operations => Screen::Settings,
            Screen::Settings => Screen::Dashboard,
        };
    }
    
    pub fn previous_screen(&mut self) {
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Settings,
            Screen::Settings => Screen::Operations,
            Screen::Operations => Screen::Accounts,
            Screen::Accounts => Screen::Dashboard,
        };
    }
    
    pub fn next_item(&mut self) {
        let len = if self.current_screen == Screen::Accounts {
            self.accounts.len()
        } else {
            self.operations.len()
        };
        
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
        }
    }
    
    pub fn previous_item(&mut self) {
        let len = if self.current_screen == Screen::Accounts {
            self.accounts.len()
        } else {
            self.operations.len()
        };
        
        if len > 0 {
            if self.selected_index == 0 {
                self.selected_index = len - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }
    
    // Actions
    pub async fn scan_accounts(&mut self) -> Result<()> {
        self.is_loading = true;
        self.add_log("Scanning for sponsored accounts...");
        
        match self.monitor.get_sponsored_accounts(100).await {
            Ok(sponsored) => {
                self.total_accounts = sponsored.len();
                
                // Check eligibility for each
                let mut eligible_count = 0;
                self.accounts.clear();
                
                for account in sponsored {
                    let is_eligible = self.eligibility_checker
                        .is_eligible(&account.pubkey, account.created_at)
                        .await
                        .unwrap_or(false);
                    
                    if is_eligible {
                        eligible_count += 1;
                    }
                    
                    let balance = self.rpc_client.get_balance(&account.pubkey).await.unwrap_or(0);
                    
                    self.accounts.push(AccountDisplay {
                        pubkey: account.pubkey.to_string(),
                        balance,
                        created: account.created_at,
                        status: if is_eligible { "Eligible".to_string() } else { "Active".to_string() },
                        eligible: is_eligible,
                    });
                }
                
                self.eligible_accounts = eligible_count;
                self.add_log(&format!("Found {} accounts, {} eligible", self.total_accounts, eligible_count));
                self.status_message = format!("Scan complete: {} accounts found", self.total_accounts);
                
                // Send Telegram notification
                if let Some(ref notifier) = self.telegram_notifier {
                    notifier.notify_scan_complete(self.total_accounts, eligible_count).await;
                }
            }
            Err(e) => {
                self.add_log(&format!("Scan failed: {}", e));
                self.status_message = format!("Scan failed: {}", e);
                
                // Send error notification
                if let Some(ref notifier) = self.telegram_notifier {
                    notifier.notify_error(&format!("Scan failed: {}", e)).await;
                }
            }
        }
        
        self.is_loading = false;
        Ok(())
    }
    
    pub async fn reclaim_selected(&mut self) -> Result<()> {
        if self.accounts.is_empty() || self.reclaim_engine.is_none() {
            self.status_message = "No account selected or reclaim engine not available".to_string();
            return Ok(());
        }
        
        let account = self.accounts[self.selected_index].clone();
        if !account.eligible {
            self.status_message = "Selected account is not eligible".to_string();
            return Ok(());
        }
        
        self.is_loading = true;
        self.add_log(&format!("Reclaiming from {}...", &account.pubkey[..8]));
        
        let pubkey = Pubkey::try_from(account.pubkey.as_str())
            .map_err(|e| crate::error::ReclaimError::Config(e.to_string()))?;
        
        let engine = self.reclaim_engine.as_ref().unwrap();
        let account_type = crate::kora::AccountType::SplToken;
        
        match engine.reclaim_account(&pubkey, &account_type).await {
            Ok(result) => {
                if let Some(sig) = result.signature {
                    // Save to database
                    let _ = self.db.save_reclaim_operation(&crate::storage::models::ReclaimOperation {
                        id: 0,
                        account_pubkey: account.pubkey.clone(),
                        reclaimed_amount: result.amount_reclaimed,
                        tx_signature: sig.to_string(),
                        timestamp: Utc::now(),
                        reason: "TUI manual reclaim".to_string(),
                    });
                    
                    self.total_reclaimed += result.amount_reclaimed;
                    self.add_log(&format!("✓ Reclaimed {} lamports", result.amount_reclaimed));
                    self.status_message = format!("Reclaimed successfully: {}", &sig.to_string()[..8]);
                    
                    // Send success notification
                    if let Some(ref notifier) = self.telegram_notifier {
                        notifier.notify_reclaim_success(&account.pubkey, result.amount_reclaimed).await;
                        
                        // Check if high-value
                        if let Some(ref tg_config) = self.config.telegram {
                            notifier.notify_high_value_reclaim(
                                &account.pubkey,
                                result.amount_reclaimed,
                                tg_config.alert_threshold_sol
                            ).await;
                        }
                    }
                } else {
                    self.add_log("Dry run - would reclaim");
                    self.status_message = "Dry run completed".to_string();
                }
            }
            Err(e) => {
                self.add_log(&format!("✗ Failed: {}", e));
                self.status_message = format!("Reclaim failed: {}", e);
                
                // Send failure notification
                if let Some(ref notifier) = self.telegram_notifier {
                    notifier.notify_reclaim_failed(&account.pubkey, &e.to_string()).await;
                }
            }
        }
        
        self.is_loading = false;
        Ok(())
    }
    
    pub async fn batch_reclaim(&mut self) -> Result<()> {
        if self.reclaim_engine.is_none() {
            self.status_message = "Reclaim engine not available".to_string();
            return Ok(());
        }
        
        let eligible: Vec<_> = self.accounts.iter()
            .filter(|a| a.eligible)
            .cloned()
            .collect();
        
        if eligible.is_empty() {
            self.status_message = "No eligible accounts found".to_string();
            return Ok(());
        }
        
        self.is_loading = true;
        self.add_log(&format!("Batch reclaiming {} accounts...", eligible.len()));
        
        let engine = self.reclaim_engine.clone().unwrap();
        let batch = BatchProcessor::new(
            engine, 
            self.config.reclaim.batch_size, 
            self.config.reclaim.batch_delay_ms
        );
        
        let eligible_list: Vec<_> = eligible.iter()
            .filter_map(|a| {
                Pubkey::try_from(a.pubkey.as_str()).ok()
                    .map(|pk| (pk, crate::kora::AccountType::SplToken))
            })
            .collect();
        
        match batch.reclaim_all_eligible(eligible_list).await {
            Ok(summary) => {
                self.total_reclaimed += summary.total_reclaimed;
                self.add_log(&format!("Batch complete: {} succeeded, {} failed", summary.successful, summary.failed));
                self.status_message = format!("Batch: {} ok, {} failed", summary.successful, summary.failed);
                
                // Send batch notification
                if let Some(ref notifier) = self.telegram_notifier {
                    let total_sol = crate::solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed);
                    notifier.notify_batch_complete(summary.successful, summary.failed, total_sol).await;
                }
            }
            Err(e) => {
                self.add_log(&format!("Batch failed: {}", e));
                self.status_message = format!("Batch failed: {}", e);
                
                // Send error notification
                if let Some(ref notifier) = self.telegram_notifier {
                    notifier.notify_error(&format!("Batch reclaim failed: {}", e)).await;
                }
            }
        }
        
        self.is_loading = false;
        Ok(())
    }
    
    pub async fn refresh_stats(&mut self) -> Result<()> {
        self.is_loading = true;
        
        // Load from database
        if let Ok(stats) = self.db.get_stats() {
            self.total_accounts = stats.total_accounts;
            self.total_reclaimed = stats.total_reclaimed;
        }
        
        // Load operations
        if let Ok(ops) = self.db.get_reclaim_history(Some(20)) {
            self.operations = ops.into_iter().map(|op| {
                OperationDisplay {
                    timestamp: op.timestamp,
                    account: op.account_pubkey,
                    amount: op.reclaimed_amount,
                    signature: op.tx_signature,
                }
            }).collect();
        }
        
        self.is_loading = false;
        self.status_message = "Stats refreshed".to_string();
        Ok(())
    }

    // Telegram controls
    pub fn toggle_telegram(&mut self) {
        if !self.telegram_configured {
            self.status_message = "Telegram not configured in config.toml".to_string();
            self.add_log("⚠ Telegram not configured");
            return;
        }
        
        if self.telegram_enabled {
            // Disable
            self.telegram_notifier = None;
            self.telegram_enabled = false;
            self.telegram_status = "Disabled".to_string();
            self.add_log("✓ Telegram notifications disabled");
            self.status_message = "Telegram notifications disabled".to_string();
        } else {
            // Enable
            self.telegram_notifier = crate::telegram::AutoNotifier::new(&self.config);
            self.telegram_enabled = self.telegram_notifier.is_some();
            
            if self.telegram_enabled {
                self.telegram_status = "Active".to_string();
                self.add_log("✓ Telegram notifications enabled");
                self.status_message = "Telegram notifications enabled".to_string();
            } else {
                self.telegram_status = "Failed to enable".to_string();
                self.add_log("✗ Failed to enable Telegram");
                self.status_message = "Failed to enable Telegram".to_string();
            }
        }
    }

    pub async fn test_telegram(&mut self) {
        let has_notifier = self.telegram_notifier.is_some();
        
        if has_notifier {
            self.add_log("Sending test notification...");
            
            if let Some(ref notifier) = self.telegram_notifier {
                notifier.notify_error("🧪 Test notification from TUI").await;
            }
            
            self.status_message = "Test notification sent".to_string();
            self.add_log("✓ Test notification sent");
        } else {
            self.status_message = "Telegram is not enabled".to_string();
            self.add_log("⚠ Telegram is not enabled");
        }
    }
    
    fn add_log(&mut self, message: &str) {
        let timestamp = Utc::now().format("%H:%M:%S");
        self.logs.push(format!("[{}] {}", timestamp, message));
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }
}