/**
 * Simple script to verify mint and create test ATAs with tokens
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
  Instruction,
  address,
} from "@solana/kit";
import {
  findAssociatedTokenPda,
  TOKEN_PROGRAM_ADDRESS,
  getCreateAssociatedTokenIdempotentInstructionAsync,
  getMintToInstruction,
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
  console.log("\n🔧 QUICK FIX: Creating ATAs and minting tokens...\n");

  const rpc = createSolanaRpc("http://127.0.0.1:8899");
  const rpcSubscriptions = createSolanaRpcSubscriptions("ws://127.0.0.1:8900");

  // Load keys
  const mint = await getEnvKeyPair("USDC_LOCAL_KEY");
  const testSender = await getEnvKeyPair("TEST_SENDER_KEYPAIR");
  const kora = await getEnvKeyPair("KORA_PRIVATE_KEY");
  const destination = await getEnvKeyPair("DESTINATION_KEYPAIR");
  const mintAuthority = await getEnvKeyPair("MINT_AUTHORITY");

  console.log("📋 Accounts:");
  console.log(`  Mint: ${mint.address}`);
  console.log(`  Test Sender: ${testSender.address}`);
  console.log(`  Destination: ${destination.address}`);
  console.log(`  Mint Authority: ${mintAuthority.address}\n`);

  // Check if mint exists
  console.log("🔍 Checking mint account...");
  const mintAccount = await rpc.getAccountInfo(address(mint.address), {
    encoding: 'base64'
  }).send();
  if (!mintAccount.value) {
    console.log("❌ Mint account doesn't exist! Run setup.ts first.");
    process.exit(1);
  }
  console.log("✅ Mint account exists\n");

  // For each wallet, create ATA and mint tokens
  const wallets = [
    { name: "TEST_SENDER", keypair: testSender },
    { name: "DESTINATION", keypair: destination },
  ];

  for (const wallet of wallets) {
    console.log(`\n🔨 Processing ${wallet.name}...`);

    // Get ATA address
    const [ata] = await findAssociatedTokenPda({
      mint: address(mint.address),
      owner: address(wallet.keypair.address),
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });

    console.log(`  ATA: ${ata}`);

    // Check if ATA exists
    const ataAccount = await rpc.getAccountInfo(address(ata), {
      encoding: 'base64'
    }).send();
    
    if (ataAccount.value) {
      // Check balance
      const data = Buffer.from(ataAccount.value.data[0], 'base64');
      const balance = data.slice(64, 72).readBigUInt64LE(0);
      console.log(`  ✓ ATA already exists with ${balance} tokens`);
      
      if (balance > 0) {
        console.log(`  ℹ️  Skipping (already has tokens)`);
        continue;
      }
    }

    // Create instructions
    const instructions: Instruction[] = [];

    // Create ATA if it doesn't exist
    if (!ataAccount.value) {
      console.log(`  → Creating ATA...`);
      instructions.push(
        await getCreateAssociatedTokenIdempotentInstructionAsync({
          mint: address(mint.address),
          payer: kora,
          owner: address(wallet.keypair.address),
        })
      );
    }

    // Mint tokens
    console.log(`  → Minting 100,000 tokens...`);
    instructions.push(
      getMintToInstruction({
        mint: address(mint.address),
        token: address(ata),
        amount: BigInt(100_000 * 1_000_000), // 100k with 6 decimals
        mintAuthority,
      })
    );

    // Send transaction
    const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();
    
    const tx = pipe(
      createTransactionMessage({ version: 0 }),
      (t) => setTransactionMessageFeePayerSigner(kora, t),
      (t) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, t),
      (t) => appendTransactionMessageInstructions(instructions, t),
    );

    const signedTx = await signTransactionMessageWithSigners(tx);
    assertIsTransactionWithBlockhashLifetime(signedTx);
    
    await sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions })(signedTx, {
      commitment: 'confirmed',
    });

    const sig = getSignatureFromTransaction(signedTx);
    console.log(`  ✅ Success: ${sig.slice(0, 16)}...`);
  }

  console.log("\n✨ Done! ATAs created and tokens minted.");
  console.log("Now run: bun empty\n");
}

main().catch(e => {
  console.error("Error:", e);
  process.exit(1);
});