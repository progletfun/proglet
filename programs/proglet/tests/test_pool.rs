use {
    anchor_lang::{
        prelude::Pubkey, solana_program::instruction::Instruction, system_program,
        AccountDeserialize, AccountSerialize, InstructionData, Space, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, get_associated_token_address_with_program_id},
        token_2022::{self, spl_token_2022},
    },
    litesvm::LiteSVM,
    proglet::{constants::*, state::*},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const SOL: u64 = 1_000_000_000;
const DECIMALS: u8 = 6;

struct Env {
    svm: LiteSVM,
    admin: Keypair,
    crank: Keypair,
    user: Keypair,
    treasury: Pubkey,
    mint: Pubkey,
    pool: Pubkey,
    team_fee_vault: Pubkey,
    pool_vault: Pubkey,
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], signers: &[&Keypair]) -> Result<(), String> {
    svm.expire_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e.err))
}

fn token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let data = svm.get_account(account).unwrap().data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

fn lamports(svm: &LiteSVM, account: &Pubkey) -> u64 {
    svm.get_account(account).map(|a| a.lamports).unwrap_or(0)
}

fn read_pool(svm: &LiteSVM, pool: &Pubkey) -> Pool {
    let data = svm.get_account(pool).unwrap().data;
    Pool::try_deserialize(&mut data.as_slice()).unwrap()
}

fn set_program_account<T: AccountSerialize>(
    svm: &mut LiteSVM,
    address: Pubkey,
    value: &T,
    space: usize,
) {
    let mut data = Vec::with_capacity(space);
    value.try_serialize(&mut data).unwrap();
    assert_eq!(data.len(), space);
    let lamports = svm.minimum_balance_for_rent_exemption(space);
    svm.set_account(address, solana_account(lamports, data, proglet::id()))
        .unwrap();
}

fn solana_account(lamports: u64, data: Vec<u8>, owner: Pubkey) -> solana_account::Account {
    solana_account::Account {
        lamports,
        data,
        owner,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_ata(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    let ix = associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &token_2022::ID,
    );
    send(svm, &[ix], &[payer]).unwrap();
    get_associated_token_address_with_program_id(owner, mint, &token_2022::ID)
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let program_id = proglet::id();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/proglet.so"
    ));
    svm.add_program(program_id, bytes).unwrap();

    let admin = Keypair::new();
    let crank = Keypair::new();
    let user = Keypair::new();
    let treasury = Pubkey::new_unique();
    for k in [&admin, &crank, &user] {
        svm.airdrop(&k.pubkey(), 1_000 * SOL).unwrap();
    }

    let program_data = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &anchor_lang::solana_program::bpf_loader_upgradeable::ID,
    )
    .0;
    let mut pd = svm.get_account(&program_data).unwrap();
    pd.data[12] = 1;
    pd.data[13..45].copy_from_slice(admin.pubkey().as_ref());
    svm.set_account(program_data, pd).unwrap();

    let config = Pubkey::find_program_address(&[CONFIG_SEED], &program_id).0;
    let ix = Instruction::new_with_bytes(
        program_id,
        &proglet::instruction::InitializeConfig {
            crank_authority: crank.pubkey(),
            treasury,
        }
        .data(),
        proglet::accounts::InitializeConfig {
            admin: admin.pubkey(),
            config,
            program: program_id,
            program_data,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &[&admin]).unwrap();

    let mint = Pubkey::new_unique();
    let mut mint_data = vec![0u8; 82];
    mint_data[0..4].copy_from_slice(&1u32.to_le_bytes());
    mint_data[4..36].copy_from_slice(admin.pubkey().as_ref());
    mint_data[44] = DECIMALS;
    mint_data[45] = 1;
    let rent = svm.minimum_balance_for_rent_exemption(82);
    svm.set_account(mint, solana_account(rent, mint_data, token_2022::ID))
        .unwrap();

    let (pool, pool_bump) = Pubkey::find_program_address(&[POOL_SEED, mint.as_ref()], &program_id);
    let (team_fee_vault, team_bump) =
        Pubkey::find_program_address(&[TEAM_FEE_SEED, pool.as_ref()], &program_id);
    let creator_bump = Pubkey::find_program_address(&[CREATOR_SEED, mint.as_ref()], &program_id).1;
    let pool_state = Pool {
        mint,
        token_program: token_2022::ID,
        sol_reserve: 0,
        token_reserve: 0,
        crank_buffer_sol: 0,
        crank_buffer_token: 0,
        total_debt: 0,
        total_loan_reserve_token: 0,
        created_slot: 0,
        last_swap_slot: 0,
        oracle_head: 0,
        observations: [Observation::default(); ORACLE_LEN],
        bump: pool_bump,
        creator_bump,
        team_fee_bump: team_bump,
        total_collateral: 0,
    };
    set_program_account(&mut svm, pool, &pool_state, 8 + Pool::INIT_SPACE);
    set_program_account(
        &mut svm,
        team_fee_vault,
        &TeamFeeVault {
            pool,
            bump: team_bump,
        },
        8 + TeamFeeVault::INIT_SPACE,
    );
    let pool_vault = create_ata(&mut svm, &admin, &pool, &mint);

    Env {
        svm,
        admin,
        crank,
        user,
        treasury,
        mint,
        pool,
        team_fee_vault,
        pool_vault,
    }
}

fn seed_ix(env: &Env, signer: &Pubkey, signer_ata: Pubkey, sol: u64, tokens: u64) -> Instruction {
    Instruction::new_with_bytes(
        proglet::id(),
        &proglet::instruction::SeedLiquidity {
            sol_amount: sol,
            token_amount: tokens,
        }
        .data(),
        proglet::accounts::SeedLiquidity {
            crank_authority: *signer,
            config: Pubkey::find_program_address(&[CONFIG_SEED], &proglet::id()).0,
            pool: env.pool,
            mint: env.mint,
            pool_token_vault: env.pool_vault,
            crank_token_account: signer_ata,
            token_program: token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn swap_ix(env: &Env, user_ata: Pubkey, is_buy: bool, amount_in: u64, min_out: u64) -> Instruction {
    Instruction::new_with_bytes(
        proglet::id(),
        &proglet::instruction::Swap {
            is_buy,
            amount_in,
            min_out,
        }
        .data(),
        proglet::accounts::Swap {
            user: env.user.pubkey(),
            pool: env.pool,
            team_fee_vault: env.team_fee_vault,
            mint: env.mint,
            pool_token_vault: env.pool_vault,
            user_token_account: user_ata,
            token_program: token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn fund_crank(env: &mut Env, amount: u64) -> Pubkey {
    let (admin, crank, mint) = (env.admin.insecure_clone(), env.crank.pubkey(), env.mint);
    let ata = create_ata(&mut env.svm, &admin, &crank, &mint);
    let ix = spl_token_2022::instruction::mint_to(
        &token_2022::ID,
        &mint,
        &ata,
        &admin.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    send(&mut env.svm, &[ix], &[&admin]).unwrap();
    ata
}

#[test]
fn seed_buy_sell_sweep() {
    let mut env = setup();
    let crank_ata = fund_crank(&mut env, 1_000_000_000_000_000);

    let ix = seed_ix(
        &env,
        &env.crank.pubkey(),
        crank_ata,
        10 * SOL,
        100_000_000_000_000,
    );
    send(&mut env.svm, &[ix], &[&env.crank]).unwrap();
    let pool = read_pool(&env.svm, &env.pool);
    assert_eq!(
        (pool.sol_reserve, pool.token_reserve),
        (10 * SOL, 100_000_000_000_000)
    );
    assert_eq!(token_balance(&env.svm, &env.pool_vault), pool.token_reserve);

    let ix = seed_ix(
        &env,
        &env.crank.pubkey(),
        crank_ata,
        SOL,
        999_000_000_000_000,
    );
    send(&mut env.svm, &[ix], &[&env.crank]).unwrap();
    let pool = read_pool(&env.svm, &env.pool);
    assert_eq!(
        (pool.sol_reserve, pool.token_reserve),
        (11 * SOL, 110_000_000_000_000)
    );

    let (user, mint) = (env.user.insecure_clone(), env.mint);
    let user_ata = create_ata(&mut env.svm, &user, &user.pubkey(), &mint);
    env.svm.warp_to_slot(100);
    let k_before = pool.sol_reserve as u128 * pool.token_reserve as u128;
    let ix = swap_ix(&env, user_ata, true, SOL, 1);
    send(&mut env.svm, &[ix], &[&env.user]).unwrap();
    let pool = read_pool(&env.svm, &env.pool);
    let bought = token_balance(&env.svm, &user_ata);
    assert!(bought > 0);
    assert_eq!(pool.sol_reserve, 12 * SOL - 2_000_000);
    assert!(pool.sol_reserve as u128 * pool.token_reserve as u128 > k_before);
    assert_eq!(pool.observations[pool.oracle_head as usize].slot, 100);

    let pool_rent = env
        .svm
        .minimum_balance_for_rent_exemption(8 + Pool::INIT_SPACE);
    assert_eq!(lamports(&env.svm, &env.pool), pool_rent + pool.sol_reserve);
    let team_rent = env
        .svm
        .minimum_balance_for_rent_exemption(8 + TeamFeeVault::INIT_SPACE);
    assert_eq!(
        lamports(&env.svm, &env.team_fee_vault),
        team_rent + 2_000_000
    );

    env.svm.warp_to_slot(120);
    let ix = swap_ix(&env, user_ata, false, bought, SOL);
    assert!(send(&mut env.svm, &[ix], &[&env.user]).is_err());

    let user_before = lamports(&env.svm, &env.user.pubkey());
    let ix = swap_ix(&env, user_ata, false, bought, 1);
    send(&mut env.svm, &[ix], &[&env.user]).unwrap();
    let got = lamports(&env.svm, &env.user.pubkey()) + 5_000 - user_before;
    assert!(
        got > 0 && got < SOL,
        "round trip must lose to fees, got {got}"
    );
    let pool = read_pool(&env.svm, &env.pool);
    assert_eq!(lamports(&env.svm, &env.pool), pool_rent + pool.sol_reserve);
    assert_eq!(token_balance(&env.svm, &env.pool_vault), pool.token_reserve);

    let team_fee = lamports(&env.svm, &env.team_fee_vault) - team_rent;
    let ix = Instruction::new_with_bytes(
        proglet::id(),
        &proglet::instruction::Sweep {}.data(),
        proglet::accounts::Sweep {
            config: Pubkey::find_program_address(&[CONFIG_SEED], &proglet::id()).0,
            team_fee_vault: env.team_fee_vault,
            treasury: env.treasury,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.user]).unwrap();
    assert_eq!(lamports(&env.svm, &env.treasury), team_fee);
    assert_eq!(lamports(&env.svm, &env.team_fee_vault), team_rent);
}

#[test]
fn only_crank_can_seed() {
    let mut env = setup();
    let crank_ata = fund_crank(&mut env, 1_000_000_000_000);
    let ix = seed_ix(&env, &env.user.pubkey(), crank_ata, SOL, 1_000_000_000);
    assert!(send(&mut env.svm, &[ix], &[&env.user]).is_err());
}

#[test]
fn swap_on_empty_pool_fails() {
    let mut env = setup();
    let (user, mint) = (env.user.insecure_clone(), env.mint);
    let user_ata = create_ata(&mut env.svm, &user, &user.pubkey(), &mint);
    let ix = swap_ix(&env, user_ata, true, SOL, 1);
    assert!(send(&mut env.svm, &[ix], &[&env.user]).is_err());
}
