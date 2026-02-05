// Fixed version - mints tokens to ALL wallets, not just the owner
import { assertKeyGenerationIsAvailable } from "@solana/assertions";
import { getCreateAccountInstruction } from "@solana-program/system";
import {
    findAssociatedTokenPda,
    getCreateAssociatedTokenIdempotentInstructionAsync,
    getInitializeMintInstruction,
    getMintSize,
    getMintToInstruction,
    TOKEN_PROGRAM_ADDRESS,
    getSetAuthorityInstruction,
    AuthorityType,
} from "@solana-program/token";
import {
    airdropFactory,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    lamports,
    sendAndConfirmTransactionFactory,
    pipe,
    createTransactionMessage,
    setTransactionMessageLifetimeUsingBlockhash,
    setTransactionMessageFeePayerSigner,
    appendTransactionMessageInstructions,
    TransactionSigner,
    SolanaRpcApi,
    RpcSubscriptions,
    Rpc,
    SolanaRpcSubscriptionsApi,
    MicroLamports,
    Commitment,
    Signature,
    signTransactionMessageWithSigners,
    getSignatureFromTransaction,
    Instruction,
    createKeyPairSignerFromBytes,
    getBase58Decoder,
    getBase58Encoder,
    KeyPairSigner,
    TransactionMessage,
    assertIsTransactionWithBlockhashLifetime,
    TransactionMessageWithSigners,
    TransactionMessageWithFeePayer,
} from "@solana/kit";
import {
    updateOrAppendSetComputeUnitLimitInstruction,
    updateOrAppendSetComputeUnitPriceInstruction,
    MAX_COMPUTE_UNIT_LIMIT,
    estimateComputeUnitLimitFactory
} from "@solana-program/compute-budget";
import { appendFile } from 'fs/promises';
import path from "path";
import dotenv from "dotenv";

dotenv.config({ path: path.join(process.cwd(), '..', '.env') });

const LAMPORTS_PER_SOL = BigInt(1_000_000_000);
const DECIMALS = 6;
const DROP_AMOUNT = 100_000;

interface Client {
    rpc: Rpc<SolanaRpcApi>;
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
}

export const createDefaultTransaction = async (
    client: Client,
    feePayer: TransactionSigner,
    computeLimit: number = MAX_COMPUTE_UNIT_LIMIT,
    feeMicroLamports: MicroLamports = 1n as MicroLamports
): Promise<TransactionMessage & TransactionMessageWithFeePayer & TransactionMessageWithSigners> => {
    const { value: latestBlockhash } = await client.rpc
        .getLatestBlockhash()
        .send();
    return pipe(
        createTransactionMessage({ version: 0 }),
        (tx) => setTransactionMessageFeePayerSigner(feePayer, tx),
        (tx) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
        (tx) => updateOrAppendSetComputeUnitPriceInstruction(feeMicroLamports, tx),
        (tx) => updateOrAppendSetComputeUnitLimitInstruction(computeLimit, tx),
    );
};

export const signAndSendTransaction = async (
    client: Client,
    transactionMessage: TransactionMessage & TransactionMessageWithFeePayer & TransactionMessageWithSigners,
    commitment: Commitment = 'confirmed'
) => {
    const signedTransaction =
        await signTransactionMessageWithSigners(transactionMessage);
    const signature = getSignatureFromTransaction(signedTransaction);
    assertIsTransactionWithBlockhashLifetime(signedTransaction);
    await sendAndConfirmTransactionFactory(client)(signedTransaction, {
        commitment,
    });
    return signature;
};

async function sendAndConfirmInstructions(
    client: Client,
    payer: TransactionSigner,
    instructions: Instruction[],
    description: string,
    additionalSigners: KeyPairSigner[] = []
): Promise<Signature> {
    try {
        const simulationTx = await pipe(
            await createDefaultTransaction(client, payer),
            (tx) => appendTransactionMessageInstructions(instructions, tx),
        );
        const estimateCompute = estimateComputeUnitLimitFactory({ rpc: client.rpc });
        const computeUnitLimit = await estimateCompute(simulationTx);
        const signature = await pipe(
            await createDefaultTransaction(client, payer, computeUnitLimit),
            (tx) => appendTransactionMessageInstructions(instructions, tx),
            (tx) => signAndSendTransaction(client, tx)
        );
        console.log(`    - ${description} - Signature: ${signature}`);

        return signature;
    } catch (error) {
        throw new Error(`Failed to ${description.toLowerCase()}: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
}

async function createB58SecretKey(): Promise<string> {
    await assertKeyGenerationIsAvailable();
    const base58Decoder = getBase58Decoder();
    const keyPair = await crypto.subtle.generateKey(
        "Ed25519",
        true,
        ["sign", "verify"],
    );

    const pkcs8ArrayBuffer = await crypto.subtle.exportKey("pkcs8", keyPair.privateKey);
    const pkcs8Bytes = new Uint8Array(pkcs8ArrayBuffer);
    const rawPrivateKey = pkcs8Bytes.slice(-32);

    const publicKeyArrayBuffer = await crypto.subtle.exportKey("raw", keyPair.publicKey);
    const publicKeyBytes = new Uint8Array(publicKeyArrayBuffer);

    const solanaSecretKey = new Uint8Array(64);
    solanaSecretKey.set(rawPrivateKey, 0);
    solanaSecretKey.set(publicKeyBytes, 32);
    return base58Decoder.decode(solanaSecretKey);
}

const createKeyPairSignerFromB58Secret = async (b58Secret: string) => {
    const base58Encoder = getBase58Encoder();
    return await createKeyPairSignerFromBytes(base58Encoder.encode(b58Secret));
}

const addKeypairToEnvFile = async (
    variableName: string,
    envPath: string = path.join(process.cwd(), '..'),
    envFileName: string = ".env",
    b58Secret?: string
) => {
    if (!b58Secret) b58Secret = await createB58SecretKey();
    const keypairSigner = await createKeyPairSignerFromB58Secret(b58Secret);
    const fullPath = path.join(envPath, envFileName);
    try {
        await appendFile(fullPath, `\n# Solana Address: ${keypairSigner.address}\n${variableName}=${b58Secret}\n`);
        console.log(`${variableName} added to env file successfully`);
        return keypairSigner;
    } catch (e) { throw e; }
};

async function getOrCreateEnvKeyPair(envKey: string) {
    if (process.env[envKey]) return await createKeyPairSignerFromB58Secret(process.env[envKey]);
    return await addKeypairToEnvFile(envKey);
}

// FIXED: Mint tokens to ALL specified wallets, not just owner
async function initializeToken({
    client,
    mintAuthority,
    payer,
    mint,
    dropAmount,
    decimals,
    walletsToFund, // Changed name to be clearer
    koraCloseAuthority,
}: {
    client: Client,
    mintAuthority: KeyPairSigner<string>,
    payer: KeyPairSigner<string>,
    mint: KeyPairSigner<string>,
    dropAmount: number,
    decimals: number,
    walletsToFund: KeyPairSigner<string>[], // These wallets will get ATAs AND tokens
    koraCloseAuthority?: KeyPairSigner<string>,
}) {
    // Get Mint size & rent
    const mintSpace = BigInt(getMintSize());
    const mintRent = await client.rpc.getMinimumBalanceForRentExemption(mintSpace).send();

    // Create mint account and initialize it
    const baseInstructions = [
        getCreateAccountInstruction({
            payer,
            newAccount: mint,
            lamports: mintRent,
            space: mintSpace,
            programAddress: TOKEN_PROGRAM_ADDRESS,
        }),
        getInitializeMintInstruction({
            mint: mint.address,
            decimals: decimals,
            mintAuthority: mintAuthority.address
        }),
    ];

    // Create ATAs and mint tokens to ALL wallets
    const ataMintInstructions = [];
    for (const wallet of walletsToFund) {
        // Get ATA address
        const [ata] = await findAssociatedTokenPda({
            mint: mint.address,
            owner: wallet.address,
            tokenProgram: TOKEN_PROGRAM_ADDRESS,
        });

        // Create ATA
        ataMintInstructions.push(
            await getCreateAssociatedTokenIdempotentInstructionAsync({
                mint: mint.address,
                payer,
                owner: wallet.address,
            })
        );

        // Mint tokens to this ATA
        ataMintInstructions.push(
            getMintToInstruction({
                mint: mint.address,
                token: ata,
                amount: BigInt(dropAmount * 10 ** decimals),
                mintAuthority,
            })
        );
    }

    // Send transaction to create mint and all ATAs with tokens
    await sendAndConfirmInstructions(
        client,
        payer,
        [...baseInstructions, ...ataMintInstructions],
        'Mint account created, ATAs initialized, and tokens minted'
    );

    console.log(`\n✅ Initialized token ${mint.address}`);
    console.log(`✅ Dropped ${dropAmount} tokens to ${walletsToFund.length} wallets:`);
    walletsToFund.forEach(w => console.log(`   - ${w.address}`));

    // Set Kora as close authority for all ATAs
    if (koraCloseAuthority) {
        console.log(`\n🔑 Setting Kora node (${koraCloseAuthority.address}) as close authority for ATAs...`);

        for (const wallet of walletsToFund) {
            const [walletAta] = await findAssociatedTokenPda({
                mint: mint.address,
                owner: wallet.address,
                tokenProgram: TOKEN_PROGRAM_ADDRESS,
            });

            const setAuthorityIx = getSetAuthorityInstruction({
                owned: walletAta,
                owner: wallet,
                authorityType: AuthorityType.CloseAccount,
                newAuthority: koraCloseAuthority.address,
            });

            await sendAndConfirmInstructions(
                client,
                payer,
                [setAuthorityIx],
                `Set close authority for ${wallet.address.slice(0, 8)}...`,
                [wallet]
            );

            console.log(`  ✓ Set close authority for ${wallet.address.slice(0, 8)}...'s ATA`);
        }

        console.log(`✅ Kora node can now reclaim rent from empty ATAs!`);
    }
}

async function main() {
    console.log('\n🚀 Starting FIXED setup for Kora rent reclaim testing...\n');
    console.log('═'.repeat(70));

    // 1 - Create client
    const httpEndpoint = 'http://127.0.0.1:8899';
    const wsEndpoint = 'ws://127.0.0.1:8900';
    const rpc = createSolanaRpc(httpEndpoint);
    const rpcSubscriptions = createSolanaRpcSubscriptions(wsEndpoint);
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    const client: Client = { rpc, rpcSubscriptions };

    console.log('📡 Connected to local validator');

    // 2 - Get or create keypairs
    console.log('🔑 Loading keypairs...');
    const USDC_LOCAL_KEY = await getOrCreateEnvKeyPair('USDC_LOCAL_KEY');
    const TEST_SENDER_KEYPAIR = await getOrCreateEnvKeyPair('TEST_SENDER_KEYPAIR');
    const KORA_PRIVATE_KEY = await getOrCreateEnvKeyPair('KORA_PRIVATE_KEY');
    const MINT_AUTHORITY = await getOrCreateEnvKeyPair('MINT_AUTHORITY');
    const DESTINATION_KEYPAIR = await getOrCreateEnvKeyPair('DESTINATION_KEYPAIR');

    console.log(`   Test Sender: ${TEST_SENDER_KEYPAIR.address}`);
    console.log(`   Kora Node:   ${KORA_PRIVATE_KEY.address}`);
    console.log(`   Destination: ${DESTINATION_KEYPAIR.address}\n`);

    // 3 - Airdrop SOL
    console.log('💰 Airdropping SOL...');
    await Promise.all([
        airdrop({ commitment: 'processed', lamports: lamports(LAMPORTS_PER_SOL), recipientAddress: KORA_PRIVATE_KEY.address }),
        airdrop({ commitment: 'processed', lamports: lamports(LAMPORTS_PER_SOL), recipientAddress: TEST_SENDER_KEYPAIR.address }),
        airdrop({ commitment: 'processed', lamports: lamports(LAMPORTS_PER_SOL), recipientAddress: MINT_AUTHORITY.address }),
    ]);
    console.log('   ✓ 1 SOL → Test Sender, Kora Node, Mint Authority\n');

    // 4 - Execute initializeToken - FIXED: mint to all wallets
    console.log('🪙  Initializing token with Kora close authority...');
    console.log('═'.repeat(70));

    await initializeToken({
        client,
        mintAuthority: MINT_AUTHORITY,
        payer: KORA_PRIVATE_KEY,
        mint: USDC_LOCAL_KEY,
        dropAmount: DROP_AMOUNT,
        decimals: DECIMALS,
        // FIXED: These wallets will ALL receive tokens now
        walletsToFund: [TEST_SENDER_KEYPAIR, KORA_PRIVATE_KEY, DESTINATION_KEYPAIR],
        koraCloseAuthority: KORA_PRIVATE_KEY,
    });

    console.log('\n═'.repeat(70));
    console.log('\n✨ Setup complete! You can now test rent reclaim.');
}

main().catch(e => console.error('❌ Error:', e));