/**
 * SIMPLE BURN SCRIPT - No complex helpers, just burns tokens
 */
import {
  createKeyPairSignerFromBytes,
  getBase58Encoder,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  pipe,
  createTransactionMessage,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  appendTransactionMessageInstructions,
  signTransactionMessageWithSigners,
  getSignatureFromTransaction,
  assertIsTransactionWithBlockhashLifetime,
  sendAndConfirmTransactionFactory,
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

async function getEnvKeyPair(envKey: string) {
  if (!process.env[envKey]) {
    throw new Error(`Environment variable ${envKey} is not set`);
  }
  const base58Encoder = getBase58Encoder();
  return await createKeyPairSignerFromBytes(base58Encoder.encode(process.env[envKey]));
}

async function main() {
  console.log("\n🔥 BURNING TOKENS...\n");

  const rpc = createSolanaRpc("http://127.0.0.1:8899");
  const rpcSubscriptions = createSolanaRpcSubscriptions("ws://127.0.0.1:8900");

  // Load keypairs
  const mint = await getEnvKeyPair("USDC_LOCAL_KEY");
  const testSender = await getEnvKeyPair("TEST_SENDER_KEYPAIR");
  const koraNode = await getEnvKeyPair("KORA_PRIVATE_KEY");
  const destination = await getEnvKeyPair("DESTINATION_KEYPAIR");

  console.log("📋 Loaded accounts");
  console.log(`  Mint: ${mint.address}`);
  console.log(`  Test Sender: ${testSender.address}`);
  console.log(`  Kora Node: ${koraNode.address}`);
  console.log(`  Destination: ${destination.address}\n`);

  const wallets = [
    { name: "TEST_SENDER", keypair: testSender },
    { name: "KORA_NODE", keypair: koraNode },
    { name: "DESTINATION", keypair: destination },
  ];

  for (const wallet of wallets) {
    console.log(`\n━━━ ${wallet.name} ━━━`);

    // Find ATA
    const [ataAddr] = await findAssociatedTokenPda({
      mint: address(mint.address),
      owner: address(wallet.keypair.address),
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });

    console.log(`  ATA: ${ataAddr}`);

    // Get account info directly
    const accountInfo = await rpc.getAccountInfo(address(ataAddr), {
      encoding: 'base64'
    }).send();

    if (!accountInfo.value) {
      console.log(`  ❌ Account doesn't exist!`);
      console.log(`  This is weird - diagnose showed it exists. Check your validator.`);
      continue;
    }

    // Parse balance
    const data = Buffer.from(accountInfo.value.data[0], 'base64');
    const balance = data.slice(64, 72).readBigUInt64LE(0);
    
    console.log(`  Current balance: ${balance} (${Number(balance) / 1_000_000} USDC)`);

    if (balance === 0n) {
      console.log(`  ✓ Already empty`);
      continue;
    }

    // Burn instruction
    const burnIx = getBurnInstruction({
      account: address(ataAddr),
      mint: address(mint.address),
      authority: wallet.keypair,
      amount: balance,
    });

    console.log(`  🔥 Burning ${Number(balance) / 1_000_000} USDC...`);

    // Build and send transaction
    try {
      const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();
      
      const tx = pipe(
        createTransactionMessage({ version: 0 }),
        (t) => setTransactionMessageFeePayerSigner(wallet.keypair, t),
        (t) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, t),
        (t) => appendTransactionMessageInstructions([burnIx], t),
      );

      const signedTx = await signTransactionMessageWithSigners(tx);
      assertIsTransactionWithBlockhashLifetime(signedTx);

      await sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions })(signedTx, {
        commitment: 'confirmed',
      });

      const sig = getSignatureFromTransaction(signedTx);
      console.log(`  ✅ Burned! Sig: ${sig.slice(0, 16)}...`);

      // Verify
      await new Promise(r => setTimeout(r, 1000));
      const newInfo = await rpc.getAccountInfo(address(ataAddr), {
        encoding: 'base64'
      }).send();
      
      if (newInfo.value) {
        const newData = Buffer.from(newInfo.value.data[0], 'base64');
        const newBalance = newData.slice(64, 72).readBigUInt64LE(0);
        console.log(`  ✓ New balance: ${newBalance}`);
        
        if (newBalance === 0n) {
          console.log(`  ✓ Account is now empty and ready for rent reclaim!`);
        }
      }

    } catch (error) {
      console.error(`  ❌ Failed to burn:`, error instanceof Error ? error.message : error);
    }
  }

  console.log("\n✨ Done!\n");
}

main().catch(e => {
  console.error("Error:", e);
  process.exit(1);
});