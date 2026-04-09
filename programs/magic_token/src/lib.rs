use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};
use spl_token_2022::extension::ExtensionType;

declare_id!("6GHPQ4n1StWk7cqspfWGLdgpmx8cbqZ1QgSZxfCwVyR9");

/// Розмір MagicConfig
const MAGIC_CONFIG_SIZE: usize = 8 + 32 + 32 + 32 + 1 + 1;

#[program]
pub mod magic_token {
    use super::*;

    /// Ініціалізація конфігурації MagicToken
    pub fn initialize(ctx: Context<InitializeMagic>, marketplace_program: Pubkey) -> Result<()> {
        let config = &mut ctx.accounts.magic_config;
        config.admin = ctx.accounts.admin.key();
        config.marketplace_program = marketplace_program;
        config.bump = ctx.bumps.magic_config;
        config.authority_bump = ctx.bumps.magic_authority;
        config.mint = Pubkey::default();
        Ok(())
    }

    /// Створення мінта MagicToken (Token-2022 з MetadataPointer)
    pub fn create_magic_mint(
        ctx: Context<CreateMagicMint>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        let config = &ctx.accounts.magic_config;
        require!(
            ctx.accounts.admin.key() == config.admin,
            MagicError::Unauthorized
        );

        let mint_key = ctx.accounts.magic_mint.key();
        let authority_key = ctx.accounts.magic_authority.key();
        let admin_info = ctx.accounts.admin.to_account_info();
        let mint_info = ctx.accounts.magic_mint.to_account_info();
        let authority_info = ctx.accounts.magic_authority.to_account_info();

        // Обчислення розміру акаунту
        let base_size = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(
            &[ExtensionType::MetadataPointer],
        )
        .map_err(|_| MagicError::MintCreationFailed)?;

        let metadata_data_len =
            32 + 32 + (4 + name.len()) + (4 + symbol.len()) + (4 + uri.len()) + 4;
        let tlv_len = 4 + metadata_data_len;
        let space = base_size + tlv_len + 128;

        let lamports = Rent::get()?.minimum_balance(space);
        let mint_bump = ctx.bumps.magic_mint;
        let mint_seeds: &[&[u8]] = &[b"magic_mint", &[mint_bump]];
        let authority_bump = config.authority_bump;
        let authority_seeds: &[&[u8]] = &[b"magic_authority", &[authority_bump]];

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

        // 2. Ініціалізація MetadataPointer
        invoke_signed(
            &spl_token_2022::extension::metadata_pointer::instruction::initialize(
                &spl_token_2022::ID,
                &mint_key,
                Some(authority_key),
                Some(mint_key),
            )
            .map_err(|_| MagicError::MintCreationFailed)?,
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
            .map_err(|_| MagicError::MintCreationFailed)?,
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

        // Збереження адреси мінта
        let config = &mut ctx.accounts.magic_config;
        config.mint = mint_key;

        Ok(())
    }

    /// Мінтинг MagicToken — тільки через авторизовану програму (Marketplace)
    pub fn mint_magic_token(ctx: Context<MintMagicToken>, amount: u64) -> Result<()> {
        let config = &ctx.accounts.magic_config;
        let authority_key = ctx.accounts.authority.key();

        // Перевірка: адмін або PDA маркетплейсу
        if authority_key != config.admin {
            let (marketplace_pda, _) = Pubkey::find_program_address(
                &[b"cpi_authority"],
                &config.marketplace_program,
            );
            require!(authority_key == marketplace_pda, MagicError::Unauthorized);
        }

        // Мінтинг через magic_authority PDA
        let authority_bump = config.authority_bump;
        let seeds: &[&[u8]] = &[b"magic_authority", &[authority_bump]];

        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token_interface::MintTo {
                    mint: ctx.accounts.magic_mint.to_account_info(),
                    to: ctx.accounts.recipient_token_account.to_account_info(),
                    authority: ctx.accounts.magic_authority.to_account_info(),
                },
                &[seeds],
            ),
            amount,
        )?;

        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
pub struct InitializeMagic<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = MAGIC_CONFIG_SIZE,
        seeds = [b"magic_config"],
        bump,
    )]
    pub magic_config: Account<'info, MagicConfig>,

    /// CHECK: PDA авторизації мінтингу
    #[account(seeds = [b"magic_authority"], bump)]
    pub magic_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateMagicMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut, seeds = [b"magic_config"], bump = magic_config.bump)]
    pub magic_config: Account<'info, MagicConfig>,

    /// CHECK: Створюється в інструкції
    #[account(mut, seeds = [b"magic_mint"], bump)]
    pub magic_mint: UncheckedAccount<'info>,

    /// CHECK: PDA авторизації
    #[account(seeds = [b"magic_authority"], bump = magic_config.authority_bump)]
    pub magic_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    /// CHECK: Token-2022 програма
    #[account(address = spl_token_2022::ID)]
    pub token_program: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintMagicToken<'info> {
    /// Авторизація: адмін або CPI PDA маркетплейсу
    pub authority: Signer<'info>,

    #[account(seeds = [b"magic_config"], bump = magic_config.bump)]
    pub magic_config: Account<'info, MagicConfig>,

    /// CHECK: PDA авторизації мінтингу
    #[account(seeds = [b"magic_authority"], bump = magic_config.authority_bump)]
    pub magic_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub magic_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

// ==================== State ====================

/// Конфігурація MagicToken
#[account]
pub struct MagicConfig {
    /// Адміністратор
    pub admin: Pubkey,
    /// Адреса мінта MagicToken
    pub mint: Pubkey,
    /// Авторизована програма маркетплейсу
    pub marketplace_program: Pubkey,
    /// Bump для PDA magic_config
    pub bump: u8,
    /// Bump для PDA magic_authority
    pub authority_bump: u8,
}

// ==================== Errors ====================

#[error_code]
pub enum MagicError {
    #[msg("Неавторизований доступ")]
    Unauthorized,
    #[msg("Помилка створення мінта")]
    MintCreationFailed,
}
