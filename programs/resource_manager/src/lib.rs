use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};
use spl_token_2022::extension::ExtensionType;

declare_id!("56yHF44YTUM9UuNCpki66PfYdHLgUEVtZB9jZEqfc6A9");

/// Розмір GameConfig акаунту
const GAME_CONFIG_SIZE: usize = 8 + 32 + (32 * 6) + 32 + (8 * 4) + 1 + 1 + 32 + 32 + 32 + 32 + 32;
/// Розмір Player акаунту
const PLAYER_SIZE: usize = 8 + 32 + 8 + 1;

#[program]
pub mod resource_manager {
    use super::*;

    /// Ініціалізація гри: створення GameConfig
    pub fn initialize_game(
        ctx: Context<InitializeGame>,
        item_prices: [u64; 4],
        search_program: Pubkey,
        crafting_program: Pubkey,
        item_nft_program: Pubkey,
        marketplace_program: Pubkey,
        magic_token_program: Pubkey,
    ) -> Result<()> {
        let game_config = &mut ctx.accounts.game_config;
        game_config.admin = ctx.accounts.admin.key();
        game_config.item_prices = item_prices;
        game_config.bump = ctx.bumps.game_config;
        game_config.authority_bump = ctx.bumps.game_authority;
        game_config.resource_mints = [Pubkey::default(); 6];
        game_config.magic_token_mint = Pubkey::default();
        game_config.search_program = search_program;
        game_config.crafting_program = crafting_program;
        game_config.item_nft_program = item_nft_program;
        game_config.marketplace_program = marketplace_program;
        game_config.magic_token_program = magic_token_program;
        Ok(())
    }

    /// Ініціалізація мінта ресурсу з Token-2022 та MetadataPointer
    pub fn init_resource_mint(
        ctx: Context<InitResourceMint>,
        resource_id: u8,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        require!(resource_id < 6, GameError::InvalidResourceId);
        let game_config = &ctx.accounts.game_config;
        require!(
            ctx.accounts.admin.key() == game_config.admin,
            GameError::Unauthorized
        );

        let mint_key = ctx.accounts.resource_mint.key();
        let authority_key = ctx.accounts.game_authority.key();
        let admin_info = ctx.accounts.admin.to_account_info();
        let mint_info = ctx.accounts.resource_mint.to_account_info();
        let authority_info = ctx.accounts.game_authority.to_account_info();

        // Обчислення розміру акаунту для Token-2022 з MetadataPointer
        let base_size = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(
            &[ExtensionType::MetadataPointer],
        )
        .map_err(|_| GameError::MintCreationFailed)?;

        let metadata_data_len =
            32 + 32 + (4 + name.len()) + (4 + symbol.len()) + (4 + uri.len()) + 4;
        let tlv_len = 4 + metadata_data_len;
        let space = base_size + tlv_len + 128;

        let lamports = Rent::get()?.minimum_balance(space);
        let mint_bump = ctx.bumps.resource_mint;
        let mint_seeds: &[&[u8]] = &[b"resource_mint", &[resource_id], &[mint_bump]];
        let authority_bump = game_config.authority_bump;
        let authority_seeds: &[&[u8]] = &[b"game_authority", &[authority_bump]];

        // 1. Створення акаунту
        invoke_signed(
            &anchor_lang::solana_program::system_instruction::create_account(
                &admin_info.key(),
                &mint_key,
                lamports,
                space as u64,
                &spl_token_2022::ID,
            ),
            &[admin_info.clone(), mint_info.clone()],
            &[mint_seeds],
        )?;

        // 2. Ініціалізація MetadataPointer (вказує на сам мінт)
        invoke_signed(
            &spl_token_2022::extension::metadata_pointer::instruction::initialize(
                &spl_token_2022::ID,
                &mint_key,
                Some(authority_key),
                Some(mint_key),
            )
            .map_err(|_| GameError::MintCreationFailed)?,
            &[mint_info.clone()],
            &[mint_seeds],
        )?;

        // 3. Ініціалізація мінта (decimals = 0)
        invoke_signed(
            &spl_token_2022::instruction::initialize_mint2(
                &spl_token_2022::ID,
                &mint_key,
                &authority_key,
                None,
                0,
            )
            .map_err(|_| GameError::MintCreationFailed)?,
            &[mint_info.clone()],
            &[mint_seeds],
        )?;

        // 4. Ініціалізація TokenMetadata
        invoke_signed(
            &spl_token_metadata_interface::instruction::initialize(
                &spl_token_2022::ID,
                &mint_key,
                &authority_key,
                &mint_key,
                &authority_key,
                name,
                symbol,
                uri,
            ),
            &[mint_info, authority_info],
            &[authority_seeds],
        )?;

        // Збереження адреси мінта в конфігурації
        let game_config = &mut ctx.accounts.game_config;
        game_config.resource_mints[resource_id as usize] = mint_key;

        Ok(())
    }

    /// Збереження адреси MagicToken мінта
    pub fn set_magic_token_mint(
        ctx: Context<SetMagicTokenMint>,
        magic_token_mint: Pubkey,
    ) -> Result<()> {
        let game_config = &mut ctx.accounts.game_config;
        require!(
            ctx.accounts.admin.key() == game_config.admin,
            GameError::Unauthorized
        );
        game_config.magic_token_mint = magic_token_mint;
        Ok(())
    }

    /// Реєстрація гравця
    pub fn register_player(ctx: Context<RegisterPlayer>) -> Result<()> {
        let player = &mut ctx.accounts.player;
        player.owner = ctx.accounts.owner.key();
        player.last_search_timestamp = 0;
        player.bump = ctx.bumps.player;
        Ok(())
    }

    /// Мінтинг ресурсів — тільки через авторизовані програми або адміна
    pub fn mint_resource(ctx: Context<MintResource>, amount: u64) -> Result<()> {
        let game_config = &ctx.accounts.game_config;
        let authority_key = ctx.accounts.authority.key();

        // Перевірка авторизації: адмін або PDA авторизованої програми
        if authority_key != game_config.admin {
            let (search_pda, _) = Pubkey::find_program_address(
                &[b"cpi_authority"],
                &game_config.search_program,
            );
            require!(authority_key == search_pda, GameError::Unauthorized);
        }

        // Мінтинг через game_authority PDA
        let authority_bump = game_config.authority_bump;
        let seeds: &[&[u8]] = &[b"game_authority", &[authority_bump]];

        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token_interface::MintTo {
                    mint: ctx.accounts.resource_mint.to_account_info(),
                    to: ctx.accounts.player_token_account.to_account_info(),
                    authority: ctx.accounts.game_authority.to_account_info(),
                },
                &[seeds],
            ),
            amount,
        )?;

        Ok(())
    }

    /// Спалення ресурсів — гравець підписує транзакцію
    pub fn burn_resource(ctx: Context<BurnResource>, amount: u64) -> Result<()> {
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token_interface::Burn {
                    mint: ctx.accounts.resource_mint.to_account_info(),
                    from: ctx.accounts.player_token_account.to_account_info(),
                    authority: ctx.accounts.player.to_account_info(),
                },
            ),
            amount,
        )?;
        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
pub struct InitializeGame<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = GAME_CONFIG_SIZE,
        seeds = [b"game_config"],
        bump,
    )]
    pub game_config: Account<'info, GameConfig>,

    /// CHECK: PDA для авторизації мінтингу ресурсів
    #[account(seeds = [b"game_authority"], bump)]
    pub game_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(resource_id: u8)]
pub struct InitResourceMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut, seeds = [b"game_config"], bump = game_config.bump)]
    pub game_config: Account<'info, GameConfig>,

    /// CHECK: Створюється в інструкції через invoke_signed
    #[account(mut, seeds = [b"resource_mint", &[resource_id]], bump)]
    pub resource_mint: UncheckedAccount<'info>,

    /// CHECK: PDA авторизації мінтингу
    #[account(seeds = [b"game_authority"], bump = game_config.authority_bump)]
    pub game_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    /// CHECK: Token-2022 програма
    #[account(address = spl_token_2022::ID)]
    pub token_program: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SetMagicTokenMint<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [b"game_config"], bump = game_config.bump)]
    pub game_config: Account<'info, GameConfig>,
}

#[derive(Accounts)]
pub struct RegisterPlayer<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = PLAYER_SIZE,
        seeds = [b"player", owner.key().as_ref()],
        bump,
    )]
    pub player: Account<'info, Player>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintResource<'info> {
    /// Авторизація: адмін або CPI PDA авторизованої програми
    pub authority: Signer<'info>,

    #[account(seeds = [b"game_config"], bump = game_config.bump)]
    pub game_config: Account<'info, GameConfig>,

    /// CHECK: PDA авторизації мінтингу
    #[account(seeds = [b"game_authority"], bump = game_config.authority_bump)]
    pub game_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub resource_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub player_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct BurnResource<'info> {
    pub player: Signer<'info>,

    #[account(mut)]
    pub resource_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub player_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

// ==================== State ====================

/// Налаштування гри
#[account]
pub struct GameConfig {
    /// Адміністратор гри
    pub admin: Pubkey,
    /// Адреси мінтів 6 ресурсів
    pub resource_mints: [Pubkey; 6],
    /// Адреса мінта MagicToken
    pub magic_token_mint: Pubkey,
    /// Ціни предметів у MagicToken [Шабля, Посох, Броня, Браслет]
    pub item_prices: [u64; 4],
    /// Bump для PDA game_config
    pub bump: u8,
    /// Bump для PDA game_authority
    pub authority_bump: u8,
    /// Авторизовані програми
    pub search_program: Pubkey,
    pub crafting_program: Pubkey,
    pub item_nft_program: Pubkey,
    pub marketplace_program: Pubkey,
    pub magic_token_program: Pubkey,
}

/// Акаунт гравця
#[account]
pub struct Player {
    /// Публічний ключ власника
    pub owner: Pubkey,
    /// Час останнього пошуку ресурсів (unix timestamp)
    pub last_search_timestamp: i64,
    /// Bump для PDA
    pub bump: u8,
}

// ==================== Errors ====================

#[error_code]
pub enum GameError {
    #[msg("Невірний ID ресурсу (має бути 0-5)")]
    InvalidResourceId,
    #[msg("Неавторизований доступ")]
    Unauthorized,
    #[msg("Помилка створення мінта")]
    MintCreationFailed,
    #[msg("Недостатньо акаунтів")]
    NotEnoughAccounts,
    #[msg("Час очікування пошуку ще не минув (60 секунд)")]
    SearchCooldown,
}
