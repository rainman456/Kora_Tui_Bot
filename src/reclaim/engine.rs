use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer, Signature},
    transaction::Transaction,
    instruction::Instruction,
};
use crate::{
    error::Result,
    solana::client::SolanaRpcClient,
    kora::types::AccountType,
};
use tracing::{info, warn};

/// Result of a reclaim operation
#[derive(Debug, Clone)]
pub struct ReclaimResult {
    pub signature: Option<Signature>,
    pub amount_reclaimed: u64,
    pub account: Pubkey,
    pub dry_run: bool,
}

pub struct ReclaimEngine {
    pub(crate) rpc_client: SolanaRpcClient,
    pub(crate) treasury_wallet: Pubkey,
    pub(crate) signer: Keypair,
    pub(crate) dry_run: bool,
}

impl ReclaimEngine {
    pub fn new(
        rpc_client: SolanaRpcClient,
        treasury_wallet: Pubkey,
        signer: Keypair,
        dry_run: bool,
    ) -> Self {
        Self {
            rpc_client,
            treasury_wallet,
            signer,
            dry_run,
        }
    }
    
    /// Reclaim rent from an account
    /// 
    /// Handles different account types:
    /// - System accounts: Transfer balance to treasury
    /// - SPL Token accounts: Close account instruction
pub async fn reclaim_account(
    &self,
    account_pubkey: &Pubkey,
    account_type: &AccountType,
) -> Result<ReclaimResult> {
    info!("Attempting to reclaim rent from account: {}", account_pubkey);
    
    let account = self.rpc_client.get_account(account_pubkey).await?;
    
    let (balance, account_data) = if let Some(acc) = account {
        (acc.lamports, acc)
    } else {
        // Account already closed
        warn!("Account {} is already closed, nothing to reclaim", account_pubkey);
        return Ok(ReclaimResult {
            signature: None,
            amount_reclaimed: 0,
            account: *account_pubkey,
            dry_run: self.dry_run,
        });
    };
    
    if balance == 0 {
        warn!("No rent to reclaim from account: {}", account_pubkey);
        return Err(crate::error::ReclaimError::NotEligible(
            "Account has no balance".to_string()
        ));
    }
    
    info!(
        "Reclaiming {} lamports ({:.9} SOL) from {} (type: {:?})",
        balance,
        crate::solana::rent::RentCalculator::lamports_to_sol(balance),
        account_pubkey,
        account_type
    );
    
    // For SPL Token accounts, verify token balance is zero before closing
    if let AccountType::SplToken = account_type {
        // SPL Token account data structure:
        // - Mint: 32 bytes (offset 0)
        // - Owner: 32 bytes (offset 32)
        // - Amount: 8 bytes (offset 64)
        // - Delegate: 36 bytes (offset 72)
        // - State: 1 byte (offset 108)
        // - IsNative: 12 bytes (offset 109)
        // - DelegatedAmount: 8 bytes (offset 121)
        // - CloseAuthority: 36 bytes (offset 129)
        
        if account_data.data.len() < 165 {
            return Err(crate::error::ReclaimError::NotEligible(
                "Invalid SPL Token account data size".to_string()
            ));
        }
        
        // Check token amount (offset 64, 8 bytes as u64 little-endian)
        let amount_bytes: [u8; 8] = account_data.data[64..72]
            .try_into()
            .map_err(|_| crate::error::ReclaimError::NotEligible(
                "Failed to parse token amount from account data".to_string()
            ))?;
        let token_amount = u64::from_le_bytes(amount_bytes);
        
        if token_amount > 0 {
            return Err(crate::error::ReclaimError::NotEligible(
                format!(
                    "Cannot close token account: still has {} tokens. Account must be emptied first.",
                    token_amount
                )
            ));
        }
        
        // Check account state (offset 108, 1 byte)
        // 0 = Uninitialized, 1 = Initialized, 2 = Frozen
        let state = account_data.data[108];
        if state == 2 {
            return Err(crate::error::ReclaimError::NotEligible(
                "Cannot close frozen token account".to_string()
            ));
        }
        
        // Verify close authority
        // CloseAuthority is at offset 129 (4 bytes for option discriminant + 32 bytes for pubkey)
        // First byte indicates if close authority is set (0 = None, 1 = Some)
        let has_close_authority = account_data.data[129] == 1;
        
        if has_close_authority {
            let close_authority_bytes: [u8; 32] = account_data.data[130..162]
                .try_into()
                .map_err(|_| crate::error::ReclaimError::NotEligible(
                    "Failed to parse close authority from account data".to_string()
                ))?;
            let close_authority = Pubkey::new_from_array(close_authority_bytes);
            
            if close_authority != self.signer.pubkey() {
                return Err(crate::error::ReclaimError::NotEligible(
                    format!(
                        "Cannot close token account: operator ({}) is not the close authority ({})",
                        self.signer.pubkey(),
                        close_authority
                    )
                ));
            }
            
            info!(
                "Verified: Operator {} has close authority for token account {}",
                self.signer.pubkey(),
                account_pubkey
            );
        } else {
            // Check if operator is the account owner as fallback
            let owner_bytes: [u8; 32] = account_data.data[32..64]
                .try_into()
                .map_err(|_| crate::error::ReclaimError::NotEligible(
                    "Failed to parse owner from account data".to_string()
                ))?;
            let owner = Pubkey::new_from_array(owner_bytes);
            
            if owner != self.signer.pubkey() {
                return Err(crate::error::ReclaimError::NotEligible(
                    format!(
                        "Cannot close token account: no close authority set and operator ({}) is not the owner ({})",
                        self.signer.pubkey(),
                        owner
                    )
                ));
            }
            
            info!(
                "Verified: Operator {} is the owner of token account {}",
                self.signer.pubkey(),
                account_pubkey
            );
        }
    }
    
    let instruction = self.build_close_instruction(account_pubkey, account_type, balance)?;
    
    if self.dry_run {
        info!("DRY RUN: Would reclaim {} lamports from {}", balance, account_pubkey);
        return Ok(ReclaimResult {
            signature: None,
            amount_reclaimed: balance,
            account: *account_pubkey,
            dry_run: true,
        });
    }
    
    let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
    
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&self.signer.pubkey()),
        &[&self.signer],
        recent_blockhash,
    );
    
    // Send transaction with retry logic
    info!("Sending reclaim transaction for account {}", account_pubkey);
    let signature = self.rpc_client.send_and_confirm_transaction(&transaction).await?;
    
    info!(
        "✓ Successfully reclaimed {} lamports from {} | Signature: {}",
        balance,
        account_pubkey,
        signature
    );
    
    Ok(ReclaimResult {
        signature: Some(signature),
        amount_reclaimed: balance,
        account: *account_pubkey,
        dry_run: false,
    })
}
    
fn build_close_instruction(
    &self,
    account_pubkey: &Pubkey,
    account_type: &AccountType,
    _balance: u64,
) -> Result<Instruction> {
    match account_type {
        AccountType::System => {
            // CRITICAL: We cannot close system accounts we don't own!
            // For Kora-sponsored accounts, the user owns the account after creation.
            // The only way to reclaim is if the user voluntarily transfers back
            // or if we implement a program-based solution.
            warn!("Cannot automatically reclaim from System account: user owns the keys");
            Err(crate::error::ReclaimError::NotEligible(
                "Cannot reclaim from System accounts - user controls the private key. \
                 Reclaim only possible if user voluntarily closes account.".to_string()
            ))
        }
        
        AccountType::SplToken => {
            // For SPL Token accounts, we can only close if:
            // 1. The operator was set as the close_authority during creation
            // 2. The account has zero token balance
            
            // First verify the account can be closed (zero token balance)
             info!(
                "Building close instruction for SPL Token account {} (program: {})",
                account_pubkey,
                account_type.program_id()
            );
            let close_instruction = spl_token::instruction::close_account(
                &spl_token::id(),
                account_pubkey,
                &self.treasury_wallet, // Destination for remaining SOL
                &self.signer.pubkey(), // Authority (must be close_authority)
                &[], // No multisig signers
            )?;
            
            Ok(close_instruction)
        }
        
        AccountType::Other(program_id) => {
            // For other program accounts, we need program-specific logic
            //warn!("Cannot automatically close account owned by program: {}", program_id);
            warn!(
                "Cannot automatically close account owned by program: {} (ID: {})",
                program_id,
                account_type.program_id()
            );
            Err(crate::error::ReclaimError::NotEligible(
                format!("Custom program accounts require program-specific close logic for: {}", program_id)
            ))
        }
    }
}


    
    /// Batch reclaim multiple accounts
    pub async fn batch_reclaim(
        &self,
        accounts: &[(Pubkey, AccountType)],
    ) -> Result<Vec<(Pubkey, Result<ReclaimResult>)>> {
        let mut results = Vec::new();
        
        for (account, account_type) in accounts {
            let result = self.reclaim_account(account, account_type).await;
            results.push((*account, result));
        }
        
        Ok(results)
    }
}


// Clone implementation for ReclaimEngine (needed for batch processing in TUI)
impl Clone for ReclaimEngine {
    fn clone(&self) -> Self {
        use solana_sdk::signature::Keypair;
        
        // Clone the keypair by reconstructing from bytes
        let signer_bytes = self.signer.to_bytes();
        let signer = Keypair::from_bytes(&signer_bytes)
            .expect("Failed to clone keypair");
        
        Self {
            rpc_client: self.rpc_client.clone(),
            treasury_wallet: self.treasury_wallet,
            signer,
            dry_run: self.dry_run,
        }
    }
}

