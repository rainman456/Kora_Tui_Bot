/**
 * Diagnostic script to check what actually exists on-chain
 */
import {
  createKeyPairSignerFromBytes,
  getBase58Encoder,
  createSolanaRpc,
  address,
} from "@solana/kit";
import {
  findAssociatedTokenPda,
  TOKEN_PROGRAM_ADDRESS,
} from "@solana-program/token";
import dotenv from "dotenv";
import path from "path";

dotenv.config({ path: path.join(process.cwd(), "..", ".env") });

const CONFIG = {
  solanaRpcUrl: "http://127.0.0.1:8899",
};

async function getEnvKeyPair(envKey: string) {
  if (!process.env[envKey]) {
    throw new Error(`Environment variable ${envKey} is not set`);
  }
  const base58Encoder = getBase58Encoder();
  const b58SecretEncoded = base58Encoder.encode(process.env[envKey]);
  return await createKeyPairSignerFromBytes(b58SecretEncoded);
}

async function checkAccount(rpc: any, addressStr: string, label: string) {
  console.log(`\n${label}:`);
  console.log(`  Address: ${addressStr}`);
  
  try {
    const accountInfo = await rpc.getAccountInfo(address(addressStr), {
      encoding: 'base64'
    }).send();
    
    if (!accountInfo.value) {
      console.log(`  ❌ Does NOT exist`);
      return false;
    }
    
    console.log(`  ✅ EXISTS`);
    console.log(`  Owner: ${accountInfo.value.owner}`);
    console.log(`  Lamports: ${accountInfo.value.lamports}`);
    console.log(`  Data length: ${accountInfo.value.data[0] ? Buffer.from(accountInfo.value.data[0], 'base64').length : 0} bytes`);
    
    // If it's a token account, parse the balance
    if (accountInfo.value.data[0]) {
      const data = Buffer.from(accountInfo.value.data[0], 'base64');
      if (data.length >= 72) {
        const amount = data.slice(64, 72).readBigUInt64LE(0);
        console.log(`  Token balance: ${amount} (${Number(amount) / 1_000_000} USDC)`);
      }
    }
    
    return true;
  } catch (error) {
    console.log(`  ❌ Error checking: ${error instanceof Error ? error.message : error}`);
    return false;
  }
}

async function main() {
  console.log("🔍 DIAGNOSTIC: Checking on-chain state...\n");
  console.log("═".repeat(70));

  const rpc = createSolanaRpc(CONFIG.solanaRpcUrl);

  // Load keypairs
  const tokenMint = await getEnvKeyPair("USDC_LOCAL_KEY");
  const testSender = await getEnvKeyPair("TEST_SENDER_KEYPAIR");
  const koraNode = await getEnvKeyPair("KORA_PRIVATE_KEY");
  const destination = await getEnvKeyPair("DESTINATION_KEYPAIR");

  console.log("\n📋 Keypair Addresses:");
  console.log(`  Mint:        ${tokenMint.address}`);
  console.log(`  Test Sender: ${testSender.address}`);
  console.log(`  Kora Node:   ${koraNode.address}`);
  console.log(`  Destination: ${destination.address}`);

  console.log("\n" + "═".repeat(70));
  console.log("CHECKING ACCOUNTS ON-CHAIN");
  console.log("═".repeat(70));

  // Check mint account
  await checkAccount(rpc, tokenMint.address, "🪙 Mint Account");

  // Check wallet SOL balances
  console.log("\n💰 SOL Balances:");
  const walletsToCheck: Array<[string, typeof testSender]> = [
    ["Test Sender", testSender],
    ["Kora Node", koraNode],
    ["Destination", destination],
  ];
  
  for (const [name, keypair] of walletsToCheck) {
    try {
      const balance = await rpc.getBalance(address(keypair.address)).send();
      console.log(`  ${name}: ${Number(balance.value) / 1_000_000_000} SOL`);
    } catch (e) {
      console.log(`  ${name}: Error getting balance`);
    }
  }

  // Check ATAs
  console.log("\n" + "═".repeat(70));
  console.log("CHECKING ASSOCIATED TOKEN ACCOUNTS (ATAs)");
  console.log("═".repeat(70));

  const atasToCheck: Array<[string, typeof testSender]> = [
    ["TEST_SENDER", testSender],
    ["KORA_NODE", koraNode],
    ["DESTINATION", destination],
  ];

  for (const [name, keypair] of atasToCheck) {
    const [ata] = await findAssociatedTokenPda({
      mint: address(tokenMint.address),
      owner: address(keypair.address),
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });

    await checkAccount(rpc, ata, `📦 ${name} ATA`);
  }

  console.log("\n" + "═".repeat(70));
  console.log("\n💡 What this means:");
  console.log("  • If Mint does NOT exist → setup.ts hasn't run or failed");
  console.log("  • If Mint exists but ATAs don't → bug in setup.ts (not creating ATAs)");
  console.log("  • If ATAs exist with 0 balance → ready for reclaim (good!)");
  console.log("  • If ATAs exist with tokens → need to run empty.ts first\n");
}

main().catch(e => {
  console.error("Error:", e);
  process.exit(1);
});