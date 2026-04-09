use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};
use item_nft::ItemMetadata;
use magic_token::cpi::accounts::MintMagicToken;
use magic_token::program::MagicToken;
use magic_token::MagicConfig;
use resource_manager::GameConfig;
use resource_manager::program::ResourceManager;

declare_id!("FRQoF4nGihSUyuu4Z5kVsaqxoxGXsweDmxzMaBGaaPW3");

/// Розмір Listing акаунту
const LISTING_SIZE: usize = 8 + 32 + 32 + 8 + 1 + 1;

#[program]
pub mod marketplace {
    use super::*;

    /// Виставити предмет на продаж
    pub fn list_item(ctx: Context<ListItem>, price: u64) -> Result<()> {
        require!(price > 0, MarketError::InvalidPrice);

        // Перенесення NFT на ескроу-акаунт маркетплейсу
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token_interface::TransferChecked {
                    from: ctx.accounts.seller_token_account.to_account_info(),
                    mint: ctx.accounts.item_mint.to_account_info(),
                    to: ctx.accounts.escrow_token_account.to_account_info(),
                    authority: ctx.accounts.seller.to_account_info(),
                },
            ),
            1,
            0, // decimals
        )?;

        // Створення лістингу
        let listing = &mut ctx.accounts.listing;
        listing.seller = ctx.accounts.seller.key();
        listing.item_mint = ctx.accounts.item_mint.key();
        listing.item_type = ctx.accounts.item_metadata.item_type;
        listing.price = price;
        listing.bump = ctx.bumps.listing;

        msg!("Предмет виставлено на продаж за {} MagicToken", price);

        Ok(())
    }

    /// Скасувати лістинг
    pub fn cancel_listing(ctx: Context<CancelListing>) -> Result<()> {
        let listing = &ctx.accounts.listing;
        let item_mint_key = listing.item_mint;
        let escrow_bump = ctx.bumps.escrow_authority;
        let seeds: &[&[u8]] = &[b"escrow", item_mint_key.as_ref(), &[escrow_bump]];

        // Повернення NFT продавцю
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token_interface::TransferChecked {
                    from: ctx.accounts.escrow_token_account.to_account_info(),
                    mint: ctx.accounts.item_mint.to_account_info(),
                    to: ctx.accounts.seller_token_account.to_account_info(),
                    authority: ctx.accounts.escrow_authority.to_account_info(),
                },
                &[seeds],
            ),
            1,
            0,
        )?;

        msg!("Лістинг скасовано");
        Ok(())
    }

    /// Продаж предмета на маркетплейсі (отримання MagicToken)
    /// NFT спалюється, продавець отримує MagicToken
    pub fn sell_item(ctx: Context<SellItem>) -> Result<()> {
        let item_metadata = &ctx.accounts.item_metadata;
        let game_config = &ctx.accounts.game_config;
        let item_type = item_metadata.item_type;

        require!(item_type < 4, MarketError::InvalidItemType);
        let price = game_config.item_prices[item_type as usize];

        // 1. Спалення NFT (гравець є власником і підписує)
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token_interface::Burn {
                    mint: ctx.accounts.item_mint.to_account_info(),
                    from: ctx.accounts.seller_token_account.to_account_info(),
                    authority: ctx.accounts.seller.to_account_info(),
                },
            ),
            1,
        )?;

        // 2. Мінтинг MagicToken через CPI до magic_token програми
        let market_auth_bump = ctx.bumps.marketplace_authority;
        let signer_seeds: &[&[u8]] = &[b"cpi_authority", &[market_auth_bump]];

        let mint_accounts = MintMagicToken {
            authority: ctx.accounts.marketplace_authority.to_account_info(),
            magic_config: ctx.accounts.magic_config.to_account_info(),
            magic_authority: ctx.accounts.magic_authority.to_account_info(),
            magic_mint: ctx.accounts.magic_mint.to_account_info(),
            recipient_token_account: ctx.accounts.seller_magic_token_account.to_account_info(),
            token_program: ctx.accounts.token_2022_program.to_account_info(),
        };

        magic_token::cpi::mint_magic_token(
            CpiContext::new_with_signer(
                ctx.accounts.magic_token_program.to_account_info(),
                mint_accounts,
                &[signer_seeds],
            ),
            price,
        )?;

        msg!("Предмет продано за {} MagicToken", price);

        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
pub struct ListItem<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    pub item_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [b"item", item_mint.key().as_ref()],
        bump = item_metadata.bump,
        seeds::program = item_nft::ID,
        constraint = item_metadata.owner == seller.key() @ MarketError::NotItemOwner,
    )]
    pub item_metadata: Account<'info, ItemMetadata>,

    #[account(mut)]
    pub seller_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub escrow_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Ескроу авторитет PDA
    #[account(seeds = [b"escrow", item_mint.key().as_ref()], bump)]
    pub escrow_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = seller,
        space = LISTING_SIZE,
        seeds = [b"listing", item_mint.key().as_ref()],
        bump,
    )]
    pub listing: Account<'info, Listing>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CancelListing<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    #[account(
        mut,
        seeds = [b"listing", listing.item_mint.as_ref()],
        bump = listing.bump,
        constraint = listing.seller == seller.key() @ MarketError::NotSeller,
        close = seller,
    )]
    pub listing: Account<'info, Listing>,

    /// CHECK: Мінт предмета для transfer_checked
    pub item_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub escrow_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Ескроу авторитет PDA
    #[account(seeds = [b"escrow", listing.item_mint.as_ref()], bump)]
    pub escrow_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub seller_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct SellItem<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    #[account(mut)]
    pub item_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [b"item", item_mint.key().as_ref()],
        bump = item_metadata.bump,
        seeds::program = item_nft::ID,
        constraint = item_metadata.owner == seller.key() @ MarketError::NotItemOwner,
    )]
    pub item_metadata: Account<'info, ItemMetadata>,

    #[account(mut)]
    pub seller_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: PDA авторизації маркетплейсу для CPI до magic_token
    #[account(seeds = [b"cpi_authority"], bump)]
    pub marketplace_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [b"game_config"],
        bump = game_config.bump,
        seeds::program = resource_manager_program.key(),
    )]
    pub game_config: Account<'info, GameConfig>,

    #[account(
        seeds = [b"magic_config"],
        bump = magic_config.bump,
        seeds::program = magic_token_program.key(),
    )]
    pub magic_config: Account<'info, MagicConfig>,

    /// CHECK: PDA magic_authority
    pub magic_authority: UncheckedAccount<'info>,

    /// CHECK: MagicToken мінт
    #[account(mut)]
    pub magic_mint: UncheckedAccount<'info>,

    /// CHECK: Токен-акаунт продавця для MagicToken
    #[account(mut)]
    pub seller_magic_token_account: UncheckedAccount<'info>,

    pub resource_manager_program: Program<'info, ResourceManager>,
    pub magic_token_program: Program<'info, MagicToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub token_2022_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

// ==================== State ====================

/// Лістинг предмета на маркетплейсі
#[account]
pub struct Listing {
    /// Продавець
    pub seller: Pubkey,
    /// Мінт предмета
    pub item_mint: Pubkey,
    /// Ціна в MagicToken
    pub price: u64,
    /// Тип предмета
    pub item_type: u8,
    /// Bump для PDA
    pub bump: u8,
}

// ==================== Errors ====================

#[error_code]
pub enum MarketError {
    #[msg("Невірна ціна")]
    InvalidPrice,
    #[msg("Невірний тип предмета")]
    InvalidItemType,
    #[msg("Ви не є власником цього предмета")]
    NotItemOwner,
    #[msg("У вас немає предмета для продажу")]
    NoItemToSell,
    #[msg("Ви не є продавцем цього лістингу")]
    NotSeller,
}
