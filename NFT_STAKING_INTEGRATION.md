# NFT Staking System - Integration Guide

## Overview

This document provides comprehensive integration guidelines for the NFT Staking system built on Solana using Anchor framework. The system implements a vault-based staking mechanism where users can stake their NFTs and earn rewards based on stake multipliers.

## Program Information

- **Program ID**: `8KzE3LCicxv13iJx2v2V4VQQNWt4QHuvfuH8jxYnkGQ1`
- **Network**: Devnet (for testing)
- **Reward Token**: `GshYgeeG5xmeMJ4crtg1SHGafYXBpnCyPz9VNF8DXxSW`
- **Framework**: Anchor 0.31.1

## Key Features

1. **NFT Type-Based Multipliers**: Each NFT type has a unique stake_multiplier
2. **Vault-Based Security**: NFTs are transferred to secure vault PDAs
3. **Flexible Reward Claims**: Claim rewards without unstaking
4. **Time-Based Rewards**: Rewards calculated based on staking duration
5. **Collection Verification**: Ensures only collection NFTs can be staked

## Account Structures

### StakePool
Global staking configuration account.

```rust
pub struct StakePool {
    pub admin: Pubkey,                    // Pool administrator
    pub reward_token_mint: Pubkey,        // Reward token mint address
    pub reward_rate_per_second: u64,      // Base reward rate (before multiplier)
    pub total_staked: u64,                // Total NFTs currently staked
    pub bump: u8,                         // PDA bump seed
}
```

**PDA Derivation**: `["stake_pool"]`

### StakeAccount
Individual stake record for each NFT.

```rust
pub struct StakeAccount {
    pub owner: Pubkey,                    // Staker's wallet address
    pub nft_mint: Pubkey,                 // Staked NFT mint
    pub nft_type: Pubkey,                 // NFT type (for multiplier lookup)
    pub stake_pool: Pubkey,               // Associated stake pool
    pub stake_timestamp: i64,             // When NFT was staked
    pub last_claim_timestamp: i64,        // Last reward claim time
    pub stake_multiplier: u64,            // Multiplier (in basis points)
    pub bump: u8,                         // PDA bump seed
}
```

**PDA Derivation**: `["stake_account", staker_pubkey, nft_mint_pubkey]`

### NftType (Updated)
NFT type definition with staking multiplier.

```rust
pub struct NftType {
    pub collection: Pubkey,
    pub name: String,
    pub uri: String,
    pub price: u64,
    pub max_supply: u64,
    pub current_supply: u64,
    pub stake_multiplier: u64,            // NEW: Multiplier in basis points
    pub bump: u8,
}
```

**Multiplier Format**: Basis points (10000 = 1x, 20000 = 2x, 15000 = 1.5x)

## Instructions

### 1. Initialize Stake Pool

Creates the global staking pool and reward vault.

**Parameters**:
- `reward_rate_per_second`: Base reward amount per second (before multiplier)

**Accounts**:
- `stake_pool`: PDA to be created
- `reward_token_mint`: The reward token mint (GshYgeeG5xmeMJ4crtg1SHGafYXBpnCyPz9VNF8DXxSW)
- `reward_token_vault`: Vault for holding reward tokens
- `admin`: Signer (pool administrator)

**Example**:
```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { NftMarketplace } from "../target/types/nft_marketplace";
import { PublicKey } from "@solana/web3.js";

const program = anchor.workspace.NftMarketplace as Program<NftMarketplace>;
const REWARD_TOKEN_MINT = new PublicKey("GshYgeeG5xmeMJ4crtg1SHGafYXBpnCyPz9VNF8DXxSW");

// Derive PDAs
const [stakePoolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("stake_pool")],
  program.programId
);

const [rewardVaultPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("reward_vault")],
  program.programId
);

// Initialize stake pool
await program.methods
  .initializeStakePool(new anchor.BN(100)) // 100 tokens per second base rate
  .accounts({
    stakePool: stakePoolPda,
    rewardTokenMint: REWARD_TOKEN_MINT,
    rewardTokenVault: rewardVaultPda,
    admin: admin.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
    tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    rent: anchor.web3.SYSVAR_RENT_PUBKEY,
  })
  .signers([admin])
  .rpc();
```

**Post-Initialization**: Admin must fund the reward vault with reward tokens.

```typescript
import { getAssociatedTokenAddress, transfer } from "@solana/spl-token";

// Transfer reward tokens to vault
const adminTokenAccount = await getAssociatedTokenAddress(
  REWARD_TOKEN_MINT,
  admin.publicKey
);

await transfer(
  connection,
  admin,
  adminTokenAccount,
  rewardVaultPda,
  admin.publicKey,
  1_000_000_000 // Amount with decimals
);
```

### 2. Stake NFT

Stakes an NFT into the vault and creates a stake account.

**Parameters**: None

**Accounts**:
- `stake_pool`: Stake pool PDA
- `stake_account`: PDA to be created for this stake
- `collection`: NFT collection account
- `nft_type`: NFT type account
- `nft_mint`: The NFT mint to stake
- `nft_metadata`: NFT metadata account
- `staker_nft_token_account`: User's NFT token account
- `vault_nft_token_account`: Vault token account to be created
- `staker`: Signer

**Example**:
```typescript
const nftMint = new PublicKey("YOUR_NFT_MINT");
const collectionName = "YourCollectionName";
const typeName = "YourTypeName";

// Derive PDAs
const [stakePoolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("stake_pool")],
  program.programId
);

const [stakeAccountPda] = PublicKey.findProgramAddressSync(
  [
    Buffer.from("stake_account"),
    staker.publicKey.toBuffer(),
    nftMint.toBuffer(),
  ],
  program.programId
);

const [collectionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("collection"), Buffer.from(collectionName)],
  program.programId
);

const [nftTypePda] = PublicKey.findProgramAddressSync(
  [
    Buffer.from("type"),
    collectionPda.toBuffer(),
    Buffer.from(typeName),
  ],
  program.programId
);

const [vaultNftTokenAccount] = PublicKey.findProgramAddressSync(
  [Buffer.from("vault"), nftMint.toBuffer()],
  program.programId
);

// Get metadata PDA
const TOKEN_METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

const [nftMetadata] = PublicKey.findProgramAddressSync(
  [
    Buffer.from("metadata"),
    TOKEN_METADATA_PROGRAM_ID.toBuffer(),
    nftMint.toBuffer(),
  ],
  TOKEN_METADATA_PROGRAM_ID
);

const stakerNftTokenAccount = await getAssociatedTokenAddress(
  nftMint,
  staker.publicKey
);

// Stake NFT
await program.methods
  .stakeNft()
  .accounts({
    stakePool: stakePoolPda,
    stakeAccount: stakeAccountPda,
    collection: collectionPda,
    nftType: nftTypePda,
    nftMint: nftMint,
    nftMetadata: nftMetadata,
    stakerNftTokenAccount: stakerNftTokenAccount,
    vaultNftTokenAccount: vaultNftTokenAccount,
    staker: staker.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
    tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
    rent: anchor.web3.SYSVAR_RENT_PUBKEY,
  })
  .signers([staker])
  .rpc();
```

### 3. Claim Rewards

Claims accumulated rewards without unstaking the NFT.

**Parameters**: None

**Accounts**:
- `stake_pool`: Stake pool PDA
- `stake_account`: User's stake account PDA
- `reward_token_mint`: Reward token mint
- `reward_token_vault`: Reward vault PDA
- `staker_reward_token_account`: User's reward token account
- `staker`: Signer

**Reward Calculation**:
```
time_since_last_claim = current_time - last_claim_timestamp
base_rewards = time_since_last_claim * reward_rate_per_second
final_rewards = (base_rewards * stake_multiplier) / 10000
```

**Example**:
```typescript
const [stakePoolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("stake_pool")],
  program.programId
);

const [stakeAccountPda] = PublicKey.findProgramAddressSync(
  [
    Buffer.from("stake_account"),
    staker.publicKey.toBuffer(),
    nftMint.toBuffer(),
  ],
  program.programId
);

const [rewardVaultPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("reward_vault")],
  program.programId
);

const stakerRewardTokenAccount = await getAssociatedTokenAddress(
  REWARD_TOKEN_MINT,
  staker.publicKey
);

await program.methods
  .claimRewards()
  .accounts({
    stakePool: stakePoolPda,
    stakeAccount: stakeAccountPda,
    rewardTokenMint: REWARD_TOKEN_MINT,
    rewardTokenVault: rewardVaultPda,
    stakerRewardTokenAccount: stakerRewardTokenAccount,
    staker: staker.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
    tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
    rent: anchor.web3.SYSVAR_RENT_PUBKEY,
  })
  .signers([staker])
  .rpc();
```

### 4. Unstake NFT

Unstakes the NFT, claims all pending rewards, and closes the stake account.

**Parameters**: None

**Accounts**: Same as claim_rewards, plus:
- `vault_nft_token_account`: Vault holding the NFT
- `staker_nft_token_account`: User's NFT token account

**Example**:
```typescript
const [stakePoolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("stake_pool")],
  program.programId
);

const [stakeAccountPda] = PublicKey.findProgramAddressSync(
  [
    Buffer.from("stake_account"),
    staker.publicKey.toBuffer(),
    nftMint.toBuffer(),
  ],
  program.programId
);

const [rewardVaultPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("reward_vault")],
  program.programId
);

const [vaultNftTokenAccount] = PublicKey.findProgramAddressSync(
  [Buffer.from("vault"), nftMint.toBuffer()],
  program.programId
);

const stakerRewardTokenAccount = await getAssociatedTokenAddress(
  REWARD_TOKEN_MINT,
  staker.publicKey
);

const stakerNftTokenAccount = await getAssociatedTokenAddress(
  nftMint,
  staker.publicKey
);

await program.methods
  .unstakeNft()
  .accounts({
    stakePool: stakePoolPda,
    stakeAccount: stakeAccountPda,
    rewardTokenMint: REWARD_TOKEN_MINT,
    rewardTokenVault: rewardVaultPda,
    stakerRewardTokenAccount: stakerRewardTokenAccount,
    vaultNftTokenAccount: vaultNftTokenAccount,
    stakerNftTokenAccount: stakerNftTokenAccount,
    staker: staker.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
    tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
    associatedTokenProgram: anchor.utils.token.ASSOCIATED_PROGRAM_ID,
    rent: anchor.web3.SYSVAR_RENT_PUBKEY,
  })
  .signers([staker])
  .rpc();
```

## Frontend Integration

### React Hooks for Staking

```typescript
// hooks/useStaking.ts
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { Program, AnchorProvider, Idl } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { useCallback, useEffect, useState } from "react";
import idl from "../idl/nft_marketplace.json";

const PROGRAM_ID = new PublicKey("8KzE3LCicxv13iJx2v2V4VQQNWt4QHuvfuH8jxYnkGQ1");
const REWARD_TOKEN_MINT = new PublicKey("GshYgeeG5xmeMJ4crtg1SHGafYXBpnCyPz9VNF8DXxSW");

export interface StakeInfo {
  nftMint: PublicKey;
  stakeTimestamp: number;
  lastClaimTimestamp: number;
  multiplier: number;
  pendingRewards: number;
}

export const useStaking = () => {
  const { connection } = useConnection();
  const wallet = useWallet();
  const [program, setProgram] = useState<Program | null>(null);
  const [stakeInfo, setStakeInfo] = useState<StakeInfo[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (wallet.publicKey) {
      const provider = new AnchorProvider(
        connection,
        wallet as any,
        AnchorProvider.defaultOptions()
      );
      const program = new Program(idl as Idl, PROGRAM_ID, provider);
      setProgram(program);
    }
  }, [connection, wallet]);

  const fetchStakedNFTs = useCallback(async () => {
    if (!program || !wallet.publicKey) return;

    setLoading(true);
    try {
      // Fetch all stake accounts owned by user
      const stakeAccounts = await program.account.stakeAccount.all([
        {
          memcmp: {
            offset: 8, // After discriminator
            bytes: wallet.publicKey.toBase58(),
          },
        },
      ]);

      const stakePool = await program.account.stakePool.fetch(
        PublicKey.findProgramAddressSync(
          [Buffer.from("stake_pool")],
          program.programId
        )[0]
      );

      const currentTime = Math.floor(Date.now() / 1000);

      const stakes = stakeAccounts.map((account) => {
        const data = account.account;
        const timeSinceLastClaim = currentTime - data.lastClaimTimestamp.toNumber();
        const baseRewards = timeSinceLastClaim * stakePool.rewardRatePerSecond.toNumber();
        const pendingRewards = (baseRewards * data.stakeMultiplier.toNumber()) / 10000;

        return {
          nftMint: data.nftMint,
          stakeTimestamp: data.stakeTimestamp.toNumber(),
          lastClaimTimestamp: data.lastClaimTimestamp.toNumber(),
          multiplier: data.stakeMultiplier.toNumber() / 10000,
          pendingRewards,
        };
      });

      setStakeInfo(stakes);
    } catch (error) {
      console.error("Error fetching staked NFTs:", error);
    } finally {
      setLoading(false);
    }
  }, [program, wallet.publicKey]);

  const stakeNFT = useCallback(
    async (nftMint: PublicKey, collectionName: string, typeName: string) => {
      if (!program || !wallet.publicKey) return;

      try {
        // Derive all necessary PDAs
        const [stakePoolPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("stake_pool")],
          program.programId
        );

        const [stakeAccountPda] = PublicKey.findProgramAddressSync(
          [
            Buffer.from("stake_account"),
            wallet.publicKey.toBuffer(),
            nftMint.toBuffer(),
          ],
          program.programId
        );

        const [collectionPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("collection"), Buffer.from(collectionName)],
          program.programId
        );

        const [nftTypePda] = PublicKey.findProgramAddressSync(
          [
            Buffer.from("type"),
            collectionPda.toBuffer(),
            Buffer.from(typeName),
          ],
          program.programId
        );

        const [vaultNftTokenAccount] = PublicKey.findProgramAddressSync(
          [Buffer.from("vault"), nftMint.toBuffer()],
          program.programId
        );

        const TOKEN_METADATA_PROGRAM_ID = new PublicKey(
          "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
        );

        const [nftMetadata] = PublicKey.findProgramAddressSync(
          [
            Buffer.from("metadata"),
            TOKEN_METADATA_PROGRAM_ID.toBuffer(),
            nftMint.toBuffer(),
          ],
          TOKEN_METADATA_PROGRAM_ID
        );

        const stakerNftTokenAccount = await getAssociatedTokenAddress(
          nftMint,
          wallet.publicKey
        );

        await program.methods
          .stakeNft()
          .accounts({
            stakePool: stakePoolPda,
            stakeAccount: stakeAccountPda,
            collection: collectionPda,
            nftType: nftTypePda,
            nftMint: nftMint,
            nftMetadata: nftMetadata,
            stakerNftTokenAccount: stakerNftTokenAccount,
            vaultNftTokenAccount: vaultNftTokenAccount,
            staker: wallet.publicKey,
          })
          .rpc();

        await fetchStakedNFTs();
        return true;
      } catch (error) {
        console.error("Error staking NFT:", error);
        return false;
      }
    },
    [program, wallet.publicKey, fetchStakedNFTs]
  );

  const claimRewards = useCallback(
    async (nftMint: PublicKey) => {
      if (!program || !wallet.publicKey) return;

      try {
        const [stakePoolPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("stake_pool")],
          program.programId
        );

        const [stakeAccountPda] = PublicKey.findProgramAddressSync(
          [
            Buffer.from("stake_account"),
            wallet.publicKey.toBuffer(),
            nftMint.toBuffer(),
          ],
          program.programId
        );

        const [rewardVaultPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("reward_vault")],
          program.programId
        );

        const stakerRewardTokenAccount = await getAssociatedTokenAddress(
          REWARD_TOKEN_MINT,
          wallet.publicKey
        );

        await program.methods
          .claimRewards()
          .accounts({
            stakePool: stakePoolPda,
            stakeAccount: stakeAccountPda,
            rewardTokenMint: REWARD_TOKEN_MINT,
            rewardTokenVault: rewardVaultPda,
            stakerRewardTokenAccount: stakerRewardTokenAccount,
            staker: wallet.publicKey,
          })
          .rpc();

        await fetchStakedNFTs();
        return true;
      } catch (error) {
        console.error("Error claiming rewards:", error);
        return false;
      }
    },
    [program, wallet.publicKey, fetchStakedNFTs]
  );

  const unstakeNFT = useCallback(
    async (nftMint: PublicKey) => {
      if (!program || !wallet.publicKey) return;

      try {
        const [stakePoolPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("stake_pool")],
          program.programId
        );

        const [stakeAccountPda] = PublicKey.findProgramAddressSync(
          [
            Buffer.from("stake_account"),
            wallet.publicKey.toBuffer(),
            nftMint.toBuffer(),
          ],
          program.programId
        );

        const [rewardVaultPda] = PublicKey.findProgramAddressSync(
          [Buffer.from("reward_vault")],
          program.programId
        );

        const [vaultNftTokenAccount] = PublicKey.findProgramAddressSync(
          [Buffer.from("vault"), nftMint.toBuffer()],
          program.programId
        );

        const stakerRewardTokenAccount = await getAssociatedTokenAddress(
          REWARD_TOKEN_MINT,
          wallet.publicKey
        );

        const stakerNftTokenAccount = await getAssociatedTokenAddress(
          nftMint,
          wallet.publicKey
        );

        await program.methods
          .unstakeNft()
          .accounts({
            stakePool: stakePoolPda,
            stakeAccount: stakeAccountPda,
            rewardTokenMint: REWARD_TOKEN_MINT,
            rewardTokenVault: rewardVaultPda,
            stakerRewardTokenAccount: stakerRewardTokenAccount,
            vaultNftTokenAccount: vaultNftTokenAccount,
            stakerNftTokenAccount: stakerNftTokenAccount,
            staker: wallet.publicKey,
          })
          .rpc();

        await fetchStakedNFTs();
        return true;
      } catch (error) {
        console.error("Error unstaking NFT:", error);
        return false;
      }
    },
    [program, wallet.publicKey, fetchStakedNFTs]
  );

  return {
    stakeInfo,
    loading,
    stakeNFT,
    claimRewards,
    unstakeNFT,
    fetchStakedNFTs,
  };
};
```

### React Component Example

```typescript
// components/StakingDashboard.tsx
import React, { useEffect } from "react";
import { useStaking } from "../hooks/useStaking";
import { PublicKey } from "@solana/web3.js";

export const StakingDashboard: React.FC = () => {
  const { stakeInfo, loading, claimRewards, unstakeNFT, fetchStakedNFTs } = useStaking();

  useEffect(() => {
    fetchStakedNFTs();
  }, [fetchStakedNFTs]);

  if (loading) {
    return <div>Loading staked NFTs...</div>;
  }

  return (
    <div className="staking-dashboard">
      <h2>Your Staked NFTs</h2>
      {stakeInfo.length === 0 ? (
        <p>No NFTs staked yet</p>
      ) : (
        <div className="staked-nfts">
          {stakeInfo.map((stake) => (
            <div key={stake.nftMint.toString()} className="stake-card">
              <h3>NFT: {stake.nftMint.toString().slice(0, 8)}...</h3>
              <p>Multiplier: {stake.multiplier}x</p>
              <p>Staked: {new Date(stake.stakeTimestamp * 1000).toLocaleDateString()}</p>
              <p>Pending Rewards: {stake.pendingRewards.toFixed(2)}</p>

              <div className="actions">
                <button onClick={() => claimRewards(stake.nftMint)}>
                  Claim Rewards
                </button>
                <button onClick={() => unstakeNFT(stake.nftMint)}>
                  Unstake NFT
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
```

## Error Handling

```typescript
export const handleStakingError = (error: any): string => {
  if (error.message.includes("InvalidStakeMultiplier")) {
    return "Invalid stake multiplier. Must be greater than 0.";
  }
  if (error.message.includes("NFTNotStaked")) {
    return "This NFT is not currently staked.";
  }
  if (error.message.includes("NFTAlreadyStaked")) {
    return "This NFT is already staked.";
  }
  if (error.message.includes("InvalidNFTMint")) {
    return "This NFT does not belong to the collection.";
  }
  if (error.message.includes("Unauthorized")) {
    return "You are not authorized to perform this action.";
  }
  return "An unknown error occurred. Please try again.";
};
```

## Testing Checklist

### Pre-Deployment (Devnet)
- [ ] Initialize stake pool with correct reward token
- [ ] Fund reward vault with sufficient tokens
- [ ] Create NFT collection with types
- [ ] Set appropriate stake_multipliers for each type
- [ ] Test stake NFT functionality
- [ ] Test claim rewards functionality
- [ ] Test unstake NFT functionality
- [ ] Verify reward calculations
- [ ] Test with different multipliers
- [ ] Test error cases (unauthorized, wrong collection, etc.)

### Post-Deployment
- [ ] Monitor reward vault balance
- [ ] Track total staked NFTs
- [ ] Monitor user transactions
- [ ] Set up admin alerts for low vault balance

## Common Issues and Solutions

### Issue: "Account not found"
**Solution**: Ensure all PDAs are correctly derived and accounts are initialized.

### Issue: "Insufficient funds"
**Solution**: Ensure reward vault has enough tokens to distribute.

### Issue: "Invalid collection"
**Solution**: Verify NFT belongs to the correct collection and metadata is valid.

### Issue: Rewards calculation seems wrong
**Solution**: Check that:
1. stake_multiplier is in basis points (10000 = 1x)
2. reward_rate_per_second is correctly set
3. Timestamps are accurate

## Security Considerations

1. **Vault Security**: NFTs are held in PDAs controlled by the program
2. **Ownership Checks**: All operations verify the caller owns the stake
3. **Collection Verification**: Only verified collection NFTs can be staked
4. **Reward Distribution**: Uses CPI with PDA signers to ensure secure transfers
5. **Account Validation**: Comprehensive account validation in all instructions

## Admin Operations

### Funding the Reward Vault

```typescript
import { transfer, getAssociatedTokenAddress } from "@solana/spl-token";

async function fundRewardVault(
  connection: Connection,
  admin: Keypair,
  amount: number
) {
  const [rewardVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("reward_vault")],
    PROGRAM_ID
  );

  const adminTokenAccount = await getAssociatedTokenAddress(
    REWARD_TOKEN_MINT,
    admin.publicKey
  );

  await transfer(
    connection,
    admin,
    adminTokenAccount,
    rewardVaultPda,
    admin.publicKey,
    amount
  );
}
```

### Monitoring Vault Balance

```typescript
async function checkVaultBalance(connection: Connection): Promise<number> {
  const [rewardVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("reward_vault")],
    PROGRAM_ID
  );

  const vaultAccount = await connection.getTokenAccountBalance(rewardVaultPda);
  return Number(vaultAccount.value.amount);
}
```

## API Endpoints (Backend Example)

### Express.js Backend Example

```typescript
// routes/staking.ts
import express from "express";
import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider } from "@coral-xyz/anchor";

const router = express.Router();
const connection = new Connection("https://api.devnet.solana.com");

// Get user's staked NFTs
router.get("/staked/:walletAddress", async (req, res) => {
  try {
    const walletAddress = new PublicKey(req.params.walletAddress);
    const provider = new AnchorProvider(connection, {} as any, {});
    const program = new Program(idl, PROGRAM_ID, provider);

    const stakeAccounts = await program.account.stakeAccount.all([
      {
        memcmp: {
          offset: 8,
          bytes: walletAddress.toBase58(),
        },
      },
    ]);

    res.json({
      success: true,
      stakes: stakeAccounts.map((acc) => ({
        nftMint: acc.account.nftMint.toString(),
        stakeTimestamp: acc.account.stakeTimestamp.toNumber(),
        multiplier: acc.account.stakeMultiplier.toNumber(),
      })),
    });
  } catch (error) {
    res.status(500).json({ success: false, error: error.message });
  }
});

// Get pending rewards for an NFT
router.get("/rewards/:walletAddress/:nftMint", async (req, res) => {
  try {
    const walletAddress = new PublicKey(req.params.walletAddress);
    const nftMint = new PublicKey(req.params.nftMint);
    const provider = new AnchorProvider(connection, {} as any, {});
    const program = new Program(idl, PROGRAM_ID, provider);

    const [stakeAccountPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("stake_account"),
        walletAddress.toBuffer(),
        nftMint.toBuffer(),
      ],
      program.programId
    );

    const stakeAccount = await program.account.stakeAccount.fetch(stakeAccountPda);
    const stakePool = await program.account.stakePool.fetch(
      PublicKey.findProgramAddressSync(
        [Buffer.from("stake_pool")],
        program.programId
      )[0]
    );

    const currentTime = Math.floor(Date.now() / 1000);
    const timeSinceLastClaim = currentTime - stakeAccount.lastClaimTimestamp.toNumber();
    const baseRewards = timeSinceLastClaim * stakePool.rewardRatePerSecond.toNumber();
    const pendingRewards = (baseRewards * stakeAccount.stakeMultiplier.toNumber()) / 10000;

    res.json({
      success: true,
      pendingRewards,
      multiplier: stakeAccount.stakeMultiplier.toNumber() / 10000,
    });
  } catch (error) {
    res.status(500).json({ success: false, error: error.message });
  }
});

export default router;
```

## Conclusion

This integration guide provides all the necessary information to integrate the NFT staking system into your backend and frontend applications. For additional support or questions, refer to the program source code at `/programs/marketplace/src/lib.rs`.

### Key Takeaways

1. Always derive PDAs correctly using the specified seeds
2. Ensure reward vault is adequately funded
3. Multipliers are in basis points (divide by 10000 for actual multiplier)
4. Test thoroughly on devnet before mainnet deployment
5. Monitor vault balance regularly to ensure continuous reward distribution

### Next Steps

1. Deploy to devnet and test all functionality
2. Create UI components for staking interface
3. Set up backend monitoring and alerts
4. Conduct security audit before mainnet deployment
5. Prepare user documentation and tutorials
