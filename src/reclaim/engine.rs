use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer, Signature},
    transaction::Transaction,
    instruction::Instruction,
};
use spl_token::state::AccountState;
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

    /// Return a clone configured explicitly for dry-run simulation mode.
    pub fn for_dry_run(&self) -> Self {
        let mut engine = self.clone();
        engine.dry_run = true;
        engine
    }

    /// Return a clone configured explicitly for on-chain execution mode.
    pub fn for_execution(&self) -> Self {
        let mut engine = self.clone();
        engine.dry_run = false;
        engine
    }

    /// Determine account type from on-chain account data.
    pub async fn determine_account_type(&self, pubkey: &Pubkey) -> Result<AccountType> {
        let account = self.rpc_client.get_account(pubkey).await?;
        let account = account.ok_or_else(|| {
            crate::error::ReclaimError::AccountNotFound(format!(
                "Account {} does not exist",
                pubkey
            ))
        })?;

        if account.owner == spl_token::id() {
            match account.data.len() {
                165 => Ok(AccountType::SplToken),
                82 => Ok(AccountType::SplMint),
                _ => Ok(AccountType::Other(account.owner)),
            }
        } else if account.owner == solana_sdk::system_program::id() {
            Ok(AccountType::System)
        } else {
            Ok(AccountType::Other(account.owner))
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
        
        // Use proper deserialization instead of manual byte parsing
        use solana_sdk::program_pack::Pack;
        use spl_token::state::Account as TokenAccount;
        use solana_sdk::program_option::COption;
        
        let token_account = TokenAccount::unpack(&account_data.data)
            .map_err(|e| crate::error::ReclaimError::NotEligible(
                format!("Failed to deserialize SPL Token account: {}", e)
            ))?;
        
        // Check token amount
        if token_account.amount > 0 {
            return Err(crate::error::ReclaimError::NotEligible(
                format!(
                    "Cannot close token account: still has {} tokens. Account must be emptied first.",
                    token_account.amount
                )
            ));
        }
        
        // Check account state
        if token_account.state == AccountState::Frozen {
            return Err(crate::error::ReclaimError::NotEligible(
                "Cannot close frozen token account".to_string()
            ));
        }
        
        // Verify close authority
        let operator_pubkey = self.signer.pubkey();
        let has_close_auth = match token_account.close_authority {
            COption::None => {
                // No close authority set - check if operator is the owner
                if token_account.owner != operator_pubkey {
                    return Err(crate::error::ReclaimError::NotEligible(
                        format!(
                            "Cannot close token account: no close authority set and operator ({}) is not the owner ({})",
                            operator_pubkey,
                            token_account.owner
                        )
                    ));
                }
                true
            }
            COption::Some(close_auth) => {
                if close_auth != operator_pubkey {
                    return Err(crate::error::ReclaimError::NotEligible(
                        format!(
                            "Cannot close token account: operator ({}) is not the close authority ({})",
                            operator_pubkey,
                            close_auth
                        )
                    ));
                }
                true
            }
        };
        
        if has_close_auth {
            info!(
                "Verified: Operator {} has close authority for token account {}",
                operator_pubkey,
                account_pubkey
            );
        }
    }
    
    // Re-verify balance before building transaction (prevent race conditions)
    let current_balance = self.rpc_client.get_balance(account_pubkey).await?;
    if current_balance == 0 {
        warn!("Account {} balance changed to zero before transaction", account_pubkey);
        return Ok(ReclaimResult {
            signature: None,
            amount_reclaimed: 0,
            account: *account_pubkey,
            dry_run: self.dry_run,
        });
    }
    
    let instruction = self.build_close_instruction(account_pubkey, account_type, current_balance)?;
    
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
        
        AccountType::SplMint => {
            // Mint accounts cannot and should not be closed
            // They are permanent infrastructure for the token
            warn!("Cannot close SPL Token Mint account: mints are permanent infrastructure");
            Err(crate::error::ReclaimError::NotEligible(
                "Cannot close SPL Token Mint accounts - they are permanent infrastructure. \
                 Mints should be filtered during discovery.".to_string()
            ))
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
