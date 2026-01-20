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
    
    pub fn get_total_reclaimed(&self) -> Result<u64> {
        let total: Option<u64> = self.conn.query_row(
            "SELECT SUM(reclaimed_amount) FROM reclaim_operations",
            [],
            |row| row.get(0),
        )?;
        
        Ok(total.unwrap_or(0))
    }
}