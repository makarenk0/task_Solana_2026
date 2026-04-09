use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;
use item_nft::program::ItemNft;
use resource_manager::cpi::accounts::BurnResource;
use resource_manager::program::ResourceManager;
use resource_manager::GameConfig;

declare_id!("52JLhS24AreNFYaHhGBbFdXGRFSgZBCNaTdRSimrTTqh");

/// Рецепти крафту: [item_type] -> [(resource_id, amount)]
/// 0 = Шабля козака:     3× Залізо(1) + 1× Дерево(0) + 1× Шкіра(3)
/// 1 = Посох старійшини:  2× Дерево(0) + 1× Золото(2) + 1× Алмаз(5)
/// 2 = Броня характерника: 4× Шкіра(3) + 2× Залізо(1) + 1× Золото(2)
/// 3 = Бойовий браслет:   4× Залізо(1) + 2× Золото(2) + 2× Алмаз(5)
const RECIPES: [[(u8, u64); 3]; 4] = [
    [(1, 3), (0, 1), (3, 1)],
    [(0, 2), (2, 1), (5, 1)],
    [(3, 4), (1, 2), (2, 1)],
    [(1, 4), (2, 2), (5, 2)],
];

#[program]
pub mod crafting {
    use super::*;

    /// Крафт предмета з ресурсів
    ///
    /// remaining_accounts:
    /// [0..5] — пари (resource_mint, player_ata) для 3 інгредієнтів = 6 акаунтів
    /// [6] — item_mint PDA
    /// [7] — player_item_ata
    /// [8] — nft_authority
    /// [9] — crafting_program (self)
    /// [10] — rent
    pub fn craft_item<'a>(ctx: Context<'_, '_, 'a, 'a, CraftItem<'a>>, item_type: u8, uri: String) -> Result<()> {
        require!(item_type < 4, CraftError::InvalidItemType);

        let recipe = RECIPES[item_type as usize];
        let remaining = &ctx.remaining_accounts;

        // Потрібно: 6 (ресурси) + 5 (NFT) = 11 remaining_accounts
        require!(remaining.len() >= 11, CraftError::NotEnoughAccounts);

        // 1. Спалення ресурсів за рецептом
        for i in 0..3 {
            let (resource_id, amount) = recipe[i];
            if amount == 0 {
                continue;
            }

            let resource_mint = &remaining[i * 2];
            let player_ata = &remaining[i * 2 + 1];

            let expected_mint = ctx.accounts.game_config.resource_mints[resource_id as usize];
            require!(
                resource_mint.key() == expected_mint,
                CraftError::InvalidResourceMint
            );

            let burn_accounts = BurnResource {
                player: ctx.accounts.player.to_account_info(),
                resource_mint: resource_mint.to_account_info(),
                player_token_account: player_ata.to_account_info(),
                token_program: ctx.accounts.token_2022_program.to_account_info(),
            };

            resource_manager::cpi::burn_resource(
                CpiContext::new(
                    ctx.accounts.resource_manager_program.to_account_info(),
                    burn_accounts,
                ),
                amount,
            )?;
        }

        // 2. Створення NFT через CPI до item_nft::create_item
        let craft_auth_bump = ctx.bumps.craft_authority;
        let signer_seeds: &[&[u8]] = &[b"cpi_authority", &[craft_auth_bump]];

        let item_mint = &remaining[6];
        let player_item_ata = &remaining[7];
        let nft_authority = &remaining[8];
        let crafting_program_account = &remaining[9];
        let rent = &remaining[10];

        let create_item_accounts = item_nft::cpi::accounts::CreateItem {
            payer: ctx.accounts.player.to_account_info(),
            authority: ctx.accounts.craft_authority.to_account_info(),
            player: ctx.accounts.player.to_account_info(),
            crafting_program: crafting_program_account.to_account_info(),
            nft_authority: nft_authority.to_account_info(),
            item_mint: item_mint.to_account_info(),
            player_token_account: player_item_ata.to_account_info(),
            item_metadata: ctx.accounts.item_metadata.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            rent: rent.to_account_info(),
        };

        item_nft::cpi::create_item(
            CpiContext::new_with_signer(
                ctx.accounts.item_nft_program.to_account_info(),
                create_item_accounts,
                &[signer_seeds],
            ),
            item_type,
            uri,
        )?;

        msg!("Скрафчено предмет типу {}", item_type);

        Ok(())
    }
}

// ==================== Accounts ====================

#[derive(Accounts)]
#[instruction(item_type: u8)]
pub struct CraftItem<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        seeds = [b"game_config"],
        bump = game_config.bump,
        seeds::program = resource_manager_program.key(),
    )]
    pub game_config: Account<'info, GameConfig>,

    /// CHECK: PDA авторизації для CPI до item_nft
    #[account(seeds = [b"cpi_authority"], bump)]
    pub craft_authority: UncheckedAccount<'info>,

    /// CHECK: Створюється через CPI до item_nft
    #[account(mut)]
    pub item_metadata: UncheckedAccount<'info>,

    pub resource_manager_program: Program<'info, ResourceManager>,
    pub item_nft_program: Program<'info, ItemNft>,
    pub token_program: Interface<'info, TokenInterface>,
    pub token_2022_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

// ==================== Errors ====================

#[error_code]
pub enum CraftError {
    #[msg("Невірний тип предмета (має бути 0-3)")]
    InvalidItemType,
    #[msg("Недостатньо акаунтів")]
    NotEnoughAccounts,
    #[msg("Невірний мінт ресурсу")]
    InvalidResourceMint,
    #[msg("Недостатньо ресурсів для крафту")]
    InsufficientResources,
}
