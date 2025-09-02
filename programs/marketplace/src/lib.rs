// programs/nft-marketplace/src/lib.rs
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, MintTo};
use anchor_spl::associated_token::AssociatedToken;
use mpl_token_metadata::{
    instructions::{
        CreateMasterEditionV3,
        CreateMasterEditionV3InstructionArgs,
        CreateMetadataAccountV3,
        CreateMetadataAccountV3InstructionArgs,
        VerifyCollection,
    },
    types::{Collection, Creator, DataV2},
};

declare_id!("8KzE3LCicxv13iJx2v2V4VQQNWt4QHuvfuH8jxYnkGQ1");

#[program]
pub mod nft_marketplace {
    use super::*;

    pub fn initialize_marketplace(ctx: Context<InitializeMarketplace>, fee_bps: u16) -> Result<()> {
        let marketplace = &mut ctx.accounts.marketplace;
        marketplace.admin = ctx.accounts.admin.key();
        marketplace.fee_bps = fee_bps;
        marketplace.total_collections = 0;
        marketplace.bump = ctx.bumps.marketplace;
        
        msg!("Marketplace initialized with admin: {}", marketplace.admin);
        Ok(())
    }

    pub fn create_nft_collection(
        ctx: Context<CreateNFTCollection>,
        collection_name: String,
        symbol: String,
        uri: String,
        max_supply: u64,
        price: u64,
        royalty: u16,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        let marketplace = &mut ctx.accounts.marketplace;
        
        collection.admin = ctx.accounts.admin.key();
        collection.name = collection_name.clone();
        collection.symbol = symbol.clone();
        collection.uri = uri.clone();
        collection.max_supply = max_supply;
        collection.current_supply = 0;
        collection.price = price;
        collection.royalty = royalty;
        collection.mint = ctx.accounts.collection_mint.key();
        collection.is_active = true;
        collection.bump = ctx.bumps.collection;

        // Mint 1 token to admin - required for master edition
        let cpi_accounts = MintTo {
            mint: ctx.accounts.collection_mint.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        
        anchor_spl::token::mint_to(cpi_ctx, 1)?;
        msg!("Minted 1 token for master edition");

        // Create collection metadata
        let metadata_data = DataV2 {
            name: collection_name.clone(),
            symbol: symbol.clone(),
            uri: uri.clone(),
            seller_fee_basis_points: royalty,
            creators: Some(vec![Creator {
                address: ctx.accounts.admin.key(),
                verified: true,
                share: 100,
            }]),
            collection: None,
            uses: None,
        };

        let create_metadata_ix = CreateMetadataAccountV3 {
            metadata: ctx.accounts.collection_metadata.key(),
            mint: ctx.accounts.collection_mint.key(),
            mint_authority: ctx.accounts.admin.key(),
            payer: ctx.accounts.admin.key(),
            update_authority: (ctx.accounts.admin.key(), true),
            system_program: ctx.accounts.system_program.key(),
            rent: Some(ctx.accounts.rent.key()),
        }.instruction(CreateMetadataAccountV3InstructionArgs {
            data: metadata_data,
            is_mutable: true,
            collection_details: None,
        });

        let metadata_accounts = vec![
            ctx.accounts.collection_metadata.to_account_info(),
            ctx.accounts.collection_mint.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.rent.to_account_info(),
        ];

        anchor_lang::solana_program::program::invoke(&create_metadata_ix, &metadata_accounts)?;
        msg!("Collection metadata created");

        // Create master edition
        let create_master_edition_ix = CreateMasterEditionV3 {
            edition: ctx.accounts.collection_master_edition.key(),
            mint: ctx.accounts.collection_mint.key(),
            update_authority: ctx.accounts.admin.key(),
            mint_authority: ctx.accounts.admin.key(),
            payer: ctx.accounts.admin.key(),
            metadata: ctx.accounts.collection_metadata.key(),
            token_program: ctx.accounts.token_program.key(),
            system_program: ctx.accounts.system_program.key(),
            rent: Some(ctx.accounts.rent.key()),
        }.instruction(CreateMasterEditionV3InstructionArgs {
            max_supply: Some(max_supply),
        });

        let master_edition_accounts = vec![
            ctx.accounts.collection_master_edition.to_account_info(),
            ctx.accounts.collection_mint.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.collection_metadata.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.rent.to_account_info(),
        ];

        anchor_lang::solana_program::program::invoke(&create_master_edition_ix, &master_edition_accounts)?;
        msg!("Master edition created");

        marketplace.total_collections += 1;
        
        msg!("NFT Collection created: {}", collection.name);
        Ok(())
    }

    pub fn mint_nft_from_collection(
        ctx: Context<MintNFTFromCollection>,
        nft_name: String,
        nft_uri: String,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        
        require!(collection.is_active, ErrorCode::CollectionInactive);
        require!(collection.current_supply < collection.max_supply, ErrorCode::CollectionSoldOut);

        // Transfer payment to collection admin
        let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.buyer.key(),
            &collection.admin,
            collection.price,
        );

        anchor_lang::solana_program::program::invoke(
            &transfer_ix,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.collection_admin.to_account_info(),
            ],
        )?;

        // Mint NFT to buyer
        let cpi_accounts = MintTo {
            mint: ctx.accounts.nft_mint.to_account_info(),
            to: ctx.accounts.buyer_token_account.to_account_info(),
            authority: ctx.accounts.collection_admin.to_account_info(),
        };

        let collection_name = collection.name.as_bytes();
        let seeds = &[
            b"collection",
            collection_name,
            &[collection.bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

        anchor_spl::token::mint_to(cpi_ctx, 1)?;

        // Create NFT metadata
        let metadata_data = DataV2 {
            name: nft_name.clone(),
            symbol: collection.symbol.clone(),
            uri: nft_uri,
            seller_fee_basis_points: collection.royalty,
            creators: Some(vec![Creator {
                address: collection.admin,
                verified: true,
                share: 100,
            }]),
            collection: Some(Collection {
                verified: false,
                key: collection.mint,
            }),
            uses: None,
        };

        let create_nft_metadata_ix = CreateMetadataAccountV3 {
            metadata: ctx.accounts.nft_metadata.key(),
            mint: ctx.accounts.nft_mint.key(),
            mint_authority: ctx.accounts.collection_admin.key(),
            payer: ctx.accounts.buyer.key(),
            update_authority: (collection.admin, true),
            system_program: ctx.accounts.system_program.key(),
            rent: Some(ctx.accounts.rent.key()),
        }.instruction(CreateMetadataAccountV3InstructionArgs {
            data: metadata_data,
            is_mutable: false,
            collection_details: None,
        });

        let nft_metadata_accounts = vec![
            ctx.accounts.nft_metadata.to_account_info(),
            ctx.accounts.nft_mint.to_account_info(),
            ctx.accounts.collection_admin.to_account_info(),
            ctx.accounts.buyer.to_account_info(),
            ctx.accounts.collection_admin.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.rent.to_account_info(),
        ];

        anchor_lang::solana_program::program::invoke(&create_nft_metadata_ix, &nft_metadata_accounts)?;

        // Verify collection (unsized) after metadata creation
        let verify_collection_ix = VerifyCollection {
            metadata: ctx.accounts.nft_metadata.key(),
            collection_authority: ctx.accounts.collection_admin.key(),
            payer: ctx.accounts.buyer.key(),
            collection_mint: ctx.accounts.collection_mint_account.key(),
            collection: ctx.accounts.collection_metadata.key(),
            collection_master_edition: ctx.accounts.collection_master_edition.key(),
        }
        .instruction();

        let verify_accounts = vec![
            ctx.accounts.nft_metadata.to_account_info(),
            ctx.accounts.collection_admin.to_account_info(),
            ctx.accounts.buyer.to_account_info(),
            ctx.accounts.collection_mint_account.to_account_info(),
            ctx.accounts.collection_metadata.to_account_info(),
            ctx.accounts.collection_master_edition.to_account_info(),
        ];

        anchor_lang::solana_program::program::invoke(&verify_collection_ix, &verify_accounts)?;

        collection.current_supply += 1;
        
        msg!("NFT minted: {} (#{}/{})", nft_name, collection.current_supply, collection.max_supply);
        Ok(())
    }
}

// Account Structures
#[derive(Accounts)]
pub struct InitializeMarketplace<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 2 + 8 + 1,
        seeds = [b"marketplace"],
        bump
    )]
    pub marketplace: Account<'info, Marketplace>,
    
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(collection_name: String)]
pub struct CreateNFTCollection<'info> {
    #[account(
        mut,
        seeds = [b"marketplace"],
        bump = marketplace.bump
    )]
    pub marketplace: Account<'info, Marketplace>,

    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 4 + collection_name.len() + 4 + 10 + 4 + 200 + 8 + 8 + 8 + 2 + 32 + 1 + 1,
        seeds = [b"collection", collection_name.as_bytes()],
        bump
    )]
    pub collection: Account<'info, NFTCollection>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 0,
        mint::authority = admin,
        mint::freeze_authority = admin,
    )]
    pub collection_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = collection_mint,
        associated_token::authority = admin,
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    /// CHECK: Metadata account
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: UncheckedAccount<'info>,

    /// CHECK: Master edition account
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
            b"edition",
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_master_edition: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Token Metadata Program
    #[account(constraint = token_metadata_program.key() == mpl_token_metadata::ID)]
    pub token_metadata_program: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintNFTFromCollection<'info> {
    #[account(
        mut,
        seeds = [b"collection", collection.name.as_bytes()],
        bump = collection.bump,
    )]
    pub collection: Account<'info, NFTCollection>,

    #[account(
        init,
        payer = buyer,
        mint::decimals = 0,
        mint::authority = collection.admin,
    )]
    pub nft_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = nft_mint,
        associated_token::authority = buyer,
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,

    /// CHECK: NFT Metadata account
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            nft_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub nft_metadata: UncheckedAccount<'info>,

    /// CHECK: Collection metadata PDA (for the collection mint)
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint_account.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: UncheckedAccount<'info>,

    /// CHECK: Collection master edition PDA
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint_account.key().as_ref(),
            b"edition",
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_master_edition: UncheckedAccount<'info>,

    /// CHECK: Collection mint account (must match stored collection.mint)
    #[account(constraint = collection_mint_account.key() == collection.mint)]
    pub collection_mint_account: UncheckedAccount<'info>,

    /// CHECK: Collection admin (receives payment) and authority to verify collection
    #[account(mut, constraint = collection_admin.key() == collection.admin)]
    pub collection_admin: Signer<'info>,

    #[account(mut)]
    pub buyer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Token Metadata Program
    pub token_metadata_program: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
}

// State Structs
#[account]
pub struct Marketplace {
    pub admin: Pubkey,
    pub fee_bps: u16,
    pub total_collections: u64,
    pub bump: u8,
}

#[account]
pub struct NFTCollection {
    pub admin: Pubkey,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub max_supply: u64,
    pub current_supply: u64,
    pub price: u64,
    pub royalty: u16,
    pub mint: Pubkey,
    pub is_active: bool,
    pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Collection is not active")]
    CollectionInactive,
    #[msg("Collection is sold out")]
    CollectionSoldOut,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Unauthorized access")]
    Unauthorized,
}