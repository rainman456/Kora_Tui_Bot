// import { getBase58Encoder } from "@solana/kit";
// import fs from "fs";
// import dotenv from "dotenv";

// dotenv.config({ path: "../.env" });

// const privateKeyString = process.env.KORA_PRIVATE_KEY;
// if (!privateKeyString) throw new Error("KORA_PRIVATE_KEY not found in .env");

// // Use the new @solana/kit encoder to turn string into bytes
// const base58Encoder = getBase58Encoder();
// const secretKeyBytes = base58Encoder.encode(privateKeyString);

// // Convert to JSON array for the Rust bot
// const jsonContent = JSON.stringify(Array.from(secretKeyBytes));

// fs.writeFileSync("../kora-wallet.json", jsonContent);

// console.log(`✅ Exported Keypair to ../kora-wallet.json`);
// console.log(`✅ Exported Keypair!`);

// client/src/export-key.ts
import { Keypair } from "@solana/web3.js";
import bs58 from "bs58"; // or the encoder you used in setup
import fs from "fs";
import dotenv from "dotenv";

dotenv.config({ path: "../.env" });

const privateKeyString = process.env.KORA_PRIVATE_KEY;
if (!privateKeyString) throw new Error("KORA_PRIVATE_KEY not found in .env");

// Decode and get keypair
const secretKey = bs58.decode(privateKeyString);
const keypair = Keypair.fromSecretKey(secretKey);

// Format as JSON array of numbers (standard Solana format)
const jsonContent = JSON.stringify(Array.from(secretKey));

fs.writeFileSync("../kora-wallet.json", jsonContent);

console.log(`✅ Exported Keypair!`);
console.log(`Public Key: ${keypair.publicKey.toBase58()}`);
console.log(`Saved to: ../kora-wallet.json`);