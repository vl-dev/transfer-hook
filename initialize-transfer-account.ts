import {
    Connection,
    Keypair,
    PublicKey,
    SystemProgram,
    Transaction,
    TransactionInstruction,
    sendAndConfirmTransaction,
} from '@solana/web3.js';
import * as fs from 'fs';

// Constants
const INITIALIZE_TRANSFER_ACCOUNT_DISCRIMINATOR = 255;

/**
 * Create an instruction to initialize a transfer account
 */
function createInitializeTransferAccountInstruction(
    programId: PublicKey,
    owner: PublicKey,
    transferAccount: PublicKey,
): TransactionInstruction {
    return new TransactionInstruction({
        programId,
        keys: [
            { pubkey: owner, isSigner: true, isWritable: true },
            { pubkey: transferAccount, isSigner: false, isWritable: true },
            { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: Buffer.from([INITIALIZE_TRANSFER_ACCOUNT_DISCRIMINATOR]),
    });
}

/**
 * Derive the transfer account address for an owner
 * This matches the derivation in accounts-config.json (index 3 = authority/owner)
 */
async function getTransferAccountAddress(
    owner: PublicKey,
    programId: PublicKey,
): Promise<PublicKey> {
    return (await PublicKey.findProgramAddressSync([owner.toBytes()], programId))[0];
}

async function main() {
    // Configuration
    const RPC_URL = process.env.RPC_URL || 'http://localhost:8899';
    const PROGRAM_ID = new PublicKey(
        process.env.PROGRAM_ID || 'TokenHookExampLe8smaVNrxTBezWTRbEwxwb1Zykrb'
    );

    // Load payer keypair - use the same keypair that solana CLI uses
    let payerKeypairPath = process.env.PAYER_KEYPAIR;

    if (!payerKeypairPath) {
        // Read from solana config to get the keypair path
        const configPath = `${process.env.HOME}/.config/solana/cli/config.yml`;
        if (fs.existsSync(configPath)) {
            const config = fs.readFileSync(configPath, 'utf-8');
            const match = config.match(/keypair_path:\s*(.+)/);
            if (match) {
                payerKeypairPath = match[1].trim();
            }
        }
    }

    if (!payerKeypairPath) {
        payerKeypairPath = `${process.env.HOME}/.config/solana/id.json`;
    }

    const payerKeypair = Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(fs.readFileSync(payerKeypairPath, 'utf-8')))
    );

    console.log('Payer:', payerKeypair.publicKey.toBase58());
    console.log('Program ID:', PROGRAM_ID.toBase58());

    // Connect
    const connection = new Connection(RPC_URL, 'confirmed');

    // Check balance
    const balance = await connection.getBalance(payerKeypair.publicKey);
    console.log('Balance:', balance / 1e9, 'SOL');

    // Derive transfer account address from the payer (index 3 = authority)
    const transferAccount = await getTransferAccountAddress(
        payerKeypair.publicKey,
        PROGRAM_ID
    );
    console.log('Transfer account:', transferAccount.toBase58());

    // Check if already exists
    const accountInfo = await connection.getAccountInfo(transferAccount);
    if (accountInfo) {
        console.log('Transfer account already exists!');
        console.log('Owner:', accountInfo.owner.toBase58());
        console.log('Data length:', accountInfo.data.length);
        return;
    }

    // Create instruction
    const instruction = createInitializeTransferAccountInstruction(
        PROGRAM_ID,
        payerKeypair.publicKey,
        transferAccount
    );

    // Build and send transaction
    const transaction = new Transaction().add(instruction);
    transaction.feePayer = payerKeypair.publicKey;

    console.log('\nSending transaction...');
    const signature = await sendAndConfirmTransaction(
        connection,
        transaction,
        [payerKeypair],
        {
            commitment: 'confirmed',
            preflightCommitment: 'confirmed',
        }
    );

    console.log('✅ Transfer account initialized!');
    console.log('Signature:', signature);
    console.log('Transfer account:', transferAccount.toBase58());

    // Verify
    const newAccountInfo = await connection.getAccountInfo(transferAccount);
    if (newAccountInfo) {
        console.log('\nAccount verified:');
        console.log('- Owner:', newAccountInfo.owner.toBase58());
        console.log('- Data length:', newAccountInfo.data.length);
        console.log('- Lamports:', newAccountInfo.lamports);
    }
}

main().catch((err) => {
    console.error('Error:', err);
    process.exit(1);
});

