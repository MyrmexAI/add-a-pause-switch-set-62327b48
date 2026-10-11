use litesvm::LiteSVM;
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

const TOKEN_PROGRAM: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([0; 32]);

const IX_INITIALIZE: u8 = 0;
const IX_TIP: u8 = 1;
const IX_WITHDRAW: u8 = 2;
const IX_CLOSE_JAR: u8 = 3;
const IX_SET_MAX_TIP: u8 = 4;
const IX_SET_PAUSED: u8 = 5;

const ERR_ZERO_AMOUNT: &str = "Custom(0)";
const ERR_UNAUTHORIZED: &str = "Custom(1)";
const ERR_VAULT_NOT_EMPTY: &str = "Custom(2)";
const ERR_TIP_TOO_LARGE: &str = "Custom(3)";
const ERR_PAUSED: &str = "Custom(4)";
const ERR_INSUFFICIENT_FUNDS: &str = "InsufficientFunds";

const MINTED: u64 = 1_000;

struct Ctx {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    owner: Keypair,
    tipper: Keypair,
    mint: Pubkey,
    jar: Pubkey,
    vault: Pubkey,
    tipper_ata: Pubkey,
    owner_ata: Pubkey,
}

fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    let program_id = Keypair::new().pubkey();
    let so = concat!(env!("CARGO_MANIFEST_DIR"), "/target/deploy/tip_jar.so");
    svm.add_program_from_file(program_id, so)
        .expect("run `cargo build-sbf` before `cargo test`");

    let payer = Keypair::new();
    let owner = Keypair::new();
    let tipper = Keypair::new();
    for kp in [&payer, &owner, &tipper] {
        svm.airdrop(&kp.pubkey(), 10_000_000_000).unwrap();
    }

    let mint = CreateMint::new(&mut svm, &payer).decimals(6).send().unwrap();
    let tipper_ata = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint)
        .owner(&tipper.pubkey())
        .send()
        .unwrap();
    let owner_ata = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint)
        .owner(&owner.pubkey())
        .send()
        .unwrap();
    MintTo::new(&mut svm, &payer, &mint, &tipper_ata, MINTED)
        .send()
        .unwrap();

    let (jar, _) = Pubkey::find_program_address(
        &[b"jar", owner.pubkey().as_ref(), mint.as_ref()],
        &program_id,
    );
    let (vault, _) = Pubkey::find_program_address(&[b"vault", jar.as_ref()], &program_id);

    Ctx {
        svm,
        program_id,
        payer,
        owner,
        tipper,
        mint,
        jar,
        vault,
        tipper_ata,
        owner_ata,
    }
}

fn amount_data(discriminator: u8, amount: u64) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

impl Ctx {
    fn send(&mut self, ix: Instruction, signers: &[&Keypair]) -> Result<(), String> {
        self.svm.expire_blockhash();
        let msg = Message::new(&[ix], Some(&signers[0].pubkey()));
        let tx = Transaction::new(signers, msg, self.svm.latest_blockhash());
        self.svm
            .send_transaction(tx)
            .map(|_| ())
            .map_err(|e| format!("{:?}", e.err))
    }

    fn initialize(&mut self) -> Result<(), String> {
        let mut data = vec![IX_INITIALIZE];
        data.extend_from_slice(self.owner.pubkey().as_ref());
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &data,
            vec![
                AccountMeta::new(self.payer.pubkey(), true),
                AccountMeta::new(self.jar, false),
                AccountMeta::new_readonly(self.mint, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            ],
        );
        let payer = self.payer.insecure_clone();
        self.send(ix, &[&payer])
    }

    fn tip(&mut self, amount: u64) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &amount_data(IX_TIP, amount),
            vec![
                AccountMeta::new_readonly(self.tipper.pubkey(), true),
                AccountMeta::new(self.tipper_ata, false),
                AccountMeta::new_readonly(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            ],
        );
        let tipper = self.tipper.insecure_clone();
        self.send(ix, &[&tipper])
    }

    fn withdraw(&mut self, signer: &Keypair, destination: Pubkey, amount: u64) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &amount_data(IX_WITHDRAW, amount),
            vec![
                AccountMeta::new_readonly(signer.pubkey(), true),
                AccountMeta::new_readonly(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new(destination, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            ],
        );
        self.send(ix, &[signer])
    }

    fn close_jar(&mut self, signer: &Keypair, owner: Pubkey) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &[IX_CLOSE_JAR],
            vec![
                AccountMeta::new(owner, signer.pubkey() == owner),
                AccountMeta::new(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            ],
        );
        self.send(ix, &[signer])
    }

    fn set_max_tip(&mut self, signer: &Keypair, max: u64) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &amount_data(IX_SET_MAX_TIP, max),
            vec![
                AccountMeta::new_readonly(signer.pubkey(), true),
                AccountMeta::new(self.jar, false),
            ],
        );
        self.send(ix, &[signer])
    }

    fn set_paused(&mut self, signer: &Keypair, paused: bool) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &[IX_SET_PAUSED, paused as u8],
            vec![
                AccountMeta::new_readonly(signer.pubkey(), true),
                AccountMeta::new(self.jar, false),
            ],
        );
        self.send(ix, &[signer])
    }

    fn lamports(&self, account: &Pubkey) -> u64 {
        self.svm.get_account(account).map_or(0, |a| a.lamports)
    }

    fn token_balance(&self, account: &Pubkey) -> u64 {
        let data = self.svm.get_account(account).unwrap().data;
        u64::from_le_bytes(data[64..72].try_into().unwrap())
    }
}

#[test]
fn tip_then_withdraw_works() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    ctx.tip(400).unwrap();
    ctx.tip(100).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 500);
    assert_eq!(ctx.token_balance(&ctx.tipper_ata), MINTED - 500);

    let owner = ctx.owner.insecure_clone();
    let owner_ata = ctx.owner_ata;
    ctx.withdraw(&owner, owner_ata, 300).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 200);
    assert_eq!(ctx.token_balance(&ctx.owner_ata), 300);

    ctx.withdraw(&owner, owner_ata, 200).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 0);
    assert_eq!(ctx.token_balance(&ctx.owner_ata), 500);
}

#[test]
fn zero_tip_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    let err = ctx.tip(0).unwrap_err();
    assert!(err.contains(ERR_ZERO_AMOUNT), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 0);
    assert_eq!(ctx.token_balance(&ctx.tipper_ata), MINTED);
}

#[test]
fn stranger_cannot_withdraw() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.tip(500).unwrap();

    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let stranger_ata = CreateAssociatedTokenAccount::new(&mut ctx.svm, &ctx.payer, &ctx.mint)
        .owner(&stranger.pubkey())
        .send()
        .unwrap();

    let err = ctx.withdraw(&stranger, stranger_ata, 100).unwrap_err();
    assert!(err.contains(ERR_UNAUTHORIZED), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 500);
    assert_eq!(ctx.token_balance(&stranger_ata), 0);

    // The real owner's pubkey without its signature is rejected as well.
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &amount_data(IX_WITHDRAW, 100),
        vec![
            AccountMeta::new_readonly(ctx.owner.pubkey(), false),
            AccountMeta::new_readonly(ctx.jar, false),
            AccountMeta::new(ctx.vault, false),
            AccountMeta::new(stranger_ata, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        ],
    );
    let err = ctx.send(ix, &[&stranger]).unwrap_err();
    assert!(err.contains("MissingRequiredSignature"), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 500);
}

#[test]
fn withdraw_more_than_balance_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.tip(500).unwrap();

    let owner = ctx.owner.insecure_clone();
    let owner_ata = ctx.owner_ata;
    let err = ctx.withdraw(&owner, owner_ata, 501).unwrap_err();
    assert!(err.contains(ERR_INSUFFICIENT_FUNDS), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 500);
    assert_eq!(ctx.token_balance(&ctx.owner_ata), 0);
}

#[test]
fn initialize_twice_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    assert!(ctx.initialize().is_err());
}

#[test]
fn tip_to_wrong_vault_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.vault = ctx.owner_ata;
    assert!(ctx.tip(10).is_err());
    assert_eq!(ctx.token_balance(&ctx.owner_ata), 0);
}

#[test]
fn owner_closes_empty_jar() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.tip(500).unwrap();
    let owner = ctx.owner.insecure_clone();
    let owner_ata = ctx.owner_ata;
    ctx.withdraw(&owner, owner_ata, 500).unwrap();

    let owner_key = owner.pubkey();
    let rent = ctx.lamports(&ctx.jar) + ctx.lamports(&ctx.vault);
    assert!(rent > 0);
    let owner_before = ctx.lamports(&owner_key);

    ctx.close_jar(&owner, owner_key).unwrap();

    assert_eq!(ctx.lamports(&ctx.jar), 0);
    assert_eq!(ctx.lamports(&ctx.vault), 0);
    assert!(ctx.svm.get_account(&ctx.jar).is_none());
    assert!(ctx.svm.get_account(&ctx.vault).is_none());
    // Owner pays the 5000 lamport fee as the transaction fee payer.
    assert_eq!(ctx.lamports(&owner_key), owner_before + rent - 5_000);

    // The jar can be created again after closing.
    ctx.initialize().unwrap();
}

#[test]
fn close_jar_with_tokens_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.tip(500).unwrap();

    let owner = ctx.owner.insecure_clone();
    let owner_key = owner.pubkey();
    let err = ctx.close_jar(&owner, owner_key).unwrap_err();
    assert!(err.contains(ERR_VAULT_NOT_EMPTY), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 500);
    assert!(ctx.lamports(&ctx.jar) > 0);
}

#[test]
fn stranger_cannot_close_jar() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let (jar_lamports, vault_lamports) = (ctx.lamports(&ctx.jar), ctx.lamports(&ctx.vault));

    // Stranger signs and tries to take the rent for themselves.
    let stranger_key = stranger.pubkey();
    let err = ctx.close_jar(&stranger, stranger_key).unwrap_err();
    assert!(err.contains(ERR_UNAUTHORIZED), "unexpected error: {err}");

    // Real owner's pubkey without its signature is rejected as well.
    let owner_key = ctx.owner.pubkey();
    let err = ctx.close_jar(&stranger, owner_key).unwrap_err();
    assert!(err.contains("MissingRequiredSignature"), "unexpected error: {err}");

    assert_eq!(ctx.lamports(&ctx.jar), jar_lamports);
    assert_eq!(ctx.lamports(&ctx.vault), vault_lamports);
}

#[test]
fn owner_sets_max_tip_and_tip_at_limit_works() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    let owner = ctx.owner.insecure_clone();
    ctx.set_max_tip(&owner, 100).unwrap();

    ctx.tip(100).unwrap();
    ctx.tip(1).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 101);
}

#[test]
fn tip_above_max_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    let owner = ctx.owner.insecure_clone();
    ctx.set_max_tip(&owner, 100).unwrap();

    let err = ctx.tip(101).unwrap_err();
    assert!(err.contains(ERR_TIP_TOO_LARGE), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 0);
    assert_eq!(ctx.token_balance(&ctx.tipper_ata), MINTED);

    // Zero removes the limit again.
    ctx.set_max_tip(&owner, 0).unwrap();
    ctx.tip(101).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 101);
}

#[test]
fn stranger_cannot_set_max_tip() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    let owner = ctx.owner.insecure_clone();
    ctx.set_max_tip(&owner, 100).unwrap();

    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let err = ctx.set_max_tip(&stranger, 0).unwrap_err();
    assert!(err.contains(ERR_UNAUTHORIZED), "unexpected error: {err}");

    // The limit is unchanged.
    let err = ctx.tip(101).unwrap_err();
    assert!(err.contains(ERR_TIP_TOO_LARGE), "unexpected error: {err}");

    // Owner's pubkey without its signature is rejected as well.
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &amount_data(IX_SET_MAX_TIP, 0),
        vec![
            AccountMeta::new_readonly(ctx.owner.pubkey(), false),
            AccountMeta::new(ctx.jar, false),
        ],
    );
    let err = ctx.send(ix, &[&stranger]).unwrap_err();
    assert!(err.contains("MissingRequiredSignature"), "unexpected error: {err}");
}

#[test]
fn owner_pauses_and_unpauses() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    let owner = ctx.owner.insecure_clone();

    ctx.set_paused(&owner, true).unwrap();
    let err = ctx.tip(10).unwrap_err();
    assert!(err.contains(ERR_PAUSED), "unexpected error: {err}");
    ctx.set_paused(&owner, false).unwrap();
    ctx.tip(10).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 10);

    // Pausing twice is idempotent.
    ctx.set_paused(&owner, true).unwrap();
    ctx.set_paused(&owner, true).unwrap();
}

#[test]
fn tip_while_paused_fails() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    let owner = ctx.owner.insecure_clone();
    ctx.set_paused(&owner, true).unwrap();

    let err = ctx.tip(100).unwrap_err();
    assert!(err.contains(ERR_PAUSED), "unexpected error: {err}");
    assert_eq!(ctx.token_balance(&ctx.vault), 0);
    assert_eq!(ctx.token_balance(&ctx.tipper_ata), MINTED);
}

#[test]
fn withdraw_while_paused_works() {
    let mut ctx = setup();
    ctx.initialize().unwrap();
    ctx.tip(500).unwrap();

    let owner = ctx.owner.insecure_clone();
    ctx.set_paused(&owner, true).unwrap();

    let owner_ata = ctx.owner_ata;
    ctx.withdraw(&owner, owner_ata, 500).unwrap();
    assert_eq!(ctx.token_balance(&ctx.vault), 0);
    assert_eq!(ctx.token_balance(&ctx.owner_ata), 500);
}

#[test]
fn stranger_cannot_pause() {
    let mut ctx = setup();
    ctx.initialize().unwrap();

    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let err = ctx.set_paused(&stranger, true).unwrap_err();
    assert!(err.contains(ERR_UNAUTHORIZED), "unexpected error: {err}");
    ctx.tip(10).unwrap();

    // Owner's pubkey without its signature is rejected as well.
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &[IX_SET_PAUSED, 1],
        vec![
            AccountMeta::new_readonly(ctx.owner.pubkey(), false),
            AccountMeta::new(ctx.jar, false),
        ],
    );
    let err = ctx.send(ix, &[&stranger]).unwrap_err();
    assert!(err.contains("MissingRequiredSignature"), "unexpected error: {err}");
    ctx.tip(10).unwrap();

    // A stranger cannot unpause either.
    let owner = ctx.owner.insecure_clone();
    ctx.set_paused(&owner, true).unwrap();
    let err = ctx.set_paused(&stranger, false).unwrap_err();
    assert!(err.contains(ERR_UNAUTHORIZED), "unexpected error: {err}");
    let err = ctx.tip(10).unwrap_err();
    assert!(err.contains(ERR_PAUSED), "unexpected error: {err}");
}
