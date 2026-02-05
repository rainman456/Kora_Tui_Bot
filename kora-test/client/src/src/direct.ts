/**
 * Direct test - query the exact ATA addresses we know exist
 */
import { createSolanaRpc, address } from "@solana/kit";

const rpc = createSolanaRpc("http://127.0.0.1:8899");

// These are the exact ATA addresses from your diagnostic output:
const atas = [
  { name: "TEST_SENDER", addr: "B6hN1nwMzWHneoiYWgEMZLT4QrDuv6dzPzBgBtnqEedq" },
  { name: "KORA_NODE", addr: "4yL5euc555kMYVe4daUomXqrP8HsSNg3pDgBRwmn9bRb" },
  { name: "DESTINATION", addr: "4JTLq873So2TgXiHKcVg4RRrW2aHiomZCFGr7ZgnP6Ak" },
];

async function test() {
  console.log("🔍 Testing direct ATA queries...\n");

  for (const ata of atas) {
    console.log(`${ata.name}: ${ata.addr}`);
    
    try {
      const info = await rpc.getAccountInfo(address(ata.addr), {
        encoding: 'base64'
      }).send();
      
      if (info.value) {
        const data = Buffer.from(info.value.data[0], 'base64');
        const balance = data.slice(64, 72).readBigUInt64LE(0);
        console.log(`  ✅ EXISTS - Balance: ${balance} (${Number(balance) / 1_000_000} USDC)\n`);
      } else {
        console.log(`  ❌ DOES NOT EXIST\n`);
      }
    } catch (e) {
      console.log(`  ❌ ERROR:`, e instanceof Error ? e.message : e, "\n");
    }
  }
}

test();