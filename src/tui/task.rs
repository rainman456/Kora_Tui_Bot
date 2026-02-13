use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;
use std::sync::Mutex;
use anyhow::Result;
use chrono::Utc;
use std::fs::File;
use csv::Writer;
use tracing::debug;

use crate::{
    solana::SolanaRpcClient,
    kora::KoraMonitor,
    reclaim::{EligibilityChecker, ReclaimEngine},
    storage::Database,
};

use super::state::{Action, AccountEntry, AccountStatus, DryRunResult, ReclaimResult, RpcHealth, HealthStatus};

/// Background task manager for async operations
pub struct TaskManager {
    rpc_client: SolanaRpcClient,
    monitor: KoraMonitor,
    eligibility_checker: EligibilityChecker,
    reclaim_engine: Option<ReclaimEngine>,
    db: Database,
    rate_limiter: Arc<Semaphore>,
    scan_control: Arc<Mutex<Option<ScanControl>>>,
}

struct ScanControl {
    token: CancellationToken,
}

enum ScanCompletion {
    Completed { total: usize, eligible: usize },
    Cancelled { scanned: usize, eligible: usize },
}

impl TaskManager {
    pub fn new(
        rpc_client: SolanaRpcClient,
        monitor: KoraMonitor,
        eligibility_checker: EligibilityChecker,
        reclaim_engine: Option<ReclaimEngine>,
        db: Database,
        ) -> Self {
        // Rate limiter: max 10 concurrent RPC calls
        let rate_limiter = Arc::new(Semaphore::new(10));
        
        Self {
            rpc_client,
            monitor,
            eligibility_checker,
            reclaim_engine,
            db,
            rate_limiter,
            scan_control: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Spawn a scan task
    pub fn spawn_scan(&self, action_tx: UnboundedSender<Action>, max_transactions: usize) {
        let monitor = self.monitor.clone();
        let eligibility_checker = self.eligibility_checker.clone();
        let rpc_client = self.rpc_client.clone();
        let rate_limiter = self.rate_limiter.clone();
        let scan_control = self.scan_control.clone();
        let cancel_token = CancellationToken::new();
        let task_token = cancel_token.clone();
        
        if let Ok(mut guard) = self.scan_control.lock() {
            *guard = Some(ScanControl { token: cancel_token });
        }

        tokio::spawn(async move {
            debug!("Spawning scan task (max_transactions={})", max_transactions);
            let _ = action_tx.send(Action::ScanStarted);
            
            match Self::scan_accounts(
                monitor,
                eligibility_checker,
                rpc_client,
                rate_limiter,
                max_transactions,
                action_tx.clone(),
                task_token,
            )
            .await
            {
                Ok(ScanCompletion::Completed { total, eligible }) => {
                    debug!("Scan task completed (total={}, eligible={})", total, eligible);
                    let _ = action_tx.send(Action::ScanFinished { total, eligible });
                }
                Ok(ScanCompletion::Cancelled { scanned, eligible }) => {
                    debug!("Scan task cancelled (scanned={}, eligible={})", scanned, eligible);
                    let _ = action_tx.send(Action::ScanCancelled { scanned, eligible });
                }
                Err(e) => {
                    debug!("Scan task failed: {}", e);
                    let _ = action_tx.send(Action::ScanFailed(e.to_string()));
                }
            }

            if let Ok(mut guard) = scan_control.lock() {
                *guard = None;
            }
        });

    }

    pub fn cancel_scan(&self, action_tx: UnboundedSender<Action>) -> bool {
        let token = {
            let guard = self.scan_control.lock().ok();
            guard.and_then(|inner| inner.as_ref().map(|control| control.token.clone()))
        };

        if let Some(token) = token {
            token.cancel();
            let _ = action_tx.send(Action::ScanCancelRequested);
            true
        } else {
            false
        }
    }
    
    async fn scan_accounts(
        monitor: KoraMonitor,
        eligibility_checker: EligibilityChecker,
        rpc_client: SolanaRpcClient,
        rate_limiter: Arc<Semaphore>,
        max_transactions: usize,
        action_tx: UnboundedSender<Action>,
        cancel_token: CancellationToken,
    ) -> Result<ScanCompletion> {
        // Discover accounts
        debug!("Scanning sponsored accounts");
        let sponsored = tokio::select! {
            result = monitor.get_sponsored_accounts_quick(max_transactions) => result?,
            _ = cancel_token.cancelled() => {
                return Ok(ScanCompletion::Cancelled { scanned: 0, eligible: 0 });
            }
        };
        let total = sponsored.len();
        debug!("Discovered {} sponsored accounts", total);
        
        let mut eligible_count = 0;
        let mut entries = Vec::with_capacity(total);
        
        for (idx, account_info) in sponsored.into_iter().enumerate() {
            if cancel_token.is_cancelled() {
                return Ok(ScanCompletion::Cancelled {
                    scanned: entries.len(),
                    eligible: eligible_count,
                });
            }

            // Rate limiting
            let _permit = tokio::select! {
                permit = rate_limiter.acquire() => permit?,
                _ = cancel_token.cancelled() => {
                    return Ok(ScanCompletion::Cancelled {
                        scanned: entries.len(),
                        eligible: eligible_count,
                    });
                }
            };
            
            // Progress update every 50 accounts
            if idx % 50 == 0 {
                let _ = action_tx.send(Action::ScanProgress {
                    current: idx,
                    total: max_transactions,
                });
            }
            
            // Check eligibility
            let is_eligible = tokio::select! {
                result = eligibility_checker.is_eligible(&account_info.pubkey, account_info.created_at) => {
                    result.unwrap_or(false)
                }
                _ = cancel_token.cancelled() => {
                    return Ok(ScanCompletion::Cancelled {
                        scanned: entries.len(),
                        eligible: eligible_count,
                    });
                }
            };
            
            if is_eligible {
                eligible_count += 1;
            }
            
            // Get balance
            let balance = tokio::select! {
                result = rpc_client.get_balance(&account_info.pubkey) => result.unwrap_or(0),
                _ = cancel_token.cancelled() => {
                    return Ok(ScanCompletion::Cancelled {
                        scanned: entries.len(),
                        eligible: eligible_count,
                    });
                }
            };
            let rent_sol = balance as f64 / 1_000_000_000.0;
            
            // Calculate age
            let age = Utc::now() - account_info.created_at;
            let age_days = age.num_days() as u64;
            
            // Get eligibility reason
            let reason = tokio::select! {
                result = eligibility_checker.get_eligibility_reason(&account_info.pubkey, account_info.created_at) => {
                    result.unwrap_or_else(|_| "Unknown".to_string())
                }
                _ = cancel_token.cancelled() => {
                    return Ok(ScanCompletion::Cancelled {
                        scanned: entries.len(),
                        eligible: eligible_count,
                    });
                }
            };
            
            // Determine status
            let status = if is_eligible {
                AccountStatus::Eligible
            } else {
                AccountStatus::Active
            };

            let program_label = if account_info.account_type.is_reclaimable() {
                format!("{} (reclaimable)", account_info.account_type.description())
            } else {
                account_info.account_type.description().to_string()
            };
            
            let entry = AccountEntry {
                address: account_info.pubkey,
                program: program_label,
                age_days,
                rent_sol,
                last_tx: account_info.created_at,
                status,
                eligibility_reason: reason,
                creation_slot: account_info.creation_slot,
                data_size: account_info.data_size,
            };

            entries.push(entry.clone());
            let _ = action_tx.send(Action::AccountFound(entry));
        }

        let _ = action_tx.send(Action::ScanResults(entries));
        Ok(ScanCompletion::Completed {
            total,
            eligible: eligible_count,
        })
    }
    
    /// Spawn a dry-run task
    pub fn spawn_dry_run(&self, action_tx: UnboundedSender<Action>, account: String) {
        let reclaim_engine = match self.reclaim_engine.as_ref() {
            Some(engine) => engine.for_dry_run(),
            None => {
                let _ = action_tx.send(Action::DryRunFailed {
                    account,
                    error: "Reclaim engine not initialized".to_string(),
                });
                return;
            }
        };
        
        let rate_limiter = self.rate_limiter.clone();
        
        tokio::spawn(async move {
            let _ = action_tx.send(Action::DryRunStarted(account.clone()));
            
            match Self::dry_run_reclaim(reclaim_engine, rate_limiter, account.clone()).await {
                Ok(result) => {
                    let _ = action_tx.send(Action::DryRunSuccess(result));
                }
                Err(e) => {
                    let _ = action_tx.send(Action::DryRunFailed {
                        account,
                        error: e.to_string(),
                    });
                }
            }
        });
    }
    
    async fn dry_run_reclaim(
        reclaim_engine: ReclaimEngine,
        rate_limiter: Arc<Semaphore>,
        account: String,
    ) -> Result<DryRunResult> {
        let _permit = rate_limiter.acquire().await?;
        
        let pubkey = account.parse()?;
        
        let account_type = reclaim_engine.determine_account_type(&pubkey).await?;
        let result = reclaim_engine.reclaim_account(&pubkey, &account_type).await?;
        
        let rent_sol = result.amount_reclaimed as f64 / 1_000_000_000.0;
        let estimated_fee = 0.000005; // 5000 lamports estimate
        let net_gain = rent_sol - estimated_fee;
        
        Ok(DryRunResult {
            account,
            projected_sol: rent_sol,
            estimated_fee,
            net_gain,
            timestamp: Utc::now(),
        })
    }
    
    /// Spawn a reclaim execution task
    pub fn spawn_reclaim(&self, action_tx: UnboundedSender<Action>, account: String) {
        let reclaim_engine = match self.reclaim_engine.as_ref() {
            Some(engine) => engine.for_execution(),
            None => {
                let _ = action_tx.send(Action::ReclaimFailed {
                    account,
                    error: "Reclaim engine not initialized".to_string(),
                });
                return;
            }
        };
        
        let db = self.db.clone();
        let rate_limiter = self.rate_limiter.clone();
        
        tokio::spawn(async move {
            let _ = action_tx.send(Action::ReclaimStarted(account.clone()));
            
            match Self::execute_reclaim(reclaim_engine, db, rate_limiter, account.clone()).await {
                Ok(result) => {
                    let _ = action_tx.send(Action::ReclaimSuccess(result));
                }
                Err(e) => {
                    let _ = action_tx.send(Action::ReclaimFailed {
                        account,
                        error: e.to_string(),
                    });
                }
            }
        });
    }
    
    async fn execute_reclaim(
        reclaim_engine: ReclaimEngine,
        db: Database,
        rate_limiter: Arc<Semaphore>,
        account: String,
    ) -> Result<ReclaimResult> {
        let _permit = rate_limiter.acquire().await?;
        
        let pubkey = account.parse()?;
        let account_type = reclaim_engine.determine_account_type(&pubkey).await?;
        
        let result = reclaim_engine.reclaim_account(&pubkey, &account_type).await?;
        
        if result.dry_run {
            return Err(anyhow::anyhow!(
                "Execution path returned a dry-run result. Transaction was not submitted."
            ));
        }

        let signature = result.signature
            .ok_or_else(|| anyhow::anyhow!("No signature returned"))?;
        
        let amount_sol = result.amount_reclaimed as f64 / 1_000_000_000.0;
        
        // Save to database
        let _ = db.save_reclaim_operation(&crate::storage::models::ReclaimOperation {
            id: 0,
            account_pubkey: account.clone(),
            reclaimed_amount: result.amount_reclaimed,
            tx_signature: signature.to_string(),
            timestamp: Utc::now(),
            reason: "TUI execution".to_string(),
        });
        
        let _ = db.update_account_status(&account, crate::storage::models::AccountStatus::Reclaimed);
        
        Ok(ReclaimResult {
            account,
            amount_sol,
            signature: signature.to_string(),
            timestamp: Utc::now(),
        })
    }
    
    /// Spawn whitelist task
    pub fn spawn_whitelist(&self, action_tx: UnboundedSender<Action>, account: String) {
        tokio::spawn(async move {
            match Self::add_to_whitelist(account.clone()).await {
                Ok(_) => {
                    let _ = action_tx.send(Action::AccountWhitelisted(account));
                }
                Err(e) => {
                    let _ = action_tx.send(Action::WhitelistFailed(e.to_string()));
                }
            }
        });
    }
    
    async fn add_to_whitelist(account: String) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::{Read, Write};
        
        let path = "whitelist.json";
        
        // Read existing whitelist
        let mut whitelist: Vec<String> = if let Ok(mut file) = OpenOptions::new().read(true).open(path) {
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            serde_json::from_str(&contents).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };
        
        // Add new account if not present
        if !whitelist.contains(&account) {
            whitelist.push(account);
        }
        
        // Write back
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        
        let json = serde_json::to_string_pretty(&whitelist)?;
        file.write_all(json.as_bytes())?;
        
        Ok(())
    }
    
    /// Spawn export task
    pub fn spawn_export(&self, action_tx: UnboundedSender<Action>, accounts: Vec<AccountEntry>) {
        tokio::spawn(async move {
            let _ = action_tx.send(Action::ExportStarted);
            
            match Self::export_to_csv(accounts).await {
                Ok(path) => {
                    let _ = action_tx.send(Action::ExportSuccess(path));
                }
                Err(e) => {
                    let _ = action_tx.send(Action::ExportFailed(e.to_string()));
                }
            }
        });
    }
    
    async fn export_to_csv(accounts: Vec<AccountEntry>) -> Result<String> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("reclaim_report_{}.csv", timestamp);
        
        let file = File::create(&filename)?;
        let mut writer = Writer::from_writer(file);
        
        // Write headers
        writer.write_record(&[
            "Address",
            "Program",
            "Age (days)",
            "Rent (SOL)",
            "Last Transaction",
            "Status",
            "Eligibility Reason",
            "Creation Slot",
            "Data Size",
        ])?;
        
        // Write data
        for account in accounts {
            writer.write_record(&[
                account.address.to_string(),
                account.program,
                account.age_days.to_string(),
                format!("{:.9}", account.rent_sol),
                account.last_tx.to_rfc3339(),
                account.status.display().to_string(),
                account.eligibility_reason,
                account.creation_slot.to_string(),
                account.data_size.to_string(),
            ])?;
        }
        
        writer.flush()?;
        
        Ok(filename)
    }
    
    /// Spawn RPC health check task
    pub fn spawn_health_check(&self, action_tx: UnboundedSender<Action>) {
        let rpc_client = self.rpc_client.clone();
        
        tokio::spawn(async move {
            loop {
                let start = std::time::Instant::now();
                //let status = match rpc_client.client.get_health().await
                let status = match rpc_client.client.get_health(){
                    Ok(_) => {
                        let latency = start.elapsed().as_millis() as u64;
                        if latency < 500 {
                            HealthStatus::Healthy
                        } else {
                            HealthStatus::Degraded
                        }
                    }
                    Err(_) => HealthStatus::Down,
                };
                
                let latency_ms = start.elapsed().as_millis() as u64;
                
                let _ = action_tx.send(Action::RpcHealthUpdate(RpcHealth {
                    status,
                    latency_ms,
                    last_check: Utc::now(),
                }));
                
                // Check every 10 seconds
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });
    }
}
