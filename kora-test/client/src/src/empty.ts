/**
 * CORRECT VERSION: Only burn tokens, keep accounts open
 * 
 * This script ONLY burns tokens, leaving accounts empty but still open.
 * Your Rust bot will then detect these empty accounts and close them to reclaim rent.
 * 
 * Flow:
 * 1. setup.ts → Creates ATAs with Kora as close authority and mints tokens
 * 2. THIS SCRIPT → Burns all tokens (accounts still exist but are empty)
 * 3. Rust bot → Detects empty accounts and closes them to reclaim rent
 */
import {
  createKeyPairSignerFromBytes,
  getBase58Encoder,
  createSolanaRpc,
  pipe,
  createTransactionMessage,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  appendTransactionMessageInstructions,
  signTransactionMessageWithSigners,
  getSignatureFromTransaction,
  KeyPairSigner,
  assertIsTransactionWithBlockhashLifetime,
  address,
} from "@solana/kit";
import {
  findAssociatedTokenPda,
  TOKEN_PROGRAM_ADDRESS,
  getBurnInstruction,
} from "@solana-program/token";
import dotenv from "dotenv";
import path from "path";

dotenv.config({ path: path.join(process.cwd(), "..", ".env") });

const CONFIG = {
  solanaRpcUrl: "http://127.0.0.1:8899",
};

// Helper to load keypair from environment
async function getEnvKeyPair(envKey: string): Promise<KeyPairSigner> {
  if (!process.env[envKey]) {
    throw new Error(`Environment variable ${envKey} is not set`);
  }
  const base58Encoder = getBase58Encoder();
  const b58SecretEncoded = base58Encoder.encode(process.env[envKey]);
  return await createKeyPairSignerFromBytes(b58SecretEncoded);
}

// Get token balance from account data
async function getTokenBalance(
  rpc: ReturnType<typeof createSolanaRpc>,
  tokenAccount: string
): Promise<bigint> {
  try {
    const accountInfo = await rpc.getAccountInfo(address(tokenAccount), { 
      encoding: 'base64' 
    }).send();
    
    if (!accountInfo.value) {
      return 0n;
    }

    const data = Buffer.from(accountInfo.value.data[0], 'base64');
    
    // Token amount is at bytes 64-71 (u64 little-endian)
    if (data.length >= 72) {
      const amountBuffer = data.slice(64, 72);
      return amountBuffer.readBigUInt64LE(0);
    }
    
    return 0n;
  } catch (error) {
    console.error(`Error getting balance for ${tokenAccount}:`, error instanceof Error ? error.message : error);
    return 0n;
  }
}

// Check if ATA exists
async function ataExists(
  rpc: ReturnType<typeof createSolanaRpc>,
  ataAddress: string
): Promise<boolean> {
  try {
    const accountInfo = await rpc.getAccountInfo(address(ataAddress)).send();
    return accountInfo.value !== null;
  } catch {
    return false;
  }
}

// Burn tokens ONLY (keep account open)
async function burnTokens(
  rpc: ReturnType<typeof createSolanaRpc>,
  ownerKeypair: KeyPairSigner,
  tokenMint: string,
  accountName: string
): Promise<{ burned: boolean; balance: bigint; signature?: string; ataAddress?: string; error?: string }> {
  console.log(`\n  ${accountName} (${ownerKeypair.address.slice(0, 12)}...):`);

  try {
    // Get ATA
    const [ata] = await findAssociatedTokenPda({
      mint: address(tokenMint),
      owner: address(ownerKeypair.address),
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });

    // Check if ATA exists
    const exists = await ataExists(rpc, ata);
    if (!exists) {
      console.log(`    ℹ️  Token account doesn't exist`);
      return { burned: false, balance: 0n };
    }

    // Check current balance
    const balance = await getTokenBalance(rpc, ata);
    
    console.log(`    → ATA address: ${ata}`);
    console.log(`    → Current balance: ${balance} tokens (${Number(balance) / 1_000_000} USDC)`);

    if (balance === 0n) {
      console.log(`    ✓ Already empty (0 tokens)`);
      return { burned: false, balance: 0n, ataAddress: ata };
    }

    // Burn all tokens
    console.log(`    → Burning ${balance} tokens...`);
    const burnInstruction = getBurnInstruction({
      account: address(ata),
      mint: address(tokenMint),
      authority: ownerKeypair,
      amount: balance,
    });

    // Build transaction
    const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();
    
    const transaction = pipe(
      createTransactionMessage({ version: 0 }),
      (tx) => setTransactionMessageFeePayerSigner(ownerKeypair, tx),
      (tx) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
      (tx) => appendTransactionMessageInstructions([burnInstruction], tx),
    );

    // Sign transaction
    const signedTransaction = await signTransactionMessageWithSigners(transaction);
    const signature = getSignatureFromTransaction(signedTransaction);
    assertIsTransactionWithBlockhashLifetime(signedTransaction);

    // Send transaction
    await rpc.sendTransaction(signedTransaction as any, {
      encoding: 'base64',
      skipPreflight: false,
    }).send();

    // Wait for confirmation
    let confirmed = false;
    let attempts = 0;
    const maxAttempts = 30;

    while (!confirmed && attempts < maxAttempts) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      attempts++;
      
      try {
        const status = await rpc.getSignatureStatuses([signature]).send();
        const confirmationStatus = status.value[0]?.confirmationStatus;
        
        if (confirmationStatus === 'confirmed' || confirmationStatus === 'finalized') {
          confirmed = true;
        } else if (status.value[0]?.err) {
          throw new Error(`Transaction failed: ${JSON.stringify(status.value[0].err)}`);
        }
      } catch (error) {
        if (attempts >= maxAttempts) {
          throw error;
        }
      }
    }

    if (!confirmed) {
      throw new Error('Transaction confirmation timeout after 30 seconds');
    }

    console.log(`    ✓ Burned ${balance} tokens: ${signature.slice(0, 16)}...`);

    // Verify balance is now zero
    await new Promise(resolve => setTimeout(resolve, 1000));
    const newBalance = await getTokenBalance(rpc, ata);
    console.log(`    ✓ New balance: ${newBalance} tokens`);
    console.log(`    ✓ Account still exists (ready for Rust bot to close)`);

    if (newBalance > 0n) {
      console.log(`    ⚠️  Warning: Balance still shows ${newBalance} tokens`);
    }

    return { burned: true, balance, signature, ataAddress: ata };
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : 'Unknown error';
    console.error(`    ✗ Failed: ${errorMsg}`);
    return { burned: false, balance: 0n, error: errorMsg };
  }
}

async function main() {
  console.log("\n╔══════════════════════════════════════════════════════════════╗");
  console.log("║  PREPARE ACCOUNTS FOR RECLAIM - Burn Tokens (Keep Accounts)  ║");
  console.log("╚══════════════════════════════════════════════════════════════╝");

  try {
    // Setup
    console.log("\n[1/4] Initializing...");
    const rpc = createSolanaRpc(CONFIG.solanaRpcUrl);
    
    console.log("  → RPC:", CONFIG.solanaRpcUrl);

    // Load keypairs
    console.log("\n[2/4] Loading keypairs...");
    const tokenMint = await getEnvKeyPair("USDC_LOCAL_KEY");
    const testSender = await getEnvKeyPair("TEST_SENDER_KEYPAIR");
    const koraNode = await getEnvKeyPair("KORA_PRIVATE_KEY");
    const destination = await getEnvKeyPair("DESTINATION_KEYPAIR");
    
    console.log("  → Token mint:", tokenMint.address);
    console.log("  → Test sender:", testSender.address);
    console.log("  → Kora node:", koraNode.address);
    console.log("  → Destination:", destination.address);

    // Burn tokens (keep accounts open)
    console.log("\n[3/4] Burning tokens (keeping accounts open)...");
    console.log("  Note: Accounts will remain open with 0 token balance");
    console.log("  Note: Your Rust bot will close them to reclaim rent");

    const results = [];

    // Burn TEST_SENDER's tokens
    const result1 = await burnTokens(
      rpc,
      testSender,
      tokenMint.address,
      "TEST_SENDER"
    );
    results.push({ account: "TEST_SENDER", address: testSender.address, ...result1 });

    await new Promise(resolve => setTimeout(resolve, 1000));

    // Burn KORA_NODE's tokens
    const result2 = await burnTokens(
      rpc,
      koraNode,
      tokenMint.address,
      "KORA_NODE"
    );
    results.push({ account: "KORA_NODE", address: koraNode.address, ...result2 });

    await new Promise(resolve => setTimeout(resolve, 1000));

    // Burn DESTINATION's tokens
    const result3 = await burnTokens(
      rpc,
      destination,
      tokenMint.address,
      "DESTINATION"
    );
    results.push({ account: "DESTINATION", address: destination.address, ...result3 });

    // Summary
    console.log("\n[4/4] Summary");
    console.log("═".repeat(70));
    
    const burnedCount = results.filter(r => r.burned).length;
    const alreadyEmptyCount = results.filter(r => !r.burned && r.balance === 0n && !r.error).length;
    const failedCount = results.filter(r => r.error).length;

    console.log(`  ✓ Successfully burned: ${burnedCount} accounts`);
    console.log(`  ℹ️  Already empty: ${alreadyEmptyCount} accounts`);
    if (failedCount > 0) {
      console.log(`  ✗ Failed: ${failedCount}`);
      console.log("\n  Failed accounts:");
      results
        .filter(r => r.error)
        .forEach(r => {
          console.log(`    - ${r.account}: ${r.error || 'Unknown error'}`);
        });
    }

    console.log("\n╔══════════════════════════════════════════════════════════════╗");
    console.log("║  TOKENS BURNED ✓ - Accounts Ready for Reclaim                ║");
    console.log("╚══════════════════════════════════════════════════════════════╝");

    console.log("\n📋 Empty Token Accounts (Ready for Rust Bot):");
    results.forEach(r => {
      if (r.ataAddress) {
        console.log(`  ✓ ${r.account}:`);
        console.log(`    Owner: ${r.address}`);
        console.log(`    ATA:   ${r.ataAddress}`);
        console.log(`    Balance: 0 tokens (READY TO CLOSE)`);
      }
    });

    console.log("\n💡 Expected reclaim per account:");
    console.log("  • ~0.00203928 SOL per SPL token account");
    
    const emptyAccounts = burnedCount + alreadyEmptyCount;
    if (emptyAccounts > 0) {
      const expectedRent = emptyAccounts * 0.00203928;
      console.log(`  • Total: ~${expectedRent.toFixed(8)} SOL from ${emptyAccounts} accounts`);
    }

    console.log("\n📋 Next steps:");
    console.log("  1. Verify accounts are empty (check above)");
    console.log("  2. Run your Rust reclaim bot:");
    console.log("     MIN_INACTIVE_DAYS=0 cargo run");
    console.log("  3. Check Kora's balance after reclaim:");
    console.log(`     solana balance ${koraNode.address}\n`);

    console.log("⚠️  Important:");
    console.log("  • Set MIN_INACTIVE_DAYS=0 for testing");
    console.log("  • Make sure Kora is the close_authority (from setup.ts)");
    console.log("  • Accounts must have 0 token balance to be reclaimable");
    console.log("  • Accounts are still OPEN (not closed yet)\n");

    // Save ATA addresses for reference
    console.log("📝 Copy these ATA addresses to verify with spl-token:");
    results.forEach(r => {
      if (r.ataAddress) {
        console.log(`  spl-token account-info --address ${r.ataAddress}`);
      }
    });
    console.log();

  } catch (error) {
    console.error("\n╔══════════════════════════════════════════════════════════════╗");
    console.error("║  ERROR: Failed to burn tokens                                  ║");
    console.error("╚══════════════════════════════════════════════════════════════╝");
    console.error("\nDetails:", error);
    process.exit(1);
  }
}

main().catch((e) => {
  console.error("Error:", e);
  process.exit(1);
});