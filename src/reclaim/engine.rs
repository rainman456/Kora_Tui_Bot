use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer, Signature},
    system_instruction,
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
    rpc_client: SolanaRpcClient,
    treasury_wallet: Pubkey,
    signer: Keypair,
    dry_run: bool,
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
        
        // Get account info
        let account = self.rpc_client.get_account(account_pubkey).await?;
        
        let balance = if let Some(acc) = account {
            acc.lamports
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
        
        // Build appropriate close instruction based on account type
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
        
        // Build and send transaction
        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
        
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.signer.pubkey()),
            &[&self.signer],
            recent_blockhash,
        );
        
        // Send transaction with retry logic
        let signature = self.rpc_client.send_and_confirm_transaction(&transaction).await?;
        
        info!(
            "✓ Reclaimed {} lamports from {} | Signature: {}",
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
    
    /// Build appropriate close instruction based on account type
    /// Returns the instruction and the balance to be reclaimed
    fn build_close_instruction(
        &self,
        account_pubkey: &Pubkey,
        account_type: &AccountType,
        balance: u64,
    ) -> Result<Instruction> {
        match account_type {
            AccountType::System => {
                // For system accounts, transfer all lamports to treasury
                // This effectively "closes" the account
                // NOTE: The account being transferred FROM must sign the transaction
                Ok(system_instruction::transfer(
                    account_pubkey,
                    &self.treasury_wallet,
                    balance, // Use actual balance
                ))
            }
            
            AccountType::SplToken => {
                // For SPL Token accounts, use the close_account instruction
                // This requires the account owner's authority
                // Note: The actual owner/authority would need to be provided
                // For Kora-sponsored accounts, the operator should have authority
                
                let close_instruction = spl_token::instruction::close_account(
                    &spl_token::id(),
                    account_pubkey,
                    &self.treasury_wallet, // Destination for remaining SOL
                    &self.signer.pubkey(), // Authority (operator)
                    &[], // No multisig signers
                )?;
                
                Ok(close_instruction)
            }
            
            AccountType::Other(program_id) => {
                warn!("Cannot automatically close account owned by program: {}", program_id);
                Err(crate::error::ReclaimError::NotEligible(
                    format!("Unsupported account type owned by program: {}", program_id)
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