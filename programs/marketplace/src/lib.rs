// programs/nft-marketplace/src/lib.rs
use anchor_lang::prelude::*;
use anchor_lang::solana_program::borsh::try_from_slice_unchecked;
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
    accounts::Metadata as TokenMetadata,
};

declare_id!("ptcbSp1UEqYLmod2jgFxGPZnFMqBECcrRyU1fTmnJ5b");

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

    pub fn create_nft_type(
        ctx: Context<CreateNFTType>,
        type_name: String,
        uri: String,
        price: u64,
        max_supply: u64,
        stake_multiplier: u64,
    ) -> Result<()> {
        let collection = &ctx.accounts.collection;
        let nft_type = &mut ctx.accounts.nft_type;

        require!(collection.is_active, ErrorCode::CollectionInactive);
        require!(stake_multiplier > 0, ErrorCode::InvalidStakeMultiplier);

        nft_type.collection = collection.key();
        nft_type.name = type_name;
        nft_type.uri = uri;
        nft_type.price = price;
        nft_type.max_supply = max_supply;
        nft_type.current_supply = 0;
        nft_type.stake_multiplier = stake_multiplier;
        nft_type.bump = ctx.bumps.nft_type;

        msg!("NFT type created under collection: {}", collection.name);
        Ok(())
    }

    pub fn create_nft_collection(
        ctx: Context<CreateNFTCollection>,
        collection_name: String,
        symbol: String,
        uri: String,
        royalty: u16,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        let marketplace = &mut ctx.accounts.marketplace;
        
        collection.admin = ctx.accounts.admin.key();
        collection.name = collection_name.clone();
        collection.symbol = symbol.clone();
        collection.uri = uri.clone();
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
            max_supply: Some(0), // Unique collection (0 = unique)
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
        type_name: String,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        let nft_type = &mut ctx.accounts.nft_type;
        
        require!(collection.is_active, ErrorCode::CollectionInactive);
        require!(nft_type.current_supply < nft_type.max_supply, ErrorCode::CollectionSoldOut);

        // Transfer payment to collection admin
        let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.buyer.key(),
            &collection.admin,
            nft_type.price,
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

        // Create NFT metadata (fixed per type)
        let nft_name = format!("{} #{}", type_name, nft_type.current_supply + 1);
        let metadata_data = DataV2 {
            name: nft_name,
            symbol: collection.symbol.clone(),
            uri: nft_type.uri.clone(),
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
            collection_master_edition_account: ctx.accounts.collection_master_edition.key(),
            collection_authority_record: None,
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

        nft_type.current_supply += 1;
        
        msg!(
            "NFT minted: {} - {} (type #{}/{})",
            collection.name,
            type_name,
            nft_type.current_supply,
            nft_type.max_supply
        );
        Ok(())
    }

	// Matchmaking: Create a room with an initial stake
	pub fn create_room(
		ctx: Context<CreateRoom>,
		room_id: u64,
		stake_lamports: u64,
	) -> Result<()> {
		require!(stake_lamports > 0, ErrorCode::InsufficientFunds);

		// Require creator to own at least 1 token of the provided NFT mint
		require!(ctx.accounts.creator_nft_token.amount >= 1, ErrorCode::Unauthorized);

		// Verify that the provided NFT belongs to the expected collection
		let metadata_account_info = ctx.accounts.nft_metadata.to_account_info();
		let metadata: TokenMetadata = try_from_slice_unchecked(&metadata_account_info.data.borrow())?;
		let collection = metadata.collection.ok_or(ErrorCode::Unauthorized)?;
		require!(collection.key == ctx.accounts.collection_mint.key(), ErrorCode::Unauthorized);

		let room = &mut ctx.accounts.room;
		room.creator = ctx.accounts.creator.key();
		room.challenger = None;
		room.room_id = room_id;
		room.stake_lamports = stake_lamports;
		room.status = RoomStatus::Waiting as u8;
		room.bump = ctx.bumps.room;

		// Transfer stake from creator to the room (escrow)
		let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
			&ctx.accounts.creator.key(),
			&room.key(),
			stake_lamports,
		);
		anchor_lang::solana_program::program::invoke(
			&transfer_ix,
			&[
				ctx.accounts.creator.to_account_info(),
				room.to_account_info(),
			],
		)?;

		Ok(())
	}

	// Matchmaking: Join a room by matching the stake
	pub fn join_room(ctx: Context<JoinRoom>) -> Result<()> {
		let room = &mut ctx.accounts.room;
		require!(room.status == RoomStatus::Waiting as u8, ErrorCode::RoomNotWaiting);
		require!(room.challenger.is_none(), ErrorCode::RoomHasChallenger);
		require!(ctx.accounts.challenger.key() != room.creator, ErrorCode::Unauthorized);

		// Require challenger to own at least 1 token of the provided NFT mint
		require!(ctx.accounts.challenger_nft_token.amount >= 1, ErrorCode::Unauthorized);

		// Verify that the provided NFT belongs to the expected collection
		let metadata_account_info = ctx.accounts.nft_metadata.to_account_info();
		let metadata: TokenMetadata = try_from_slice_unchecked(&metadata_account_info.data.borrow())?;
		let collection = metadata.collection.ok_or(ErrorCode::Unauthorized)?;
		require!(collection.key == ctx.accounts.collection_mint.key(), ErrorCode::Unauthorized);

		// Transfer matching stake from challenger to the room escrow
		let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
			&ctx.accounts.challenger.key(),
			&room.key(),
			room.stake_lamports,
		);
		anchor_lang::solana_program::program::invoke(
			&transfer_ix,
			&[
				ctx.accounts.challenger.to_account_info(),
				room.to_account_info(),
			],
		)?;

		room.challenger = Some(ctx.accounts.challenger.key());
		room.status = RoomStatus::Ongoing as u8;
		Ok(())
	}

	// Matchmaking: Resolve room, pay winner (creator for now) and close
	pub fn resolve_room(ctx: Context<ResolveRoom>) -> Result<()> {
		let room = &ctx.accounts.room;
		require!(room.status == RoomStatus::Ongoing as u8, ErrorCode::RoomNotOngoing);
		require!(ctx.accounts.creator.key() == room.creator, ErrorCode::Unauthorized);

		// Payout all lamports held by room to the creator.
		let room_lamports = **ctx.accounts.room.to_account_info().lamports.borrow();
		let rent_exempt = Rent::get()?.minimum_balance(Room::space(None));
		let transferable = room_lamports.saturating_sub(rent_exempt);
		if transferable > 0 {
			let seeds = &[
				b"room",
				room.creator.as_ref(),
				&room.room_id.to_le_bytes(),
				&[room.bump],
			];
			let signer = &[&seeds[..]];
			let ix = anchor_lang::solana_program::system_instruction::transfer(
				&ctx.accounts.room.key(),
				&ctx.accounts.creator.key(),
				transferable,
			);
			anchor_lang::solana_program::program::invoke_signed(
				&ix,
				&[
					ctx.accounts.room.to_account_info(),
					ctx.accounts.creator.to_account_info(),
					ctx.accounts.system_program.to_account_info(),
				],
				signer,
			)?;
		}

		// Status will be set to Closed and Anchor will close the account via close attribute
		Ok(())
	}

    // Presale: initialize with 1-day timer and 845 SOL target
    pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
        let presale = &mut ctx.accounts.presale;
        presale.admin = ctx.accounts.admin.key();
        presale.bump = ctx.bumps.presale;
        presale.is_active = true;
        let clock = Clock::get()?;
        presale.start_ts = clock.unix_timestamp;
        presale.end_ts = clock.unix_timestamp + 86_400; // 1 day
        presale.total_raised = 0;
        presale.target_lamports = 845u64.saturating_mul(1_000_000_000);
        Ok(())
    }

    // Presale: restart/reset with 1-day timer; keeps same admin and target
    pub fn restart_presale(ctx: Context<RestartPresale>) -> Result<()> {
        let presale = &mut ctx.accounts.presale;
        require!(ctx.accounts.admin.key() == presale.admin, ErrorCode::Unauthorized);

        let clock = Clock::get()?;
        presale.is_active = true;
        presale.start_ts = clock.unix_timestamp;
        presale.end_ts = clock.unix_timestamp + 86_400; // 1 day
        presale.total_raised = 0;
        Ok(())
    }

    // Presale: contribute SOL and record contributor
    pub fn contribute_presale(ctx: Context<ContributePresale>, lamports: u64) -> Result<()> {
        require!(lamports > 0, ErrorCode::InsufficientFunds);

        let presale = &mut ctx.accounts.presale;
        require!(presale.is_active, ErrorCode::PresaleNotActive);
        let clock = Clock::get()?;
        require!(clock.unix_timestamp <= presale.end_ts, ErrorCode::PresaleEnded);

        // Transfer SOL from contributor to the presale PDA (escrow)
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.contributor.key(),
            &presale.key(),
            lamports,
        );
        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.contributor.to_account_info(),
                presale.to_account_info(),
            ],
        )?;

        // Track contribution amount per contributor (accumulates)
        let contrib = &mut ctx.accounts.contribution;
        contrib.presale = presale.key();
        contrib.contributor = ctx.accounts.contributor.key();
        contrib.amount = contrib.amount.saturating_add(lamports);
        contrib.bump = ctx.bumps.contribution;

        presale.total_raised = presale.total_raised.saturating_add(lamports);
        Ok(())
    }

    // Presale: end and withdraw funds to admin after timer or if target reached
    pub fn end_presale(ctx: Context<EndPresale>) -> Result<()> {
        let presale = &mut ctx.accounts.presale;
        require!(presale.is_active, ErrorCode::PresaleNotActive);
        require!(ctx.accounts.admin.key() == presale.admin, ErrorCode::Unauthorized);

        let clock = Clock::get()?;
        let reached_time = clock.unix_timestamp >= presale.end_ts;
        let reached_target = presale.total_raised >= presale.target_lamports;
        require!(reached_time || reached_target, ErrorCode::PresaleNotEnded);

        // Transfer lamports by directly adjusting balances (source has data)
        let presale_info = presale.to_account_info();
        let admin_info = ctx.accounts.admin.to_account_info();
        let presale_lamports = **presale_info.lamports.borrow();
        let rent_exempt = Rent::get()?.minimum_balance(Presale::space());
        let transferable = presale_lamports.saturating_sub(rent_exempt);
        if transferable > 0 {
            **presale_info.try_borrow_mut_lamports()? -= transferable;
            **admin_info.try_borrow_mut_lamports()? += transferable;
        }

        presale.is_active = false;
        Ok(())
    }

    // Staking: Initialize the staking pool with reward token and rate
    pub fn initialize_stake_pool(
        ctx: Context<InitializeStakePool>,
        reward_rate_per_second: u64, // Reward tokens per second (base rate before multiplier)
    ) -> Result<()> {
        let stake_pool = &mut ctx.accounts.stake_pool;
        stake_pool.admin = ctx.accounts.admin.key();
        stake_pool.reward_token_mint = ctx.accounts.reward_token_mint.key();
        stake_pool.reward_rate_per_second = reward_rate_per_second;
        stake_pool.total_staked = 0;
        stake_pool.bump = ctx.bumps.stake_pool;

        msg!("Stake pool initialized with reward rate: {} tokens/second", reward_rate_per_second);
        Ok(())
    }

    // Staking: Stake an NFT into the vault
    pub fn stake_nft(ctx: Context<StakeNFT>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let nft_type = &ctx.accounts.nft_type;
        let stake_pool = &mut ctx.accounts.stake_pool;

        // Verify NFT metadata belongs to the collection
        let metadata_account_info = ctx.accounts.nft_metadata.to_account_info();
        let metadata: TokenMetadata = try_from_slice_unchecked(&metadata_account_info.data.borrow())?;
        let collection = metadata.collection.ok_or(ErrorCode::InvalidNFTMint)?;
        require!(collection.key == ctx.accounts.collection.mint, ErrorCode::InvalidNFTMint);

        let clock = Clock::get()?;

        // Initialize stake account
        stake_account.owner = ctx.accounts.staker.key();
        stake_account.nft_mint = ctx.accounts.nft_mint.key();
        stake_account.nft_type = nft_type.key();
        stake_account.stake_pool = stake_pool.key();
        stake_account.stake_timestamp = clock.unix_timestamp;
        stake_account.last_claim_timestamp = clock.unix_timestamp;
        stake_account.stake_multiplier = nft_type.stake_multiplier;
        stake_account.bump = ctx.bumps.stake_account;

        // Transfer NFT from staker to vault
        let transfer_cpi_accounts = anchor_spl::token::Transfer {
            from: ctx.accounts.staker_nft_token_account.to_account_info(),
            to: ctx.accounts.vault_nft_token_account.to_account_info(),
            authority: ctx.accounts.staker.to_account_info(),
        };
        let transfer_cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_cpi_accounts,
        );
        anchor_spl::token::transfer(transfer_cpi_ctx, 1)?;

        stake_pool.total_staked += 1;

        msg!(
            "NFT staked: {} with multiplier {}",
            ctx.accounts.nft_mint.key(),
            nft_type.stake_multiplier
        );
        Ok(())
    }

    // Staking: Unstake NFT and claim all pending rewards
    pub fn unstake_nft(ctx: Context<UnstakeNFT>) -> Result<()> {
        let stake_account = &ctx.accounts.stake_account;

        require!(stake_account.owner == ctx.accounts.staker.key(), ErrorCode::Unauthorized);

        let clock = Clock::get()?;

        // Calculate and transfer pending rewards
        let time_staked = clock.unix_timestamp.saturating_sub(stake_account.last_claim_timestamp);
        let reward_rate_per_second = ctx.accounts.stake_pool.reward_rate_per_second;
        let base_rewards = (time_staked as u64)
            .saturating_mul(reward_rate_per_second);
        let rewards = base_rewards
            .saturating_mul(stake_account.stake_multiplier)
            .saturating_div(10000); // Divide by 10000 because multiplier is in basis points

        let pool_bump = ctx.accounts.stake_pool.bump;
        if rewards > 0 {
            let pool_seeds = &[
                b"stake_pool".as_ref(),
                &[pool_bump],
            ];
            let signer = &[&pool_seeds[..]];

            let transfer_cpi_accounts = anchor_spl::token::Transfer {
                from: ctx.accounts.reward_token_vault.to_account_info(),
                to: ctx.accounts.staker_reward_token_account.to_account_info(),
                authority: ctx.accounts.stake_pool.to_account_info(),
            };
            let transfer_cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                transfer_cpi_accounts,
                signer,
            );
            anchor_spl::token::transfer(transfer_cpi_ctx, rewards)?;
        }

        // Transfer NFT back from vault to staker
        let nft_mint_key = stake_account.nft_mint;
        let staker_key = ctx.accounts.staker.key();
        let stake_account_seeds = &[
            b"stake_account",
            staker_key.as_ref(),
            nft_mint_key.as_ref(),
            &[stake_account.bump],
        ];
        let signer = &[&stake_account_seeds[..]];

        let nft_transfer_cpi_accounts = anchor_spl::token::Transfer {
            from: ctx.accounts.vault_nft_token_account.to_account_info(),
            to: ctx.accounts.staker_nft_token_account.to_account_info(),
            authority: ctx.accounts.stake_account.to_account_info(),
        };
        let nft_transfer_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            nft_transfer_cpi_accounts,
            signer,
        );
        anchor_spl::token::transfer(nft_transfer_cpi_ctx, 1)?;

        ctx.accounts.stake_pool.total_staked = ctx.accounts.stake_pool.total_staked.saturating_sub(1);

        msg!(
            "NFT unstaked: {}, rewards claimed: {}",
            nft_mint_key,
            rewards
        );
        Ok(())
    }

    // Staking: Claim rewards without unstaking
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let stake_pool = &ctx.accounts.stake_pool;

        require!(stake_account.owner == ctx.accounts.staker.key(), ErrorCode::Unauthorized);

        let clock = Clock::get()?;

        // Calculate rewards since last claim
        let time_since_last_claim = clock.unix_timestamp.saturating_sub(stake_account.last_claim_timestamp);
        let base_rewards = (time_since_last_claim as u64)
            .saturating_mul(stake_pool.reward_rate_per_second);
        let rewards = base_rewards
            .saturating_mul(stake_account.stake_multiplier)
            .saturating_div(10000); // Divide by 10000 because multiplier is in basis points

        if rewards > 0 {
            let pool_seeds = &[
                b"stake_pool".as_ref(),
                &[stake_pool.bump],
            ];
            let signer = &[&pool_seeds[..]];

            let transfer_cpi_accounts = anchor_spl::token::Transfer {
                from: ctx.accounts.reward_token_vault.to_account_info(),
                to: ctx.accounts.staker_reward_token_account.to_account_info(),
                authority: ctx.accounts.stake_pool.to_account_info(),
            };
            let transfer_cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                transfer_cpi_accounts,
                signer,
            );
            anchor_spl::token::transfer(transfer_cpi_ctx, rewards)?;

            // Update last claim timestamp
            stake_account.last_claim_timestamp = clock.unix_timestamp;

            msg!("Rewards claimed: {}", rewards);
        } else {
            msg!("No rewards to claim");
        }

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
        space = 8 + 32 + 4 + collection_name.len() + 4 + 10 + 4 + 200 + 2 + 32 + 1 + 1,
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
#[instruction(type_name: String)]
pub struct CreateNFTType<'info> {
    #[account(
        mut,
        seeds = [b"collection", collection.name.as_bytes()],
        bump = collection.bump,
    )]
    pub collection: Account<'info, NFTCollection>,

    #[account(
        init,
        payer = admin,
        space = 8
            + 32
            + 4 + type_name.len()
            + 4 + 200
            + 8
            + 8
            + 8
            + 8
            + 1,
        seeds = [b"type", collection.key().as_ref(), type_name.as_bytes()],
        bump,
    )]
    pub nft_type: Account<'info, NftType>,

    #[account(mut, constraint = admin.key() == collection.admin)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(type_name: String)]
pub struct MintNFTFromCollection<'info> {
    #[account(
        mut,
        seeds = [b"collection", collection.name.as_bytes()],
        bump = collection.bump,
    )]
    pub collection: Account<'info, NFTCollection>,

    #[account(
        mut,
        seeds = [
            b"type",
            collection.key().as_ref(),
            type_name.as_bytes(),
        ],
        bump = nft_type.bump,
        constraint = nft_type.collection == collection.key(),
    )]
    pub nft_type: Account<'info, NftType>,

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

#[derive(Accounts)]
#[instruction(room_id: u64)]
pub struct CreateRoom<'info> {
	#[account(
		init,
		payer = creator,
		space = Room::space(None),
		seeds = [b"room", creator.key().as_ref(), &room_id.to_le_bytes()],
		bump
	)]
	pub room: Account<'info, Room>,

	#[account(mut)]
	pub creator: Signer<'info>,

	/// CHECK: Mint of an NFT the creator owns
	pub nft_mint: Account<'info, Mint>,

	/// CHECK: Metadata account of the provided NFT mint
	#[account(
		seeds = [
			b"metadata",
			token_metadata_program.key().as_ref(),
			nft_mint.key().as_ref(),
		],
		bump,
		seeds::program = token_metadata_program.key(),
	)]
	pub nft_metadata: UncheckedAccount<'info>,

	/// CHECK: Collection mint that rooms should be gated by
	pub collection_mint: Account<'info, Mint>,

	#[account(
		constraint = creator_nft_token.owner == creator.key(),
		constraint = creator_nft_token.mint == nft_mint.key(),
	)]
	pub creator_nft_token: Account<'info, TokenAccount>,
	pub system_program: Program<'info, System>,
	/// CHECK: Token Metadata Program
	pub token_metadata_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct JoinRoom<'info> {
	#[account(mut, has_one = creator, seeds = [b"room", creator.key().as_ref(), &room.room_id.to_le_bytes()], bump = room.bump)]
	pub room: Account<'info, Room>,

	/// CHECK: only used as seed and authority check
	pub creator: UncheckedAccount<'info>,

	#[account(mut)]
	pub challenger: Signer<'info>,

	/// CHECK: Mint of an NFT the challenger owns
	pub nft_mint: Account<'info, Mint>,

	/// CHECK: Metadata account of the provided NFT mint
	#[account(
		seeds = [
			b"metadata",
			token_metadata_program.key().as_ref(),
			nft_mint.key().as_ref(),
		],
		bump,
		seeds::program = token_metadata_program.key(),
	)]
	pub nft_metadata: UncheckedAccount<'info>,

	/// CHECK: Collection mint that rooms should be gated by
	pub collection_mint: Account<'info, Mint>,

	#[account(
		constraint = challenger_nft_token.owner == challenger.key(),
		constraint = challenger_nft_token.mint == nft_mint.key(),
	)]
	pub challenger_nft_token: Account<'info, TokenAccount>,
	pub system_program: Program<'info, System>,
	/// CHECK: Token Metadata Program
	pub token_metadata_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ResolveRoom<'info> {
	#[account(
		mut,
		close = creator,
		seeds = [b"room", creator.key().as_ref(), &room.room_id.to_le_bytes()],
		bump = room.bump
	)]
	pub room: Account<'info, Room>,

	#[account(mut)]
	pub creator: Signer<'info>,
	pub system_program: Program<'info, System>,
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
pub struct Presale {
    pub admin: Pubkey,
    pub start_ts: i64,
    pub end_ts: i64,
    pub total_raised: u64,
    pub target_lamports: u64,
    pub is_active: bool,
    pub bump: u8,
}

impl Presale {
    pub fn space() -> usize {
        // discriminator
        8 +
        // admin
        32 +
        // start_ts
        8 +
        // end_ts
        8 +
        // total_raised
        8 +
        // target_lamports
        8 +
        // is_active
        1 +
        // bump
        1
    }
}

#[account]
pub struct PresaleContribution {
    pub presale: Pubkey,
    pub contributor: Pubkey,
    pub amount: u64,
    pub bump: u8,
}

impl PresaleContribution {
    pub fn space() -> usize {
        8 + 32 + 32 + 8 + 1
    }
}

#[account]
pub struct NFTCollection {
    pub admin: Pubkey,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub royalty: u16,
    pub mint: Pubkey,
    pub is_active: bool,
    pub bump: u8,
}

#[account]
pub struct NftType {
    pub collection: Pubkey,
    pub name: String,
    pub uri: String,
    pub price: u64,
    pub max_supply: u64,
    pub current_supply: u64,
    pub stake_multiplier: u64, // Multiplier for staking rewards (basis points, e.g., 10000 = 1x)
    pub bump: u8,
}

#[account]
pub struct Room {
	pub creator: Pubkey,
	pub challenger: Option<Pubkey>,
	pub room_id: u64,
	pub stake_lamports: u64,
	pub status: u8,
	pub bump: u8,
}

impl Room {
	pub fn space(_name: Option<&str>) -> usize {
		// discriminator
		8 +
		// creator
		32 +
		// challenger (Option<Pubkey>) -> 1 + 32
		1 + 32 +
		// room_id
		8 +
		// stake_lamports
		8 +
		// status
		1 +
		// bump
		1
	}
}

#[account]
pub struct StakePool {
    pub admin: Pubkey,
    pub reward_token_mint: Pubkey,
    pub reward_rate_per_second: u64,
    pub total_staked: u64,
    pub bump: u8,
}

impl StakePool {
    pub fn space() -> usize {
        8 + // discriminator
        32 + // admin
        32 + // reward_token_mint
        8 + // reward_rate_per_second
        8 + // total_staked
        1 // bump
    }
}

#[account]
pub struct StakeAccount {
    pub owner: Pubkey,
    pub nft_mint: Pubkey,
    pub nft_type: Pubkey,
    pub stake_pool: Pubkey,
    pub stake_timestamp: i64,
    pub last_claim_timestamp: i64,
    pub stake_multiplier: u64,
    pub bump: u8,
}

impl StakeAccount {
    pub fn space() -> usize {
        8 + // discriminator
        32 + // owner
        32 + // nft_mint
        32 + // nft_type
        32 + // stake_pool
        8 + // stake_timestamp
        8 + // last_claim_timestamp
        8 + // stake_multiplier
        1 // bump
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum RoomStatus {
	Waiting = 0,
	Ongoing = 1,
	Closed = 2,
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
	#[msg("Room is not in waiting state")]
	RoomNotWaiting,
	#[msg("Room is not in ongoing state")]
	RoomNotOngoing,
	#[msg("Room already has a challenger")]
	RoomHasChallenger,
    #[msg("Presale is not active")]
    PresaleNotActive,
    #[msg("Presale has ended")]
    PresaleEnded,
    #[msg("Presale cannot be ended yet")]
    PresaleNotEnded,
    #[msg("Invalid stake multiplier")]
    InvalidStakeMultiplier,
    #[msg("NFT not staked")]
    NFTNotStaked,
    #[msg("NFT already staked")]
    NFTAlreadyStaked,
    #[msg("Invalid NFT mint")]
    InvalidNFTMint,
}

// Accounts for presale
#[derive(Accounts)]
pub struct InitializePresale<'info> {
    #[account(
        init,
        payer = admin,
        space = Presale::space(),
        seeds = [b"presale"],
        bump
    )]
    pub presale: Account<'info, Presale>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ContributePresale<'info> {
    #[account(mut, seeds = [b"presale"], bump = presale.bump)]
    pub presale: Account<'info, Presale>,

    #[account(
        init_if_needed,
        payer = contributor,
        space = PresaleContribution::space(),
        seeds = [b"contrib", presale.key().as_ref(), contributor.key().as_ref()],
        bump
    )]
    pub contribution: Account<'info, PresaleContribution>,

    #[account(mut)]
    pub contributor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EndPresale<'info> {
    #[account(mut, seeds = [b"presale"], bump = presale.bump)]
    pub presale: Account<'info, Presale>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RestartPresale<'info> {
    #[account(mut, seeds = [b"presale"], bump = presale.bump)]
    pub presale: Account<'info, Presale>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// Staking Accounts
#[derive(Accounts)]
pub struct InitializeStakePool<'info> {
    #[account(
        init,
        payer = admin,
        space = StakePool::space(),
        seeds = [b"stake_pool"],
        bump
    )]
    pub stake_pool: Account<'info, StakePool>,

    /// CHECK: Reward token mint - validated in handler
    pub reward_token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        token::mint = reward_token_mint,
        token::authority = stake_pool,
        seeds = [b"reward_vault"],
        bump
    )]
    pub reward_token_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct StakeNFT<'info> {
    #[account(
        mut,
        seeds = [b"stake_pool"],
        bump = stake_pool.bump
    )]
    pub stake_pool: Account<'info, StakePool>,

    #[account(
        init,
        payer = staker,
        space = StakeAccount::space(),
        seeds = [b"stake_account", staker.key().as_ref(), nft_mint.key().as_ref()],
        bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        seeds = [b"collection", collection.name.as_bytes()],
        bump = collection.bump,
    )]
    pub collection: Account<'info, NFTCollection>,

    #[account(
        seeds = [b"type", collection.key().as_ref(), nft_type.name.as_bytes()],
        bump = nft_type.bump,
    )]
    pub nft_type: Account<'info, NftType>,

    /// CHECK: NFT mint
    pub nft_mint: Account<'info, Mint>,

    /// CHECK: NFT Metadata account
    #[account(
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            nft_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub nft_metadata: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = staker_nft_token_account.owner == staker.key(),
        constraint = staker_nft_token_account.mint == nft_mint.key(),
        constraint = staker_nft_token_account.amount >= 1,
    )]
    pub staker_nft_token_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = staker,
        token::mint = nft_mint,
        token::authority = stake_account,
        seeds = [b"vault", nft_mint.key().as_ref()],
        bump
    )]
    pub vault_nft_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub staker: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    /// CHECK: Token Metadata Program
    pub token_metadata_program: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct UnstakeNFT<'info> {
    #[account(
        mut,
        seeds = [b"stake_pool"],
        bump = stake_pool.bump
    )]
    pub stake_pool: Account<'info, StakePool>,

    #[account(
        mut,
        close = staker,
        seeds = [b"stake_account", staker.key().as_ref(), stake_account.nft_mint.as_ref()],
        bump = stake_account.bump,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    /// CHECK: Reward token mint from stake pool
    pub reward_token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"reward_vault"],
        bump,
        constraint = reward_token_vault.mint == reward_token_mint.key(),
    )]
    pub reward_token_vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = staker,
        associated_token::mint = reward_token_mint,
        associated_token::authority = staker,
    )]
    pub staker_reward_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"vault", stake_account.nft_mint.as_ref()],
        bump,
    )]
    pub vault_nft_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = staker_nft_token_account.owner == staker.key(),
        constraint = staker_nft_token_account.mint == stake_account.nft_mint,
    )]
    pub staker_nft_token_account: Account<'info, TokenAccount>,

    #[account(mut, constraint = staker.key() == stake_account.owner)]
    pub staker: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(
        seeds = [b"stake_pool"],
        bump = stake_pool.bump
    )]
    pub stake_pool: Account<'info, StakePool>,

    #[account(
        mut,
        seeds = [b"stake_account", staker.key().as_ref(), stake_account.nft_mint.as_ref()],
        bump = stake_account.bump,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    /// CHECK: Reward token mint from stake pool
    pub reward_token_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"reward_vault"],
        bump,
        constraint = reward_token_vault.mint == reward_token_mint.key(),
    )]
    pub reward_token_vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = staker,
        associated_token::mint = reward_token_mint,
        associated_token::authority = staker,
    )]
    pub staker_reward_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub staker: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}