/**
 * Скрипт взаємодії з грою "Козацький бізнес"
 *
 * Використання:
 *   npx ts-node scripts/interact.ts <command> [args]
 *
 * Команди:
 *   init           — Ініціалізація гри (тільки адмін)
 *   register       — Реєстрація гравця
 *   search         — Пошук ресурсів
 *   craft <type>   — Крафт предмета (0=Шабля, 1=Посох, 2=Броня, 3=Браслет)
 *   sell <mint>     — Продаж предмета на маркетплейсі
 *   balance        — Показати баланс ресурсів та MagicToken
 */

import * as anchor from "@coral-xyz/anchor";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Connection,
  clusterApiUrl,
} from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  getAccount,
  createAssociatedTokenAccountInstruction,
} from "@solana/spl-token";

const RESOURCE_SYMBOLS = ["WOOD", "IRON", "GOLD", "LEATHER", "STONE", "DIAMOND"];

async function main() {
  // Налаштування провайдера
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const resourceManager = anchor.workspace.ResourceManager;
  const search = anchor.workspace.Search;
  const crafting = anchor.workspace.Crafting;
  const itemNft = anchor.workspace.ItemNft;
  const marketplace = anchor.workspace.Marketplace;
  const magicToken = anchor.workspace.MagicToken;

  const wallet = provider.wallet as anchor.Wallet;
  const command = process.argv[2];

  // PDA адреси
  const [gameConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_config")],
    resourceManager.programId
  );
  const [gameAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("game_authority")],
    resourceManager.programId
  );
  const [magicConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("magic_config")],
    magicToken.programId
  );
  const [magicAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("magic_authority")],
    magicToken.programId
  );
  const [magicMintPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("magic_mint")],
    magicToken.programId
  );
  const [playerPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("player"), wallet.publicKey.toBuffer()],
    resourceManager.programId
  );
  const [searchAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("cpi_authority")],
    search.programId
  );
  const [marketplaceAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("cpi_authority")],
    marketplace.programId
  );

  // Мінти ресурсів
  const resourceMints: PublicKey[] = [];
  for (let i = 0; i < 6; i++) {
    const [mintPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("resource_mint"), Buffer.from([i])],
      resourceManager.programId
    );
    resourceMints.push(mintPda);
  }

  switch (command) {
    case "init": {
      console.log("🏰 Ініціалізація гри 'Козацький бізнес'...");

      const itemPrices = [100, 150, 200, 250].map((p) => new anchor.BN(p));

      // 1. Ініціалізація GameConfig
      await resourceManager.methods
        .initializeGame(
          itemPrices,
          search.programId,
          crafting.programId,
          itemNft.programId,
          marketplace.programId,
          magicToken.programId
        )
        .accounts({
          admin: wallet.publicKey,
          gameConfig: gameConfigPda,
          gameAuthority: gameAuthorityPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      console.log("  GameConfig створено");

      // 2. Створення мінтів ресурсів
      const RESOURCE_NAMES = ["Дерево", "Залізо", "Золото", "Шкіра", "Камінь", "Алмаз"];
      for (let i = 0; i < 6; i++) {
        await resourceManager.methods
          .initResourceMint(
            i,
            RESOURCE_NAMES[i],
            RESOURCE_SYMBOLS[i],
            `https://kozak.game/resources/${RESOURCE_SYMBOLS[i].toLowerCase()}.json`
          )
          .accounts({
            admin: wallet.publicKey,
            gameConfig: gameConfigPda,
            resourceMint: resourceMints[i],
            gameAuthority: gameAuthorityPda,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: SYSVAR_RENT_PUBKEY,
          })
          .rpc();
        console.log(`  Ресурс ${RESOURCE_NAMES[i]} (${RESOURCE_SYMBOLS[i]}) створено`);
      }

      // 3. Ініціалізація MagicToken
      await magicToken.methods
        .initialize(marketplace.programId)
        .accounts({
          admin: wallet.publicKey,
          magicConfig: magicConfigPda,
          magicAuthority: magicAuthorityPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      await magicToken.methods
        .createMagicMint(
          "MagicToken",
          "MAGIC",
          "https://kozak.game/magic-token.json"
        )
        .accounts({
          admin: wallet.publicKey,
          magicConfig: magicConfigPda,
          magicMint: magicMintPda,
          magicAuthority: magicAuthorityPda,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .rpc();
      console.log("  MagicToken створено");

      // 4. Збереження MagicToken у GameConfig
      await resourceManager.methods
        .setMagicTokenMint(magicMintPda)
        .accounts({
          admin: wallet.publicKey,
          gameConfig: gameConfigPda,
        })
        .rpc();

      console.log("✅ Гра ініціалізована!");
      break;
    }

    case "register": {
      console.log("📋 Реєстрація гравця...");
      await resourceManager.methods
        .registerPlayer()
        .accounts({
          owner: wallet.publicKey,
          player: playerPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      // Створення ATA для ресурсів
      for (let i = 0; i < 6; i++) {
        const ata = getAssociatedTokenAddressSync(
          resourceMints[i],
          wallet.publicKey,
          false,
          TOKEN_2022_PROGRAM_ID
        );
        try {
          await getAccount(provider.connection, ata, undefined, TOKEN_2022_PROGRAM_ID);
        } catch {
          const ix = createAssociatedTokenAccountInstruction(
            wallet.publicKey,
            ata,
            wallet.publicKey,
            resourceMints[i],
            TOKEN_2022_PROGRAM_ID
          );
          const tx = new anchor.web3.Transaction().add(ix);
          await provider.sendAndConfirm(tx);
          console.log(`  ATA для ${RESOURCE_SYMBOLS[i]} створено`);
        }
      }

      // Створення ATA для MagicToken
      const magicAta = getAssociatedTokenAddressSync(
        magicMintPda,
        wallet.publicKey,
        false,
        TOKEN_2022_PROGRAM_ID
      );
      try {
        await getAccount(provider.connection, magicAta, undefined, TOKEN_2022_PROGRAM_ID);
      } catch {
        const ix = createAssociatedTokenAccountInstruction(
          wallet.publicKey,
          magicAta,
          wallet.publicKey,
          magicMintPda,
          TOKEN_2022_PROGRAM_ID
        );
        const tx = new anchor.web3.Transaction().add(ix);
        await provider.sendAndConfirm(tx);
        console.log("  ATA для MagicToken створено");
      }

      console.log("✅ Гравця зареєстровано!");
      break;
    }

    case "search": {
      console.log("🔍 Пошук ресурсів...");

      const playerAtas = resourceMints.map((mint) =>
        getAssociatedTokenAddressSync(
          mint,
          wallet.publicKey,
          false,
          TOKEN_2022_PROGRAM_ID
        )
      );

      const remainingAccounts = [
        ...resourceMints.map((mint) => ({
          pubkey: mint,
          isWritable: true,
          isSigner: false,
        })),
        ...playerAtas.map((ata) => ({
          pubkey: ata,
          isWritable: true,
          isSigner: false,
        })),
      ];

      await search.methods
        .searchResources()
        .accounts({
          playerOwner: wallet.publicKey,
          player: playerPda,
          gameConfig: gameConfigPda,
          searchAuthority: searchAuthorityPda,
          gameAuthority: gameAuthorityPda,
          resourceManagerProgram: resourceManager.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .remainingAccounts(remainingAccounts)
        .rpc();

      console.log("✅ Ресурси знайдено! Перевірте баланс командою: balance");
      break;
    }

    case "balance": {
      console.log("💰 Баланс гравця:");
      console.log("--- Ресурси ---");

      for (let i = 0; i < 6; i++) {
        const ata = getAssociatedTokenAddressSync(
          resourceMints[i],
          wallet.publicKey,
          false,
          TOKEN_2022_PROGRAM_ID
        );
        try {
          const account = await getAccount(
            provider.connection,
            ata,
            undefined,
            TOKEN_2022_PROGRAM_ID
          );
          console.log(
            `  ${RESOURCE_SYMBOLS[i]}: ${account.amount.toString()}`
          );
        } catch {
          console.log(`  ${RESOURCE_SYMBOLS[i]}: 0`);
        }
      }

      console.log("--- MagicToken ---");
      const magicAta = getAssociatedTokenAddressSync(
        magicMintPda,
        wallet.publicKey,
        false,
        TOKEN_2022_PROGRAM_ID
      );
      try {
        const account = await getAccount(
          provider.connection,
          magicAta,
          undefined,
          TOKEN_2022_PROGRAM_ID
        );
        console.log(`  MAGIC: ${account.amount.toString()}`);
      } catch {
        console.log("  MAGIC: 0");
      }
      break;
    }

    default:
      console.log("Використання: npx ts-node scripts/interact.ts <command>");
      console.log("Команди: init, register, search, balance, craft <type>, sell <mint>");
      break;
  }
}

main().catch(console.error);
