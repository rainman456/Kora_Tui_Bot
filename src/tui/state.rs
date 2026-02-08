use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use solana_sdk::pubkey::Pubkey;

/// Maximum number of log entries to keep in memory
const MAX_LOG_ENTRIES: usize = 50;

/// Actions that can be dispatched to update the TUI state
#[derive(Debug, Clone)]
pub enum Action {
    // Scan lifecycle
    ScanStarted,
    AccountFound(AccountEntry),
    ScanResults(Vec<AccountEntry>),
    ScanProgress { current: usize, total: usize },
    ScanFinished { total: usize, eligible: usize },
    ScanFailed(String),
    
    // Dry-run lifecycle
    DryRunStarted(String),
    DryRunSuccess(DryRunResult),
    DryRunFailed { account: String, error: String },
    
    // Reclaim lifecycle
    ReclaimStarted(String),
    ReclaimSuccess(ReclaimResult),
    ReclaimFailed { account: String, error: String },
    
    // Whitelist operations
    AccountWhitelisted(String),
    WhitelistFailed(String),
    
    // Export operations
    ExportStarted,
    ExportSuccess(String),
    ExportFailed(String),
    
    // RPC Health
    RpcHealthUpdate(RpcHealth),
    
    // System events
    Log(LogEntry),
    Error(String),
}

/// Represents an account entry in the monitor table
#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub address: Pubkey,
    pub program: String,
    pub age_days: u64,
    pub rent_sol: f64,
    pub last_tx: DateTime<Utc>,
    pub status: AccountStatus,
    pub eligibility_reason: String,
    pub creation_slot: u64,
    pub data_size: usize,
}

/// Account status for UI rendering
#[derive(Debug, Clone, PartialEq)]
pub enum AccountStatus {
    Active,
    Eligible,
    Whitelisted,
    Reclaimed,
    Processing,
    Failed,
}

impl AccountStatus {
    pub fn display(&self) -> &str {
        match self {
            Self::Active => "ACTIVE",
            Self::Eligible => "ELIGIBLE",
            Self::Whitelisted => "WHITELISTED 🔒",
            Self::Reclaimed => "RECLAIMED ✓",
            Self::Processing => "PROCESSING...",
            Self::Failed => "FAILED ✗",
        }
    }
}

/// Result of a dry-run operation
#[derive(Debug, Clone)]
pub struct DryRunResult {
    pub account: String,
    pub projected_sol: f64,
    pub estimated_fee: f64,
    pub net_gain: f64,
    pub timestamp: DateTime<Utc>,
}

/// Result of a reclaim operation
#[derive(Debug, Clone)]
pub struct ReclaimResult {
    pub account: String,
    pub amount_sol: f64,
    pub signature: String,
    pub timestamp: DateTime<Utc>,
}

/// RPC health status
#[derive(Debug, Clone)]
pub struct RpcHealth {
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub last_check: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Down,
}

impl HealthStatus {
    pub fn display(&self) -> &str {
        match self {
            Self::Healthy => "✓ HEALTHY",
            Self::Degraded => "⚠ DEGRADED",
            Self::Down => "✗ DOWN",
        }
    }
}

/// Log entry for the activity log
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Main TUI state - updated exclusively via Actions
pub struct State {
    // UI State
    pub current_mode: Mode,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub show_help: bool,
    pub show_expanded_details: bool,
    
    // Data
    pub accounts: Vec<AccountEntry>,
    pub activity_log: VecDeque<LogEntry>,
    pub dry_run_cache: std::collections::HashMap<String, DryRunResult>,
    pub whitelist: std::collections::HashSet<String>,
    
    // Summary metrics
    pub total_accounts: usize,
    pub total_locked_sol: f64,
    pub eligible_accounts: usize,
    pub total_reclaimed_sol: f64,
    pub at_risk_sol: f64,
    
    // System state
    pub rpc_health: RpcHealth,
    pub is_scanning: bool,
    pub is_processing: bool,
    pub network: String,
    pub treasury_address: String,
    
    // Safety flags
    pub dry_run_required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Monitor,
    DryRun,
    Execute,
}

impl State {
    pub fn new(network: String, treasury_address: String) -> Self {
        Self {
            current_mode: Mode::Monitor,
            selected_index: 0,
            scroll_offset: 0,
            show_help: false,
            show_expanded_details: false,
            accounts: Vec::new(),
            activity_log: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            dry_run_cache: std::collections::HashMap::new(),
            whitelist: std::collections::HashSet::new(),
            total_accounts: 0,
            total_locked_sol: 0.0,
            eligible_accounts: 0,
            total_reclaimed_sol: 0.0,
            at_risk_sol: 0.0,
            rpc_health: RpcHealth {
                status: HealthStatus::Healthy,
                latency_ms: 0,
                last_check: Utc::now(),
            },
            is_scanning: false,
            is_processing: false,
            network,
            treasury_address,
            dry_run_required: true,
        }
    }
    
    /// Apply an action to update the state
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::ScanStarted => {
                self.is_scanning = true;
                self.accounts.clear();
                self.dry_run_cache.clear();
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.add_log(LogLevel::Info, "Starting account scan...");
            }
            
            Action::AccountFound(account) => {
                self.accounts.push(account);
                self.recalculate_metrics();
            }

            Action::ScanResults(accounts) => {
                self.accounts = accounts;
                self.recalculate_metrics();
                if self.selected_index >= self.accounts.len() {
                    self.selected_index = 0;
                }
            }
            
            Action::ScanProgress { current, total } => {
                // Update progress (could be used for a progress bar)
                if current % 100 == 0 {
                    self.add_log(
                        LogLevel::Info, 
                        &format!("Scanned {}/{} transactions", current, total)
                    );
                }
            }
            
            Action::ScanFinished { total, eligible } => {
                self.is_scanning = false;
                self.total_accounts = total;
                self.eligible_accounts = eligible;
                self.recalculate_metrics();
                self.add_log(
                    LogLevel::Success,
                    &format!("Scan complete: {} accounts, {} eligible", total, eligible)
                );
            }
            
            Action::ScanFailed(error) => {
                self.is_scanning = false;
                self.add_log(LogLevel::Error, &format!("Scan failed: {}", error));
            }
            
            Action::DryRunStarted(account) => {
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account) {
                    acc.status = AccountStatus::Processing;
                }
                self.add_log(LogLevel::Info, &format!("Dry-run started for {}", &account[..8]));
            }
            
            Action::DryRunSuccess(result) => {
                let account_key = result.account.clone();
                self.dry_run_cache.insert(account_key.clone(), result.clone());
                
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account_key) {
                    acc.status = AccountStatus::Eligible;
                }
                
                self.add_log(
                    LogLevel::Success,
                    &format!("Dry-run complete: Net gain {:.4} SOL", result.net_gain)
                );
            }
            
            Action::DryRunFailed { account, error } => {
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account) {
                    acc.status = AccountStatus::Failed;
                }
                self.add_log(LogLevel::Error, &format!("Dry-run failed: {}", error));
            }
            
            Action::ReclaimStarted(account) => {
                self.is_processing = true;
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account) {
                    acc.status = AccountStatus::Processing;
                }
                self.add_log(LogLevel::Info, &format!("Reclaim started for {}", &account[..8]));
            }
            
            Action::ReclaimSuccess(result) => {
                self.is_processing = false;
                
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == result.account) {
                    acc.status = AccountStatus::Reclaimed;
                }
                
                self.total_reclaimed_sol += result.amount_sol;
                self.recalculate_metrics();
                
                self.add_log(
                    LogLevel::Success,
                    &format!(
                        "✓ Reclaimed {:.4} SOL (sig: {}...) at {}",
                        result.amount_sol,
                        &result.signature[..8],
                        result.timestamp.format("%H:%M:%S")
                    )
                );
            }
            
            Action::ReclaimFailed { account, error } => {
                self.is_processing = false;
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account) {
                    acc.status = AccountStatus::Failed;
                }
                self.add_log(LogLevel::Error, &format!("Reclaim failed: {}", error));
            }
            
            Action::AccountWhitelisted(account) => {
                self.whitelist.insert(account.clone());
                if let Some(acc) = self.accounts.iter_mut().find(|a| a.address.to_string() == account) {
                    acc.status = AccountStatus::Whitelisted;
                }
                self.add_log(LogLevel::Success, &format!("Account whitelisted: {}", &account[..8]));
            }
            
            Action::WhitelistFailed(error) => {
                self.add_log(LogLevel::Error, &format!("Whitelist failed: {}", error));
            }
            
            Action::ExportStarted => {
                self.add_log(LogLevel::Info, "Exporting to CSV...");
            }
            
            Action::ExportSuccess(path) => {
                self.add_log(LogLevel::Success, &format!("✓ Exported to {}", path));
            }
            
            Action::ExportFailed(error) => {
                self.add_log(LogLevel::Error, &format!("Export failed: {}", error));
            }
            
            Action::RpcHealthUpdate(health) => {
                self.rpc_health = health;
            }
            
            Action::Log(entry) => {
                self.add_log_entry(entry);
            }
            
            Action::Error(error) => {
                self.add_log(LogLevel::Error, &error);
            }
        }
    }
    
    /// Get the currently selected account
    pub fn get_selected_account(&self) -> Option<&AccountEntry> {
        self.accounts.get(self.selected_index)
    }
    
    /// Get dry-run result for the selected account
    pub fn get_selected_dry_run(&self) -> Option<&DryRunResult> {
        self.get_selected_account()
            .and_then(|acc| self.dry_run_cache.get(&acc.address.to_string()))
    }
    
    /// Check if execute is allowed for selected account
    pub fn can_execute_selected(&self) -> bool {
        if !self.dry_run_required {
            return true;
        }
        
        self.get_selected_account()
            .map(|acc| {
                self.dry_run_cache.contains_key(&acc.address.to_string())
                    && acc.status == AccountStatus::Eligible
                    && !self.whitelist.contains(&acc.address.to_string())
            })
            .unwrap_or(false)
    }
    
    /// Navigation helpers
    pub fn select_next(&mut self) {
        if !self.accounts.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.accounts.len() - 1);
        }
    }
    
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }
    
    // Internal helpers
    fn add_log(&mut self, level: LogLevel, message: &str) {
        self.add_log_entry(LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
        });
    }
    
    fn add_log_entry(&mut self, entry: LogEntry) {
        if self.activity_log.len() >= MAX_LOG_ENTRIES {
            self.activity_log.pop_back();
        }
        self.activity_log.push_front(entry);
    }
    
    fn recalculate_metrics(&mut self) {
        self.total_accounts = self.accounts.len();
        self.total_locked_sol = self.accounts.iter()
            .map(|a| a.rent_sol)
            .sum();
        self.eligible_accounts = self.accounts.iter()
            .filter(|a| a.status == AccountStatus::Eligible)
            .count();
        self.at_risk_sol = self.accounts.iter()
            .filter(|a| a.status == AccountStatus::Eligible && a.age_days > 30)
            .map(|a| a.rent_sol)
            .sum();
    }
}
