// Devnet end-to-end flow against the deployed program.
// Usage: NODE_PATH=<dir containing @solana/web3.js> node scripts/devnet-e2e.mjs <payer.json> <programId>
// The payer should be a throwaway devnet key. All other roles are generated in memory.
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const require = createRequire(import.meta.url);
const {
  Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction,
  sendAndConfirmTransaction,
} = require("@solana/web3.js");

const [payerPath, programIdStr] = process.argv.slice(2);
const conn = new Connection("https://api.devnet.solana.com", "confirmed");
const payer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(payerPath, "utf8"))));
const PID = new PublicKey(programIdStr);
const TOKEN = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYS = SystemProgram.programId;
const sigs = [];

const u8 = (n) => Buffer.from([n]);
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const i32 = (n) => { const b = Buffer.alloc(4); b.writeInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const i64 = (n) => { const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(n)); return b; };
const sha = (...p) => createHash("sha256").update(Buffer.concat(p)).digest();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function send(label, ixs, signers) {
  const tx = new Transaction().add(...ixs);
  const sig = await sendAndConfirmTransaction(conn, tx, signers, { commitment: "confirmed" });
  sigs.push([label, sig]);
  console.log(`${label}: ${sig}`);
  return sig;
}
async function chainTime() {
  const slot = await conn.getSlot("confirmed");
  return (await conn.getBlockTime(slot)) ?? Math.floor(Date.now() / 1000);
}
const meta = (pubkey, isSigner, isWritable) => ({ pubkey, isSigner, isWritable });
const ix = (keys, data) => new TransactionInstruction({ programId: PID, keys, data: Buffer.concat(data) });

async function newTokenAccount(mint, owner) {
  const acc = Keypair.generate();
  const lamports = await conn.getMinimumBalanceForRentExemption(165);
  await send(`token account for ${owner.toBase58().slice(0, 6)}`, [
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: acc.publicKey, lamports, space: 165, programId: TOKEN }),
    new TransactionInstruction({ programId: TOKEN, keys: [meta(acc.publicKey, false, true), meta(mint, false, false)], data: Buffer.concat([u8(18), owner.toBuffer()]) }),
  ], [payer, acc]);
  return acc.publicKey;
}

const operator = Keypair.generate(), oracle = Keypair.generate(), auditor = Keypair.generate();
const farmers = [Keypair.generate(), Keypair.generate()];

// 1. test mint (6 decimals), sponsor token account, fund farmers for fees + receipt rent
const mintKp = Keypair.generate();
const mintRent = await conn.getMinimumBalanceForRentExemption(82);
await send("create mint", [
  SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: mintKp.publicKey, lamports: mintRent, space: 82, programId: TOKEN }),
  new TransactionInstruction({ programId: TOKEN, keys: [meta(mintKp.publicKey, false, true)], data: Buffer.concat([u8(20), u8(6), payer.publicKey.toBuffer(), u8(0)]) }),
], [payer, mintKp]);
const mint = mintKp.publicKey;
const sponsorTok = await newTokenAccount(mint, payer.publicKey);
await send("mint 10 test tokens to sponsor", [
  new TransactionInstruction({ programId: TOKEN, keys: [meta(mint, false, true), meta(sponsorTok, false, true), meta(payer.publicKey, true, false)], data: Buffer.concat([u8(7), u64(10_000_000)]) }),
], [payer]);
const farmerToks = [];
for (const f of farmers) farmerToks.push(await newTokenAccount(mint, f.publicKey));
await send("fund farmer + role fee lamports", [...farmers, operator, oracle, auditor].map((k) =>
  SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: k.publicKey, lamports: 5_000_000 })), [payer]);

// 2. CreateCampaign
const campaignId = BigInt(Date.now());
const [campaign] = PublicKey.findProgramAddressSync([Buffer.from("camp"), payer.publicKey.toBuffer(), u64(campaignId)], PID);
const [vault] = PublicKey.findProgramAddressSync([Buffer.from("vault"), campaign.toBuffer()], PID);
const now = await chainTime();
const freeze = now + 40, windowEnd = now + 70;
const FULL = 2_000_000, HALF = 1_000_000;
await send("CreateCampaign", [ix([
  meta(payer.publicKey, true, true), meta(campaign, false, true), meta(vault, false, true), meta(mint, false, false),
  meta(sponsorTok, false, true), meta(SYS, false, false), meta(TOKEN, false, false),
], [u8(0), u64(campaignId), operator.publicKey.toBuffer(), auditor.publicKey.toBuffer(), oracle.publicKey.toBuffer(),
  i64(freeze), i64(windowEnd), u64(FULL), u64(HALF), i32(800), i32(1200), sha(Buffer.from("terms-v1")), u64(2 * FULL + 500_000), u8(0)])], [payer]);

// 3. LockRoster (2 farmers)
const leaves = farmers.map((f, i) => sha(Buffer.from("NH1"), campaign.toBuffer(), f.publicKey.toBuffer(), u32(i)));
const [l, r] = Buffer.compare(leaves[0], leaves[1]) <= 0 ? [leaves[0], leaves[1]] : [leaves[1], leaves[0]];
const root = sha(Buffer.from("NHN"), l, r);
const proofs = [leaves[1], leaves[0]];
await send("LockRoster", [ix([meta(operator.publicKey, true, true), meta(campaign, false, true), meta(vault, false, false)], [u8(1), u32(2), root])], [operator]);

// 4. wait for window end, Settle with observed below p10 floor (operator + oracle)
while ((await chainTime()) < windowEnd + 2) await sleep(4000);
await send("Settle (observed 55.0mm < floor)", [ix([meta(operator.publicKey, true, true), meta(oracle.publicKey, true, false), meta(campaign, false, true)], [u8(2), i32(550), sha(Buffer.from("data.json"))])], [operator, oracle]);

// 5. The designed zero-wait path past the 48h window: auditor disputes, then
//    operator + oracle + auditor re-settle -> SETTLED_FINAL -> Release immediately.
await send("Dispute (auditor)", [ix([meta(auditor.publicKey, true, true), meta(campaign, false, true)], [u8(3)])], [auditor]);
await send("Settle final (operator+oracle+auditor)", [ix([meta(operator.publicKey, true, true), meta(oracle.publicKey, true, false), meta(campaign, false, true), meta(auditor.publicKey, true, false)], [u8(2), i32(550), sha(Buffer.from("data.json"))])], [operator, oracle, auditor]);
await send("Release (anyone)", [ix([meta(campaign, false, true), meta(vault, false, true), meta(sponsorTok, false, true), meta(payer.publicKey, false, true), meta(TOKEN, false, false)], [u8(4)])], [payer]);

// 6. Claims
const claimIx = (i) => {
  const [receipt] = PublicKey.findProgramAddressSync([Buffer.from("claim"), campaign.toBuffer(), u32(i)], PID);
  return ix([meta(farmers[i].publicKey, true, true), meta(campaign, false, false), meta(vault, false, true), meta(farmerToks[i], false, true),
    meta(receipt, false, true), meta(SYS, false, false), meta(TOKEN, false, false)], [u8(6), u32(i), proofs[i]]);
};
for (const i of [0, 1]) await send(`Claim farmer ${i}`, [claimIx(i)], [farmers[i]]);
let doubleClaim = "unexpectedly succeeded";
try {
  const tx = new Transaction().add(claimIx(0), SystemProgram.transfer({ fromPubkey: farmers[0].publicKey, toPubkey: farmers[0].publicKey, lamports: 1 }));
  await sendAndConfirmTransaction(conn, tx, [farmers[0]]);
} catch (e) { doubleClaim = `rejected: ${String(e.message).match(/custom program error: 0x[0-9a-f]+/)?.[0] ?? e.message.slice(0, 120)}`; }
console.log(`Double claim: ${doubleClaim}`);

const bal = async (k) => (await conn.getTokenAccountBalance(k)).value.amount;
console.log(JSON.stringify({ programId: PID.toBase58(), campaign: campaign.toBase58(), vault: vault.toBase58(),
  farmer0: await bal(farmerToks[0]), farmer1: await bal(farmerToks[1]), sponsor: await bal(sponsorTok), vaultLeft: await bal(vault), doubleClaim, sigs }, null, 2));
