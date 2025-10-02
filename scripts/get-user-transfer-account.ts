import {
    Connection,
    Keypair,
    PublicKey,
} from '@solana/web3.js';
import * as fs from 'fs';

/**
 * Derive the transfer account address for an owner
 */
function getTransferAccountAddress(
    owner: PublicKey,
    programId: PublicKey,
): PublicKey {
    return PublicKey.findProgramAddressSync([owner.toBytes()], programId)[0];
}

/**
 * Deserialize TransferAccount data
 * Layout: owner (32 bytes) + transfered (8 bytes u64 LE)
 */
function deserializeTransferAccount(data: Buffer): {
    owner: PublicKey;
    transfered: bigint;
} {
    if (data.length < 40) {
        throw new Error(`Invalid account data length: ${data.length}, expected at least 40`);
    }

    // Read owner (32 bytes at offset 0)
    const ownerBytes = data.subarray(0, 32);
    const owner = new PublicKey(ownerBytes);

    // Read transfered (8 bytes at offset 32, little endian u64)
    const transferedBytes = data.subarray(32, 40);
    const transfered = transferedBytes.readBigUInt64LE(0);

    return { owner, transfered };
}

async function main() {
    // Configuration
    const RPC_URL = process.env.RPC_URL || 'http://localhost:8899';
    const PROGRAM_ID = new PublicKey(
        process.env.PROGRAM_ID || 'TokenHookExampLe8smaVNrxTBezWTRbEwxwb1Zykrb'
    );

    // Load owner keypair
    let ownerKeypairPath = process.env.OWNER_KEYPAIR;

    if (!ownerKeypairPath) {
        const configPath = `${process.env.HOME}/.config/solana/cli/config.yml`;
        if (fs.existsSync(configPath)) {
            const config = fs.readFileSync(configPath, 'utf-8');
            const match = config.match(/keypair_path:\s*(.+)/);
            if (match) {
                ownerKeypairPath = match[1].trim();
            }
        }
    }

    if (!ownerKeypairPath) {
        ownerKeypairPath = `${process.env.HOME}/.config/solana/id.json`;
    }

    const ownerKeypair = Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(fs.readFileSync(ownerKeypairPath, 'utf-8')))
    );

    console.log('Owner:', ownerKeypair.publicKey.toBase58());
    console.log('Program ID:', PROGRAM_ID.toBase58());

    // Connect
    const connection = new Connection(RPC_URL, 'confirmed');

    // Derive transfer account address
    const transferAccount = getTransferAccountAddress(
        ownerKeypair.publicKey,
        PROGRAM_ID
    );
    console.log('Transfer account:', transferAccount.toBase58());

    // Fetch account info
    const accountInfo = await connection.getAccountInfo(transferAccount);
    if (!accountInfo) {
        console.log('\n❌ Transfer account does not exist!');
        console.log('Run initialize-transfer-account.ts first.');
        return;
    }

    console.log('\n✅ Transfer account found!');
    console.log('Account owner (program):', accountInfo.owner.toBase58());
    console.log('Data length:', accountInfo.data.length, 'bytes');
    console.log('Lamports:', accountInfo.lamports);

    // Deserialize and print fields
    const { owner, transfered } = deserializeTransferAccount(accountInfo.data);

    console.log('\n📦 TransferAccount fields:');
    console.log('  owner:', owner.toBase58());
    console.log('  transfered:', transfered.toString());
}

main().catch((err) => {
    console.error('Error:', err);
    process.exit(1);
});