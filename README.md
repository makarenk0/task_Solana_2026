# Гра "Козацький бізнес" — Версія для Solana

## Введення

Дане тестове завдання було підготовлено компанією WhiteBIT для студентів університету НаУКМА. Це завдання дає змогу компанії оцінити аналітичні, технічні та архітектурні навички кандидатів у екосистемі Solana.

---

## Адреси програм (Program ID)

| Програма | Program ID |
|----------|------------|
crafting: 52JLhS24AreNFYaHhGBbFdXGRFSgZBCNaTdRSimrTTqh
item_nft: GdW5QURPJVagNXwiNkUYtGkJZ5w367oEGig5s7Lm9SVw
magic_token: 6GHPQ4n1StWk7cqspfWGLdgpmx8cbqZ1QgSZxfCwVyR9
marketplace: FRQoF4nGihSUyuu4Z5kVsaqxoxGXsweDmxzMaBGaaPW3
resource_manager: 56yHF44YTUM9UuNCpki66PfYdHLgUEVtZB9jZEqfc6A9
search: 7vJBA1edH8F869HkF33cQQZfD3RWV6P85rQojUgB5AX1

---

## Вимоги до середовища

| Інструмент | Версія |
|-----------|--------|
| Rust | 1.75+ |
| Solana CLI | 1.18.17+ |
| Anchor CLI | 0.30.1 |
| Node.js | 18+ |
| Yarn | 1.22+ |

---

## Інструкції з деплою

### 1. Встановлення залежностей

```bash
# Встановлення Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Встановлення Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/v1.18.17/install)"

# Встановлення Anchor CLI
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install 0.30.1
avm use 0.30.1

# Встановлення JS залежностей
yarn install
```

### 2. Налаштування Solana

```bash
# Генерація ключа (якщо потрібно)
solana-keygen new

# Переключення на devnet
solana config set --url devnet

# Отримання SOL для деплою
solana airdrop 5
```

### 3. Білд та деплой

```bash
# Перший білд
anchor build

# Оновлення Program ID (обов'язково після першого білду!)
anchor keys sync

# Повторний білд з оновленими ID
anchor build

# Деплой на devnet
anchor deploy --provider.cluster devnet
```

### 4. Запуск тестів

```bash
# Запуск всіх тестів (localnet)
anchor test

# Запуск тестів без перебілду
anchor test --skip-build
```

---

## Архітектура

### Програми

| Програма | Призначення |
|----------|-------------|
| **resource_manager** | Керування GameConfig, мінтом/спаленням ресурсів (SPL Token-2022) |
| **search** | Логіка пошуку ресурсів з таймером (60 сек), CPI до resource_manager |
| **crafting** | Логіка крафту предметів з ресурсів, CPI до resource_manager та item_nft |
| **item_nft** | Створення/спалення NFT-предметів (Metaplex Token Metadata) |
| **marketplace** | Продаж предметів за MagicToken, CPI до magic_token |
| **magic_token** | Мінт MagicToken (SPL Token-2022), тільки через авторизовані програми |

### Базові ресурси (SPL Token-2022)

| ID | Назва | Символ | Decimals |
|----|-------|--------|----------|
| 0 | Дерево | WOOD | 0 |
| 1 | Залізо | IRON | 0 |
| 2 | Золото | GOLD | 0 |
| 3 | Шкіра | LEATHER | 0 |
| 4 | Камінь | STONE | 0 |
| 5 | Алмаз | DIAMOND | 0 |

Кожен ресурс реалізований як SPL Token-2022 з розширенням **MetadataPointer** — метадані зберігаються безпосередньо в акаунті мінта.

### Унікальні предмети (NFT через Metaplex)

| Предмет | Рецепт |
|---------|--------|
| Шабля козака (type=0) | 3× Залізо + 1× Дерево + 1× Шкіра |
| Посох старійшини (type=1) | 2× Дерево + 1× Золото + 1× Алмаз |
| Броня характерника (type=2) | 4× Шкіра + 2× Залізо + 1× Золото |
| Бойовий браслет (type=3) | 4× Залізо + 2× Золото + 2× Алмаз |

### CPI (Cross-Program Invocation) потік

```
search → resource_manager::mint_resource   (мінтинг ресурсів)
crafting → resource_manager::burn_resource  (спалення ресурсів)
crafting → item_nft::create_item            (створення NFT)
marketplace → magic_token::mint_magic_token (мінтинг MagicToken)
```

### Структура PDA (Program Derived Addresses)

```
resource_manager:
  GameConfig     → seeds: ["game_config"]
  GameAuthority  → seeds: ["game_authority"]     (mint authority для ресурсів)
  ResourceMint   → seeds: ["resource_mint", resource_id]
  Player         → seeds: ["player", owner_pubkey]

magic_token:
  MagicConfig    → seeds: ["magic_config"]
  MagicAuthority → seeds: ["magic_authority"]     (mint authority для MagicToken)
  MagicMint      → seeds: ["magic_mint"]

item_nft:
  NftAuthority   → seeds: ["nft_authority"]       (mint authority для NFT)
  ItemMetadata   → seeds: ["item", mint_pubkey]

marketplace:
  Listing        → seeds: ["listing", item_mint_pubkey]
  EscrowAuth     → seeds: ["escrow", item_mint_pubkey]

search / crafting / marketplace:
  CpiAuthority   → seeds: ["cpi_authority"]       (авторизація для CPI викликів)
```

### Механіка безпеки

- **PDA-based authorization**: Кожна програма має CPI Authority PDA. При міжпрограмному виклику, програма-ініціатор підписує транзакцію своїм PDA. Програма-отримувач перевіряє, що PDA належить авторизованій програмі.
- **Mint authority**: Мінт-авторити для ресурсів та MagicToken — PDA відповідних програм. Прямий мінтинг через Token Program неможливий.
- **Owner checks**: Всі операції з ресурсами/предметами гравця вимагають підпису власника.
- **Timer enforcement**: Пошук ресурсів обмежений он-чейн таймером (60 секунд) через PDA акаунт гравця.

---

## Структура акаунтів

```rust
/// Налаштування гри
#[account]
pub struct GameConfig {
    pub admin: Pubkey,
    pub resource_mints: [Pubkey; 6],
    pub magic_token_mint: Pubkey,
    pub item_prices: [u64; 4],
    pub bump: u8,
    pub authority_bump: u8,
    pub search_program: Pubkey,
    pub crafting_program: Pubkey,
    pub item_nft_program: Pubkey,
    pub marketplace_program: Pubkey,
    pub magic_token_program: Pubkey,
}

/// Акаунт гравця
#[account]
pub struct Player {
    pub owner: Pubkey,
    pub last_search_timestamp: i64,
    pub bump: u8,
}

/// Метадані предмета (NFT)
#[account]
pub struct ItemMetadata {
    pub item_type: u8,
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub bump: u8,
}
```

---

## Приклади взаємодії

### Через скрипт

```bash
# Ініціалізація гри (адмін)
npx ts-node scripts/interact.ts init

# Реєстрація гравця
npx ts-node scripts/interact.ts register

# Пошук ресурсів
npx ts-node scripts/interact.ts search

# Перевірка балансу
npx ts-node scripts/interact.ts balance
```

### Через TypeScript (@coral-xyz/anchor)

```typescript
import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { TOKEN_2022_PROGRAM_ID } from "@solana/spl-token";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
const program = anchor.workspace.ResourceManager;

// Реєстрація гравця
const [playerPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("player"), provider.wallet.publicKey.toBuffer()],
  program.programId
);

await program.methods
  .registerPlayer()
  .accounts({
    owner: provider.wallet.publicKey,
    player: playerPda,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .rpc();
```

---

## Структура проєкту

```
task_Solana_2026/
├── Anchor.toml              # Конфігурація Anchor
├── Cargo.toml               # Workspace конфігурація
├── package.json             # JS залежності
├── tsconfig.json            # TypeScript конфігурація
├── programs/
│   ├── resource_manager/    # Керування ресурсами (SPL Token-2022)
│   ├── search/              # Пошук ресурсів з таймером
│   ├── crafting/            # Крафт предметів
│   ├── item_nft/            # NFT предмети (Metaplex)
│   ├── marketplace/         # Маркетплейс
│   └── magic_token/         # MagicToken
├── tests/
│   └── kozak_business.ts    # Тести (100% покриття)
├── scripts/
│   └── interact.ts          # Скрипт взаємодії
└── migrations/
    └── deploy.ts
```

---

## Тестування

Тести покривають:

- **Мінтинг/спалення ресурсів** — перевірка Token-2022 операцій
- **Створення NFT через крафт** — перевірка рецептів та Metaplex інтеграції
- **Таймер пошуку (60 секунд)** — он-чейн перевірка кулдауну
- **Продаж на Marketplace** — спалення NFT та мінтинг MagicToken
- **MagicToken тільки через Marketplace** — перевірка CPI авторизації
- **Права доступу (PDA authority)** — перевірка що неавторизовані виклики відхиляються

```bash
anchor test
```

---

## Критерії оцінювання

| Критерій | Вага |
|----------|------|
| Архітектура програм | 25% |
| Безпека (PDA, authority checks) | 25% |
| Покриття тестами | 20% |
| Якість коду (Rust best practices) | 15% |
| Документація (README, коментарі) | 10% |
| Інновації/оптимізація | 5% |

---

## Корисні ресурси

- [Anchor Documentation](https://www.anchor-lang.com/)
- [Solana Developer Docs](https://solana.com/developers)
- [SPL Token-2022 Docs](https://spl.solana.com/token-2022)
- [Metaplex Token Metadata](https://developers.metaplex.com/token-metadata)
- [Solana Program Library](https://github.com/solana-labs/solana-program-library)

---

## Здача завдання

1. Створіть pull request в цьому репозиторії на GitHub.
2. Додайте всі вихідні коди, тести, скрипти та README.
3. Створіть Pull Request з описом реалізації.
4. Відправте посилання на PR через Distedu.

---

## Важливі зауваження

- Не використовуйте Solidity або EVM-інструменти.
- Всі програми мають бути деплоєні на Solana Devnet.
- MagicToken може бути замінений на будь-який інший SPL Token для тестування.
- Таймер 60 секунд має бути реалізований он-чейн (через PDA з timestamp).
- Всі транзакції мають бути підписані користувачем (owner check).
