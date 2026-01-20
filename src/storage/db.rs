use rusqlite::{Connection, params};
use crate::{
    error::Result,
    storage::models::{SponsoredAccount, ReclaimOperation, AccountStatus},
};
use chrono::Utc;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }
    
    fn init_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sponsored_accounts (
                pubkey TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                closed_at TEXT,
                rent_lamports INTEGER NOT NULL,
                data_size INTEGER NOT NULL,
                status TEXT NOT NULL
            )",
            [],
        )?;
        
        self.conn.execute(
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
        
        // Create index for faster status queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status ON sponsored_accounts(status)",
            [],
        )?;
        
        Ok(())
    }
    
    pub fn save_account(&self, account: &SponsoredAccount) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sponsored_accounts 
             (pubkey, created_at, closed_at, rent_lamports, data_size, status) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                account.pubkey,
                account.created_at.to_rfc3339(),
                account.closed_at.map(|dt| dt.to_rfc3339()),
                account.rent_lamports,
                account.data_size,
                format!("{:?}", account.status),
            ],
        )?;
        Ok(())
    }
    
    pub fn get_active_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status 
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
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_closed_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status 
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
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_reclaimed_accounts(&self) -> Result<Vec<SponsoredAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status 
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
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        
        Ok(accounts)
    }
    
    pub fn get_account_by_pubkey(&self, pubkey: &str) -> Result<Option<SponsoredAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, created_at, closed_at, rent_lamports, data_size, status 
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
            })
        })?;
        
        Ok(accounts.next().transpose()?)
    }
    
    pub fn update_account_status(&self, pubkey: &str, status: AccountStatus) -> Result<()> {
        let now = if status != AccountStatus::Active {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        
        self.conn.execute(
            "UPDATE sponsored_accounts 
             SET status = ?1, closed_at = COALESCE(?2, closed_at)
             WHERE pubkey = ?3",
            params![format!("{:?}", status), now, pubkey],
        )?;
        
        Ok(())
    }
    
    pub fn save_reclaim_operation(&self, operation: &ReclaimOperation) -> Result<()> {
        self.conn.execute(
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
        
        let mut stmt = self.conn.prepare(&query)?;
        
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
        let total: Option<u64> = self.conn.query_row(
            "SELECT SUM(reclaimed_amount) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        Ok(total.unwrap_or(0))
    }
    
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let total_accounts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts",
            [],
            |row| row.get(0),
        )?;
        
        let active_accounts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Active'",
            [],
            |row| row.get(0),
        )?;
        
        let closed_accounts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Closed'",
            [],
            |row| row.get(0),
        )?;
        
        let reclaimed_accounts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sponsored_accounts WHERE status = 'Reclaimed'",
            [],
            |row| row.get(0),
        )?;
        
        let total_operations: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        let total_reclaimed = self.get_total_reclaimed()?;
        
        let avg_reclaim: Option<f64> = self.conn.query_row(
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
}

#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_accounts: usize,
    pub active_accounts: usize,
    pub closed_accounts: usize,
    pub reclaimed_accounts: usize,
    pub total_operations: usize,
    pub total_reclaimed: u64,
    pub avg_reclaim_amount: u64,
}