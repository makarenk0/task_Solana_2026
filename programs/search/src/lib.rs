use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;
use resource_manager::cpi::accounts::MintResource;
use resource_manager::program::ResourceManager;
use resource_manager::{GameConfig, Player};

declare_id!("7vJBA1edH8F869HkF33cQQZfD3RWV6P85rQojUgB5AX1");

/// Інтервал очікування між пошуками (секунди)
const SEARCH_COOLDOWN: i64 = 60;
/// Кількість ресурсів за один пошук
const RESOURCES_PER_SEARCH: usize = 3;

#[program]
pub mod search {
    use super::*;

    /// Пошук ресурсів — генерує 3 випадкових ресурси кожні 60 секунд
    ///
    /// remaining_accounts очікує 12 акаунтів:
    /// [0..6] — мінти 6 ресурсів (відповідно до resource_mints у GameConfig)
    /// [6..12] — токен-акаунти гравця для кожного ресурсу
    pub fn search_resources<'a>(ctx: Context<'_, '_, 'a, 'a, SearchResources<'a>>) -> Result<()> {
        let clock = Clock::get()?;
        let player = &mut ctx.accounts.player;

        // Перевірка таймера (60 секунд)
        require!(
            clock.unix_timestamp - player.last_search_timestamp >= SEARCH_COOLDOWN,
            SearchError::SearchCooldown
        );
        player.last_search_timestamp = clock.unix_timestamp;

        // Генерація 3 випадкових ресурсів
        let player_key_bytes = ctx.accounts.player_owner.key().to_bytes();
        let seed = (clock.unix_timestamp as u64)
            .wrapping_mul(clock.slot)
            .wrapping_add(u64::from_le_bytes(
                player_key_bytes[0..8].try_into().unwrap(),
            ));

        let resource_ids = [
            ((seed) % 6) as usize,
            ((seed >> 16) % 6) as usize,
            ((seed >> 32) % 6) as usize,
        ];

        // Перевірка remaining_accounts
        let remaining = &ctx.remaining_accounts;
        require!(remaining.len() >= 12, SearchError::NotEnoughAccounts);

        let search_auth_bump = ctx.bumps.search_authority;
        let signer_seeds: &[&[u8]] = &[b"cpi_authority", &[search_auth_bump]];

        // Мінтинг кожного ресурсу через CPI до resource_manager
        for i in 0..RESOURCES_PER_SEARCH {
            let resource_id = resource_ids[i];
            let resource_mint = &remaining[resource_id];
            let player_ata = &remaining[6 + resource_id];

            // Валідація: мінт має відповідати GameConfig
            require!(
                resource_mint.key() == ctx.accounts.game_config.resource_mints[resource_id],
                SearchError::InvalidResourceMint
            );

            let cpi_accounts = MintResource {
                authority: ctx.accounts.search_authority.to_account_info(),
                game_config: ctx.accounts.game_config.to_account_info(),
                game_authority: ctx.accounts.game_authority.to_account_info(),
                resource_mint: resource_mint.to_account_info(),
                player_token_account: player_ata.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            };

            resource_manager::cpi::mint_resource(
                CpiContext::new_with_signer(
                    ctx.accounts.resource_manager_program.to_account_info(),
                    cpi_accounts,
                    &[signer_seeds],
                ),
                1,
            )?;
        }

        // Логування знайдених ресурсів
        msg!(
            "Знайдено ресурси: {}, {}, {}",
            resource_ids[0],
            resource_ids[1],
            resource_ids[2]
        );

        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
pub struct SearchResources<'info> {
    #[account(mut)]
    pub player_owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"player", player_owner.key().as_ref()],
        bump = player.bump,
        seeds::program = resource_manager_program.key(),
        constraint = player.owner == player_owner.key() @ SearchError::InvalidPlayer,
    )]
    pub player: Account<'info, Player>,

    #[account(
        seeds = [b"game_config"],
        bump = game_config.bump,
        seeds::program = resource_manager_program.key(),
    )]
    pub game_config: Account<'info, GameConfig>,

    /// CHECK: PDA авторизації для CPI до resource_manager
    #[account(seeds = [b"cpi_authority"], bump)]
    pub search_authority: UncheckedAccount<'info>,

    /// CHECK: PDA game_authority з resource_manager
    pub game_authority: UncheckedAccount<'info>,

    pub resource_manager_program: Program<'info, ResourceManager>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

// ==================== Errors ====================

#[error_code]
pub enum SearchError {
    #[msg("Пошук доступний кожні 60 секунд")]
    SearchCooldown,
    #[msg("Недостатньо акаунтів (потрібно 12: 6 мінтів + 6 токен-акаунтів)")]
    NotEnoughAccounts,
    #[msg("Невірний акаунт гравця")]
    InvalidPlayer,
    #[msg("Мінт ресурсу не відповідає GameConfig")]
    InvalidResourceMint,
}
