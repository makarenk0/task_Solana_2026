use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};
use spl_token_2022::extension::ExtensionType;

declare_id!("GdW5QURPJVagNXwiNkUYtGkJZ5w367oEGig5s7Lm9SVw");

/// Розмір ItemMetadata
const ITEM_METADATA_SIZE: usize = 8 + 1 + 32 + 32 + 1;

/// Назви предметів
const ITEM_NAMES: [&str; 4] = [
    "Шабля козака",
    "Посох старійшини",
    "Броня характерника",
    "Бойовий браслет",
];

/// Символи предметів
const ITEM_SYMBOLS: [&str; 4] = ["SABER", "STAFF", "ARMOR", "BRACELET"];

#[program]
pub mod item_nft {
    use super::*;

    /// Створення NFT предмета (Token-2022 з MetadataPointer, supply=1)
    /// Викликається через CPI з програми crafting або напряму адміном
    pub fn create_item(
        ctx: Context<CreateItem>,
        item_type: u8,
        uri: String,
    ) -> Result<()> {
        require!(item_type < 4, NftError::InvalidItemType);

        // Перевірка авторизації: CPI від crafting або payer (тестування)
        let authority_key = ctx.accounts.authority.key();
        let (crafting_pda, _) = Pubkey::find_program_address(
            &[b"cpi_authority"],
            &ctx.accounts.crafting_program.key(),
        );
        if authority_key != crafting_pda {
            require!(
                authority_key == ctx.accounts.payer.key(),
                NftError::Unauthorized
            );
        }

        let name = ITEM_NAMES[item_type as usize].to_string();
        let symbol = ITEM_SYMBOLS[item_type as usize].to_string();

        let mint_key = ctx.accounts.item_mint.key();
        let nft_authority_key = ctx.accounts.nft_authority.key();
        let payer_info = ctx.accounts.payer.to_account_info();
        let mint_info = ctx.accounts.item_mint.to_account_info();
        let authority_info = ctx.accounts.nft_authority.to_account_info();

        let nft_auth_bump = ctx.bumps.nft_authority;
        let nft_auth_seeds: &[&[u8]] = &[b"nft_authority", &[nft_auth_bump]];

        // 1. Обчислення розміру акаунту для Token-2022 з MetadataPointer
        let base_size = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(
            &[ExtensionType::MetadataPointer],
        )
        .map_err(|_| NftError::MintCreationFailed)?;

        let metadata_data_len =
            32 + 32 + (4 + name.len()) + (4 + symbol.len()) + (4 + uri.len()) + 4;
        let tlv_len = 4 + metadata_data_len;
        let space = base_size + tlv_len + 128;

        let lamports = Rent::get()?.minimum_balance(space);
        let mint_bump = ctx.bumps.item_mint;
        let mint_seeds: &[&[u8]] = &[
            b"item_mint",
            ctx.accounts.player.key.as_ref(),
            &[item_type],
            &[mint_bump],
        ];

        // 2. Створення акаунту
        invoke_signed(
            &anchor_lang::solana_program::system_instruction::create_account(
                &payer_info.key(),
                &mint_key,
                lamports,
                space as u64,
                &spl_token_2022::ID,
            ),
            &[payer_info.clone(), mint_info.clone()],
            &[mint_seeds],
        )?;

        // 3. Ініціалізація MetadataPointer
        invoke_signed(
            &spl_token_2022::extension::metadata_pointer::instruction::initialize(
                &spl_token_2022::ID,
                &mint_key,
                Some(nft_authority_key),
                Some(mint_key),
            )
            .map_err(|_| NftError::MintCreationFailed)?,
            &[mint_info.clone()],
            &[mint_seeds],
        )?;

        // 4. Ініціалізація мінта (decimals=0, authority=nft_authority)
        invoke_signed(
            &spl_token_2022::instruction::initialize_mint2(
                &spl_token_2022::ID,
                &mint_key,
                &nft_authority_key,
                None, // no freeze authority
                0,    // decimals = 0
            )
            .map_err(|_| NftError::MintCreationFailed)?,
            &[mint_info.clone()],
            &[mint_seeds],
        )?;

        // 5. Ініціалізація TokenMetadata
        invoke_signed(
            &spl_token_metadata_interface::instruction::initialize(
                &spl_token_2022::ID,
                &mint_key,
                &nft_authority_key,
                &mint_key,
                &nft_authority_key,
                name,
                symbol,
                uri,
            ),
            &[mint_info.clone(), authority_info.clone()],
            &[nft_auth_seeds],
        )?;

        // 6. Мінтинг 1 токена на акаунт гравця
        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token_interface::MintTo {
                    mint: mint_info.clone(),
                    to: ctx.accounts.player_token_account.to_account_info(),
                    authority: authority_info.clone(),
                },
                &[nft_auth_seeds],
            ),
            1,
        )?;

        // 7. Відключення мінт-авторити (робить це справжнім NFT — більше не можна мінтити)
        token_interface::set_authority(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token_interface::SetAuthority {
                    current_authority: authority_info,
                    account_or_mint: mint_info,
                },
                &[nft_auth_seeds],
            ),
            anchor_spl::token_interface::spl_token_2022::instruction::AuthorityType::MintTokens,
            None,
        )?;

        // 8. Збереження метаданих предмета
        let item_metadata = &mut ctx.accounts.item_metadata;
        item_metadata.item_type = item_type;
        item_metadata.owner = ctx.accounts.player.key();
        item_metadata.mint = mint_key;
        item_metadata.bump = ctx.bumps.item_metadata;

        Ok(())
    }

    /// Спалення NFT предмета
    pub fn burn_item(ctx: Context<BurnItem>) -> Result<()> {
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token_interface::Burn {
                    mint: ctx.accounts.item_mint.to_account_info(),
                    from: ctx.accounts.player_token_account.to_account_info(),
                    authority: ctx.accounts.player.to_account_info(),
                },
            ),
            1,
        )?;

        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
#[instruction(item_type: u8)]
pub struct CreateItem<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Авторизація: CPI PDA від crafting або payer
    pub authority: Signer<'info>,

    /// CHECK: Публічний ключ гравця
    pub player: UncheckedAccount<'info>,

    /// CHECK: Програма crafting для перевірки CPI авторизації
    pub crafting_program: UncheckedAccount<'info>,

    /// CHECK: PDA авторизації NFT операцій
    #[account(seeds = [b"nft_authority"], bump)]
    pub nft_authority: UncheckedAccount<'info>,

    /// CHECK: PDA мінт нового NFT (Token-2022)
    #[account(
        mut,
        seeds = [b"item_mint", player.key().as_ref(), &[item_type]],
        bump,
    )]
    pub item_mint: UncheckedAccount<'info>,

    /// Токен-акаунт гравця для нового NFT
    #[account(mut)]
    pub player_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Метадані предмета (наш PDA)
    #[account(
        init,
        payer = payer,
        space = ITEM_METADATA_SIZE,
        seeds = [b"item", item_mint.key().as_ref()],
        bump,
    )]
    pub item_metadata: Account<'info, ItemMetadata>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct BurnItem<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(mut)]
    pub item_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub player_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"item", item_mint.key().as_ref()],
        bump = item_metadata.bump,
        close = player,
        constraint = item_metadata.owner == player.key() @ NftError::NotItemOwner,
    )]
    pub item_metadata: Account<'info, ItemMetadata>,

    pub token_program: Interface<'info, TokenInterface>,
}

// ==================== State ====================

/// Метадані предмета (NFT)
#[account]
pub struct ItemMetadata {
    /// Тип предмета (0=Шабля, 1=Посох, 2=Броня, 3=Браслет)
    pub item_type: u8,
    /// Поточний власник
    pub owner: Pubkey,
    /// Адреса мінта NFT
    pub mint: Pubkey,
    /// Bump для PDA
    pub bump: u8,
}

// ==================== Errors ====================

#[error_code]
pub enum NftError {
    #[msg("Невірний тип предмета (має бути 0-3)")]
    InvalidItemType,
    #[msg("Неавторизований доступ")]
    Unauthorized,
    #[msg("Ви не є власником цього предмета")]
    NotItemOwner,
    #[msg("Помилка створення мінта")]
    MintCreationFailed,
}
