use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};
use crate::{
    error::Result,
    storage::models::{SponsoredAccount, ReclaimOperation, AccountStatus, PassiveReclaimRecord},
};
use chrono::Utc;
use std::str::FromStr;

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { 
            conn: Arc::new(Mutex::new(conn)) 
        };
        db.init_schema()?;
        Ok(db)
    }
    
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sponsored_accounts (
                pubkey TEXT PRIMARY KEY,
            created_at TEXT NOT NULL,
            closed_at TEXT,
            rent_lamports INTEGER NOT NULL,
            data_size INTEGER NOT NULL,
            status TEXT NOT NULL,
            creation_signature TEXT,
            creation_slot INTEGER,
            close_authority TEXT,
            reclaim_strategy TEXT
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS reclaim_operations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_pubkey TEXT NOT NULL,
                reclaimed_amount INTEGER NOT NULL,
                tx_signature TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                reason TEXT NOT NULL,
                FOREIGN KEY (account_pubkey) REFERENCES sponsored_accounts(pubkey)
            )",
            [],
        )?;
        
        // Checkpoints table for tracking scan progress
        conn.execute(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

          conn.execute(
        "CREATE TABLE IF NOT EXISTS passive_reclaims (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            amount INTEGER NOT NULL,
            attributed_accounts TEXT NOT NULL,
            confidence TEXT NOT NULL,
            timestamp TEXT NOT NULL
        )",
        [],
    )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status ON sponsored_accounts(status)",
            [],
        )?;

        conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_reclaim_strategy 
         ON sponsored_accounts(reclaim_strategy)",
        [],
    )?;
        
        // Index on creation_signature for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_creation_signature ON sponsored_accounts(creation_signature)",
            [],
        )?;
        
        Ok(())
    }
    
    pub fn save_account(&self, account: &SponsoredAccount) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO sponsored_accounts 
             (pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                account.pubkey,
                account.created_at.to_rfc3339(),
                account.closed_at.map(|dt| dt.to_rfc3339()),
                account.rent_lamports,
                account.data_size,
                format!("{:?}", account.status),
                account.creation_signature,
                account.creation_slot.map(|s| s as i64),
            ],
        )?;
        Ok(())
    }
    
    pub fn get_active_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE status = 'Active'"
        )?;
        
        let accounts = stmt.query_map([], |row| {
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status: AccountStatus::Active,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_closed_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE status = 'Closed'"
        )?;
        
        let accounts = stmt.query_map([], |row| {
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status: AccountStatus::Closed,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_reclaimed_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE status = 'Reclaimed'"
        )?;
        
        let accounts = stmt.query_map([], |row| {
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status: AccountStatus::Reclaimed,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_account_by_pubkey(&self, pubkey: &str) -> Result<Option<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE pubkey = ?1"
        )?;
        
        let mut accounts = stmt.query_map([pubkey], |row| {
            let status_str: String = row.get(5)?;
            let status = match status_str.as_str() {
                "Active" => AccountStatus::Active,
                "Closed" => AccountStatus::Closed,
                "Reclaimed" => AccountStatus::Reclaimed,
                _ => AccountStatus::Active,
            };
            
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?;
        
        Ok(accounts.next().transpose()?)
    }
    
    pub fn update_account_status(&self, pubkey: &str, status: AccountStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = if status != AccountStatus::Active {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        
        conn.execute(
            "UPDATE sponsored_accounts 
             SET status = ?1, closed_at = COALESCE(?2, closed_at)
             WHERE pubkey = ?3",
            params![format!("{:?}", status), now, pubkey],
        )?;
        
        Ok(())
    }
    
    pub fn save_reclaim_operation(&self, operation: &ReclaimOperation) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO reclaim_operations 
             (account_pubkey, reclaimed_amount, tx_signature, timestamp, reason) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                operation.account_pubkey,
                operation.reclaimed_amount,
                operation.tx_signature,
                operation.timestamp.to_rfc3339(),
                operation.reason,
            ],
        )?;
        Ok(())
    }
    
    pub fn get_reclaim_history(&self, limit: Option<usize>) -> Result<Vec<ReclaimOperation>> {
        let conn = self.conn.lock().unwrap();
        let query = if let Some(lim) = limit {
            format!(
                "SELECT id, account_pubkey, reclaimed_amount, tx_signature, timestamp, reason 
                 FROM reclaim_operations 
                 ORDER BY timestamp DESC 
                 LIMIT {}",
                lim
            )
        } else {
            "SELECT id, account_pubkey, reclaimed_amount, tx_signature, timestamp, reason 
             FROM reclaim_operations 
             ORDER BY timestamp DESC".to_string()
        };
        
        let mut stmt = conn.prepare(&query)?;
        
        let operations = stmt.query_map([], |row| {
            Ok(ReclaimOperation {
                id: row.get(0)?,
                account_pubkey: row.get(1)?,
                reclaimed_amount: row.get(2)?,
                tx_signature: row.get(3)?,
                timestamp: row.get::<_, String>(4)?.parse().unwrap(),
                reason: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(operations)
    }
    
    pub fn get_total_reclaimed(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let total: Option<u64> = conn.query_row(
            "SELECT SUM(reclaimed_amount) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        Ok(total.unwrap_or(0))
    }
    
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let conn = self.conn.lock().unwrap();
        let total_accounts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts",
            [],
            |row| row.get(0),
        )?;
        
        let active_accounts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Active'",
            [],
            |row| row.get(0),
        )?;
        
        let closed_accounts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Closed'",
            [],
            |row| row.get(0),
        )?;
        
        let reclaimed_accounts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Reclaimed'",
            [],
            |row| row.get(0),
        )?;
        
        let total_operations: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        let total_reclaimed: Option<u64> = conn.query_row(
            "SELECT SUM(reclaimed_amount) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        let total_reclaimed = total_reclaimed.unwrap_or(0);
        
        let avg_reclaim: Option<f64> = conn.query_row(
            "SELECT AVG(reclaimed_amount) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        Ok(DatabaseStats {
            total_accounts: total_accounts as usize,
            active_accounts: active_accounts as usize,
            closed_accounts: closed_accounts as usize,
            reclaimed_accounts: reclaimed_accounts as usize,
            total_operations: total_operations as usize,
            total_reclaimed,
            avg_reclaim_amount: avg_reclaim.unwrap_or(0.0) as u64,
        })
    }
    
    pub fn get_account_creation_details(&self, pubkey: &str) -> Result<Option<(String, u64)>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT creation_signature, creation_slot 
             FROM sponsored_accounts 
             WHERE pubkey = ?1 AND creation_signature IS NOT NULL",
            [pubkey],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                ))
            },
        );
        
        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    // Checkpoint management for incremental scanning
    
    /// Save the last processed signature to avoid re-scanning old transactions
    pub fn save_last_processed_signature(&self, signature: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO checkpoints (key, value, updated_at) 
             VALUES ('last_signature', ?1, ?2)",
            params![signature, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
    
    /// Get the last processed signature for incremental scanning
    pub fn get_last_processed_signature(&self) -> Result<Option<solana_sdk::signature::Signature>> {
        let conn = self.conn.lock().unwrap();
        let result: std::result::Result<String, rusqlite::Error> = conn.query_row(
            "SELECT value FROM checkpoints WHERE key = 'last_signature'",
            [],
            |row| row.get(0),
        );
        
        match result {
            Ok(sig_str) => {
                match solana_sdk::signature::Signature::from_str(&sig_str) {
                    Ok(sig) => Ok(Some(sig)),
                    Err(e) => {
                        tracing::warn!("Invalid signature in checkpoint: {} - {}", sig_str, e);
                        Ok(None)
                    }
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Save the last processed slot for tracking
    pub fn save_last_processed_slot(&self, slot: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO checkpoints (key, value, updated_at) 
             VALUES ('last_slot', ?1, ?2)",
            params![slot.to_string(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
    
    /// Get the last processed slot
    pub fn get_last_processed_slot(&self) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let result: std::result::Result<String, rusqlite::Error> = conn.query_row(
            "SELECT value FROM checkpoints WHERE key = 'last_slot'",
            [],
            |row| row.get(0),
        );
        
        match result {
            Ok(slot_str) => Ok(slot_str.parse::<u64>().ok()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Check if an account already exists in database (avoid re-processing)
    pub fn account_exists(&self, pubkey: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE pubkey = ?1",
            [pubkey],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
    
    /// Get all accounts (regardless of status) for caching
    pub fn get_all_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot
             FROM sponsored_accounts 
             ORDER BY created_at DESC"
        )?;
        
        let accounts = stmt.query_map([], |row| {
            let status_str: String = row.get(5)?;
            let status = match status_str.as_str() {
                "Active" => AccountStatus::Active,
                "Closed" => AccountStatus::Closed,
                "Reclaimed" => AccountStatus::Reclaimed,
                _ => AccountStatus::Active,
            };
            
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    /// Get checkpoint metadata (useful for debugging)
    pub fn get_checkpoint_info(&self) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, value, updated_at FROM checkpoints ORDER BY updated_at DESC"
        )?;
        
        let checkpoints = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(checkpoints)
    }
    
    /// Clear all checkpoints (useful for reset/debugging)
    pub fn clear_checkpoints(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM checkpoints", [])?;
        Ok(())
    }

    /// Save treasury balance checkpoint
    pub fn save_treasury_balance(&self, balance: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO checkpoints (key, value, updated_at) 
             VALUES ('treasury_balance', ?1, ?2)",
            params![balance.to_string(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Get last known treasury balance
    pub fn get_last_treasury_balance(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let result: std::result::Result<String, rusqlite::Error> = conn.query_row(
            "SELECT value FROM checkpoints WHERE key = 'treasury_balance'",
            [],
            |row| row.get(0),
        );
        
        match result {
            Ok(balance_str) => Ok(balance_str.parse::<u64>().unwrap_or(0)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    /// Get accounts that were recently marked as closed
    pub fn get_recently_closed_accounts(&self, hours: i64) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, 
                    creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE status = 'Closed' AND closed_at > ?1
             ORDER BY closed_at DESC"
        )?;
        
        let accounts = stmt.query_map([cutoff.to_rfc3339()], |row| {
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status: AccountStatus::Closed,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }

    /// Save a passive reclaim event
    pub fn save_passive_reclaim(
        &self,
        amount: u64,
        attributed_accounts: &[String],
        confidence: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO passive_reclaims 
             (amount, attributed_accounts, confidence, timestamp) 
             VALUES (?1, ?2, ?3, ?4)",
            params![
                amount,
                serde_json::to_string(attributed_accounts)?,
                confidence,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get total amount passively reclaimed
    pub fn get_total_passive_reclaimed(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let total: Option<u64> = conn.query_row(
            "SELECT SUM(amount) FROM passive_reclaims",
            [],
            |row| row.get(0),
        )?;
        
        Ok(total.unwrap_or(0))
    }

    /// Get passive reclaim history
    pub fn get_passive_reclaim_history(&self, limit: Option<usize>) -> Result<Vec<PassiveReclaimRecord>> {
        let conn = self.conn.lock().unwrap();
        let query = if let Some(lim) = limit {
            format!(
                "SELECT id, amount, attributed_accounts, confidence, timestamp 
                 FROM passive_reclaims 
                 ORDER BY timestamp DESC 
                 LIMIT {}",
                lim
            )
        } else {
            "SELECT id, amount, attributed_accounts, confidence, timestamp 
             FROM passive_reclaims 
             ORDER BY timestamp DESC".to_string()
        };
        
        let mut stmt = conn.prepare(&query)?;
        
        let records = stmt.query_map([], |row| {
            Ok(PassiveReclaimRecord {
                id: row.get(0)?,
                amount: row.get(1)?,
                attributed_accounts: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                confidence: row.get(3)?,
                timestamp: row.get::<_, String>(4)?.parse().unwrap(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(records)
    }

    /// Update account authority information
    pub fn update_account_authority(
        &self,
        pubkey: &str,
        close_authority: Option<String>,
        reclaim_strategy: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sponsored_accounts 
             SET close_authority = ?1, reclaim_strategy = ?2
             WHERE pubkey = ?3",
            params![close_authority, reclaim_strategy, pubkey],
        )?;
        Ok(())
    }

    /// Get accounts by reclaim strategy
    pub fn get_accounts_by_strategy(&self, strategy: &str) -> Result<Vec<SponsoredAccount>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status, 
                    creation_signature, creation_slot
             FROM sponsored_accounts 
             WHERE reclaim_strategy = ?1"
        )?;
        
        let accounts = stmt.query_map([strategy], |row| {
            let status_str: String = row.get(5)?;
            let status = match status_str.as_str() {
                "Active" => AccountStatus::Active,
                "Closed" => AccountStatus::Closed,
                "Reclaimed" => AccountStatus::Reclaimed,
                _ => AccountStatus::Active,
            };
            
            Ok(SponsoredAccount {
                pubkey: row.get(0)?,
                created_at: row.get::<_, String>(1)?.parse().unwrap(),
                closed_at: row.get::<_, Option<String>>(2)?
                    .map(|s| s.parse().unwrap()),
                rent_lamports: row.get(3)?,
                data_size: row.get(4)?,
                status,
                creation_signature: row.get(6).ok(),
                creation_slot: row.get::<_, Option<i64>>(7).ok()
                    .flatten()
                    .map(|s| s as u64),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    /// Batch save accounts (more efficient than individual saves)
    pub fn save_accounts_batch(&self, accounts: &[SponsoredAccount]) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut saved = 0;
        
        for account in accounts {
            tx.execute(
                "INSERT OR REPLACE INTO sponsored_accounts 
                 (pubkey, created_at, closed_at, rent_lamports, data_size, status, creation_signature, creation_slot) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    account.pubkey,
                    account.created_at.to_rfc3339(),
                    account.closed_at.map(|dt| dt.to_rfc3339()),
                    account.rent_lamports,
                    account.data_size,
                    format!("{:?}", account.status),
                    account.creation_signature,
                    account.creation_slot.map(|s| s as i64),
                ],
            )?;
            saved += 1;
        }
        
        tx.commit()?;
        Ok(saved)
    }
}

// Implement Clone manually for internal Arc cloning
impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DatabaseStats {
    pub total_accounts: usize,
    pub active_accounts: usize,
    pub closed_accounts: usize,
    pub reclaimed_accounts: usize,
    pub total_operations: usize,
    pub total_reclaimed: u64,
    pub avg_reclaim_amount: u64,
}