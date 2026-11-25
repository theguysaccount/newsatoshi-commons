# NEWSATOSHI/2.1: Tiny-Kernel Machine-Checked Post-Quantum Recursive Covenants, Hash-Only Machine Money, and Budish-Safe Asset Backing

**Authors:** Newsatoshi Commons Protocol Working Group (`commons@newsatoshi.org`)  
**Date:** November 25, 2026  
**Version:** `v0.10.1` — Formal Companion Paper to *A Store of Order* (`newsatoshi.org/white-paper/`) & `RFC-0021`  
**Artifact & Proof Verification:**
- **Lean 4.36.0 Formal Proof (355 lines, 0 `sorry`, 0 external libraries, 0 `Classical.choice`):** [`formal/NewsatoshiCovenants.lean`](https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean)
- **Reference Implementation (MIT Rust + Winterfell 0.13 STARK AIR):** `https://github.com/theguysaccount/newsatoshi-commons`
- **Live Verify-Only Gateway & MCP Server (`org.newsatoshi.commons/newsatoshi-commons`):** `https://commons.newsatoshi.org`
- **Live Universal Ownerless EVM HTLC (`Base 8453 · Arbitrum 42161 · Optimism 10 · Polygon 137`, Sourcify `exact_match`):** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`

---

## Abstract

Bitcoin [Nakamoto 2008] made the **order** of electronic payments a matter of cryptographic proof rather than trust, but left **validity** to unbounded historical re-execution (~760 GB in 2026), relied on elliptic-curve signatures vulnerable to Shor's algorithm, and left recursive spending covenants unresolved. *A Store of Order* (`newsatoshi.org`, Nov 2026) reduces private electronic cash and validity proofs to a single hash function (`SHA-256`) merge-mined with Bitcoin (`AuxPoW chainid = 20051`).

Yet two 2026 watersheds redefined the bar for cryptographic money: (1) the June 2026 discovery of a four-year-old under-constrained circuit flaw in Zcash's Orchard pool established **machine-checked formal soundness proofs** as mandatory for private money; and (2) the November 2026 release of a 17-million-line AI-assisted Lean 4 proof of the Generalized Riemann Hypothesis—and the ensuing audit by independent mathematicians and frontier reasoning models (e.g., Gemini 4 Neon)—exposed a deeper principle: **in an era where AI can synthesize million-line formalizations, trustworthiness requires a *tiny, human-auditable formal kernel* whose definitions cannot conceal subtle tautologies or field-aliasing bugs.**

We specify and formally verify **`NEWSATOSHI/2.1`**, a backwards-compatible soft-fork extension (`OP_VERIFY_COVENANT_STARK`, `0xb4` redefining `OP_NOP5`) and multi-chain machine-money architecture. We present:
1. A **355-line, zero-dependency, zero-`sorry` Lean 4.36.0 kernel** verifying nine soundness theorems directly matching our executing Winterfell 0.13 STARK AIR (`15` columns × `64` rows over $\mathbb{F}_p$, $p = 2^{128} - 45 \cdot 2^{40} + 1$), including injective 3-limb SHA-256 packing, zero modular wrap-around, exact integer value conservation, exact Euclidean basis-point floor uniqueness, unbounded inductive covenant continuity, monotonic Web4 child-agent spending bounds, and cross-chain HTLC anti-race safety.
2. A **Budish rented-hashpower escape proposition** proving why coupling L1 block issuance to physical-world oracles fails economically, whereas isolating real-world asset backing and autonomous agent treasuries strictly inside pre-mined covenanted UTXOs preserves zero L1 systemic contagion.
3. **Three open cryptographic conjectures** and reproducible verification test vectors inviting cryptographers, Bitcoin protocol researchers, and automated theorem provers to audit, tighten, or falsify our bounds.

---

## 1. Introduction: From Proof of Order to Tiny-Kernel Verified Money

### 1.1 The Two 2026 Inflections in Formal Verification
In November 2026, artificial intelligence crossed a historic boundary in formal mathematics: frontier models collaborated with mathematicians to synthesize a 17-million-line Lean 4 archive claiming the Generalized Riemann Hypothesis, while independent researchers and frontier reasoning systems (including Google's Gemini 4 Neon) raced to inspect whether hidden definitional circularities or subtle mismatches invalidated parts of the massive codebase.

That debate illuminates a profound lesson for monetary cryptography: **machine-checked compilation is necessary, but opacity is fatal.** In June 2026, private cryptocurrency infrastructure experienced its own reckoning when an under-constrained circuit bug in Zcash's Orchard pool demonstrated that unverified arithmetic circuits can permit silent counterfeiting for years. As the foundational Newsatoshi whitepaper (*A Store of Order*, §12) notes, private electronic cash now requires a machine-checked proof that balance and constraint invariants hold.

If AI can now assist in discovering deep mathematical proofs, it can also construct—and adversarially audit—the next evolution of hard electronic money. But monetary consensus cannot hide behind 17 million lines of opaque Lean or complex trusted-setup SNARK circuits. Every consensus-critical algebraic constraint must fit inside a **Tiny Verifier Kernel**: small enough that any independent cryptographer, Bitcoin Core developer, or automated reasoning model can read every single line of both the formal specification (`formal/NewsatoshiCovenants.lean`, `355` lines) and the executing AIR (`src/stark.rs`, `~480` core lines) in one sitting and verify it locally in ~1 second using stock `lean`.

### 1.2 Solving Bitcoin's Recursive Covenant Deadlock & Web4's Safety Flaw Simultaneously
Bitcoin's scripting layer has debated recursive covenants (`OP_CTV`, `OP_CAT`, `OP_TXHASH`, `OP_CSFS`) for a decade because passive UTXOs cannot enforce *how money behaves after it is spent*. At the same time, the viral 2026 **Web4 / Automaton** movement gave autonomous AI agents crypto wallets and HTTP `402` billing—drawing sharp criticism from Ethereum's Vitalik Buterin because text-file constitutions (`constitution.md`) fail under prompt injection, agents hold freezable fiat stablecoins without owning hard productive assets, and self-replicating agents lack hard mathematical spending bounds.

`NEWSATOSHI/2.1` resolves both problems with one minimal primitive: **`OP_VERIFY_COVENANT_STARK` (`0xb4`)** paired with **pure-SHA-256 PayWord micropayments** and **vaultless SHA-256 cross-chain HTLCs**.

---

## 2. Protocol Specification & Executing Winterfell 0.13 STARK AIR

### 2.1 Shielded Note Covenant Commitment Extension (`RFC-0021`)
Extending `newsatoshi v0.1.0`'s SHA-256 shielded note commitment, `NEWSATOSHI/2.1` binds a 32-byte `covenant_vk_hash`:

$$\text{cm} = \text{SHA256}\big(\texttt{"NEWSATOSHI\_NOTE\_CM\_V2\_1"} \parallel \text{be64}(v) \parallel \text{owner\_pk} \parallel \text{covenant\_vk\_hash} \parallel \text{salt}\big)$$

- When $\text{covenant\_vk\_hash} = 0^{256}$, the note is an ordinary private `LOVE` note with zero covenant overhead.
- When $\text{covenant\_vk\_hash} \neq 0^{256}$, spending the note invokes `OP_VERIFY_COVENANT_STARK` (`0xb4`, redefining `OP_NOP5` as a strict soft fork) and enforces exact equality of `covenant_vk_hash` on the change output commitment.

### 2.2 Exact Executing Algebraic Intermediate Representation (`src/stark.rs`)
`CovenantTransitionAir` operates over Winterfell 0.13's 128-bit prime field:

$$p = 2^{128} - 45 \cdot 2^{40} + 1 = 340282366920938463463374557953744961537$$

with trace width $w = 15$ columns, trace length $N = 64$ rows ($63$ transitions), Blake3-256 Merkle commitments, blowup factor $B = 8$, `32` FRI queries, and `4`-bit grinding ($\ge 96$–$100$ bits conjectured transparent PQ security; proof size `~22 KiB`, verification `< 10 ms`):

1. **Columns $c_0, c_1, c_2$ (Immutable Covenant Rule Digest):** Carry the 32-byte `covenant_rule_hash` packed injectively into $(88, 88, 80)$-bit limbs. Transitions: $c_i' - c_i = 0$ ($i \in \{0,1,2\}$). Boundary assertions pin row $0$ to `rule_in` and row $63$ to `rule_out`.
2. **Columns $c_3, c_4, c_5$ (Conserved Amounts):** Constant columns for $(\text{input}, \text{payout}, \text{change})$ with degree-1 conservation constraint:
   $$c_3 - c_4 - c_5 = 0$$
3. **Columns $(c_6, c_7), (c_8, c_9), (c_{10}, c_{11}), (c_{12}, c_{13})$ (Four 63-Bit Non-Negativity Bit-Decomposition Pairs):** For each pair $(b, a)$, transitions enforce:
   $$b \cdot (b - 1) = 0, \qquad a' - (2a + b) = 0$$
   with boundary assertions $a_0 = 0$ and $a_{63}$ pinned to:
   - `TelemetryBounty`: $s_1 = \text{max\_bounty} - \text{payout}$, $s_2 = \text{soil} - \text{min\_soil}$, $s_3 = \text{change}$, $s_4 = \text{payout}$.
   - `LandDeed`: $s_1 = \text{input} - \text{payout}$, $s_2 = \text{change}$, $s_3 = \text{change}$, $s_4 = \text{payout}$.
   - `StreamingSettlement`: $s_1 = \text{input} \cdot \text{bps} - 10000 \cdot \text{change}$, $s_2 = 9999 - s_1$, $s_3 = \text{change}$, $s_4 = \text{payout}$.
4. **Column $c_{14}$ (Degree-3 State-Chain Mixing):** $c_{14}' - (c_{14}^3 + c_0 + c_4 + 1) = 0$ anchored at row $0$ to `prev_state_root[0]`.

---

## 3. Machine-Checked Soundness Theorems (Lean 4.36.0 Kernel)

All nine theorems below are formally proven with **zero `sorry`**, **zero Mathlib dependencies**, and **zero `Classical.choice`** in [`formal/NewsatoshiCovenants.lean`](https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean) (`355` lines; verified in `1.00s` CPU time under `lean 4.36.0`).

| # | Theorem (`formal/NewsatoshiCovenants.lean`) | Formal Statement Proven in Lean 4 Kernel | Axioms Used (`#print axioms`) |
|---|---|---|---|
| **T1** | `digest_pack_injective` | Packing 256-bit SHA-256 digests into $(88, 88, 80)$-bit limbs modulo $p = 2^{128} - 45 \cdot 2^{40} + 1$ is strictly injective: $\text{pack}(a) = \text{pack}(b) \implies a = b$. Zero hash-aliasing counterfeit paths. | `[propext]` |
| **T2** | `prime_boolean_constraint_sound` | Over any prime field $\mathbb{F}_p$, for canonical $b < p$, $p \mid b(b-1) \implies b \in \{0, 1\}$. | `[propext, Quot.sound]` |
| **T3** | `air_63bit_range_sound` | By induction on $k \in [0, 63]$, $\text{acc}_0 = 0 \land \text{acc}_{k+1} = 2\text{acc}_k + b_k \land b_k \in \{0,1\} \implies \text{acc}_{63} \le 2^{63}-1 < p$. Eliminates negative-slack wrap-around (`p - 1`) attacks. | `[propext, Quot.sound]` |
| **T4** | `value_conservation_no_wrap` | For $\text{input}, \text{payout}, \text{change} \le 2^{63}-1$, $\text{input} \equiv \text{payout} + \text{change} \pmod p \implies \text{payout} + \text{change} = \text{input}$ in $\mathbb{N}$. Zero silent inflation. | `[propext, Quot.sound]` |
| **T5** | `streaming_bps_floor_unique` | $\text{change} \cdot 10000 + r = \text{input} \cdot \text{bps} \land r < 10000 \implies \text{change} = \lfloor (\text{input}\cdot\text{bps})/10000 \rfloor \land r = (\text{input}\cdot\text{bps}) \bmod 10000$. | `[propext, Quot.sound]` |
| **T6** | `air_streaming_modular_soundness` | Directly from the exact $\mathbb{F}_p$ modular equations of `StreamingSettlement` with 63-bit bounds, proves zero wrap-around and unique exact integer floor division for revenue/backing splits. | `[propext, Quot.sound]` |
| **T7** | `recursive_covenant_continuity_and_bound` | By structural induction over arbitrary spend depth $T \ge 0$, every valid covenant chain preserves $\text{ruleHash}_T = \text{ruleHash}_0$, conserves $\text{extracted} + \text{bal}_T = \text{bal}_0$, and bounds $\text{extracted} \le T \cdot \text{maxBounty}$. | `[propext, Quot.sound]` |
| **T8** | `web4_monotonic_child_bound` | Across arbitrary Web4 parent→child agent replication depth $d$, monotonic covenant inheritance preserves $\text{constitutionHash}_d = \text{constitutionHash}_0$, $\text{maxSpend}_d \le \text{maxSpend}_0$, and $\text{backingBps}_d \ge \text{backingBps}_0$. | **None (0 axioms!)** |
| **T9** | `htlc_anti_race_safety` | If $t_{\text{first}} \ge t_{\text{second}} + 120$ and a counterparty claims leg 2 at $t_{\text{claim}} < t_{\text{second}}$, then $t_{\text{claim}} + 120 < t_{\text{first}}$, guaranteeing the preimage revelation window strictly dominates refund races. | `[propext, Quot.sound]` |

---

## 4. Economic Security: Budish Rented-Hashpower Escape & Proof-of-Backing Isolation

### Proposition 10 (L1 Blindness & Contagion Isolation)
Let $C_{\text{rent}}(z)$ denote the spot cost of renting $>50\%$ SHA-256 hashpower for $z$ blocks to execute a majority reorg [Budish 2018], and let $V_{\text{oracle}}$ denote the value of base-layer currency minted in response to external physical/oracle claims (e.g., "Proof of Useful Work" or "Proof of Solar").

1. **Oracle-Coupled Minting Instability:** Whenever block subsidies depend on oracle-verified physical claims rather than blind work, an adversary controlling compromised sensor hardware (zero marginal energy cost via closed-loop wash-farming) or renting spot hashpower can extract unbounded $V_{\text{oracle}} > C_{\text{rent}}(z)$, destroying monetary neutrality for all honest holders.
2. **Strict Layer Isolation in `NEWSATOSHI/2.1`:** Newsatoshi L1 issuance remains 100% blind and ossified (`50 LOVE/block`, halving every `210,000` blocks to a `0.1953125 LOVE` floor, AuxPoW `chainid = 20051` sharing Bitcoin's existing SHA-256 work). Real-world asset backing, hardware telemetry (`SLH-DSA`/`XMSS`), and Web4 business covenants operate **strictly over pre-mined `LOVE` UTXOs and immutable multi-chain reserve vaults**. Consequently, even total compromise of a specific physical sensor or business oracle has **zero blast radius** on L1 issuance or unrelated holders, while honest businesses mathematically route `backing_bps` of verified revenue into permanently auditable backing reserves.

---

## 5. Cross-Chain Vaultless Gravity & Sub-Millisecond Machine Commerce

All external buy roads enforce the identical `SHA256(32-byte preimage) == payment_hash` atomic invariant verified natively by `newsatoshi v0.1.0` (`lock`/`unlock`)—eliminating custodians, multisig bridges, and wrapped IOUs:

- **Live Universal Ownerless EVM HTLC (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`):** Deployed identically at nonce `0` with `0` constructor arguments and `0` admin functions using EVM precompile `0x02` (`sha256`), verified `exact_match` (creation + runtime) on Sourcify and proven with live atomic `lockEth` → `claim` transactions across **4 production mainnets**:
  - **Base (`8453`):** Claim TX `0x0f6cbfd277610b112d79fe36138dd549931f45f4ffa2894f7ea9e5831a15f595`
  - **Arbitrum One (`42161`):** Claim TX `0xd11efcb1750aae5d5d443f4d446b84b609f6d253ca5152ebf1ea279a13de4d6e`
  - **Optimism (`10`):** Claim TX `0xdb4ee991b86febd16a60c6a8a29a6ed4ac2d0dd92aebbc6a835f1d9c195881b8`
  - **Polygon PoS (`137`):** Claim TX `0x868f347586d34f4c09bb6aae2b6b9c0f0ec657ab5f2dbfa87b22197aa8f94fb4`
- **Bitcoin L1 & Lightning:** Canonical BIP-199 SegWit v0 P2WSH (`bc1q…`) `OP_SHA256` HTLC compiler + native BOLT-11 payment-hash decoder (`src/swap.rs`).
- **Solana (`bridges/solana/`):** Deterministic `sol_sha256` PDA escrow (`["ns_htlc", hash, sender]`), unit-tested and ready for mainnet-beta deployment.
- **HTTP `402` PayWord Micropayments:** Pure-SHA-256 hash chains ($H^k(\text{token}) = \text{tip}$) verified in `< 1 ms` with `0` gas per call—amortizing up to $10^6$ AI agent API calls per L1 hash-lock.

---

## 6. Three Open Cryptographic Conjectures & Community Audit Challenges

We explicitly publish three open research problems and invite cryptographers, Bitcoin developers, and automated theorem provers (Lean FRO, Gemini 4 Neon, Claude) to attack, refine, or refute them:

### Conjecture 1 — Minimum-Width Transparent AIR for Sub-10ms MCU Recursive Covenants
> *For a transparent FRI-based STARK over a prime field $\mathbb{F}_p$ achieving $\ge 128$-bit post-quantum soundness (no elliptic curves, no trusted setup) that simultaneously enforces 63-bit non-negativity range bounds, value conservation, and recursive verification-key hash continuity (`cm_out.vk = cm_in.vk`), what is the minimal trace area $A = w \cdot N$ and maximum constraint degree $d$ permitting full verification in $< 10\text{ ms}$ on a 240 MHz microcontroller (ESP32-S3 / ARM Cortex-M7) with $\le 512\text{ KiB}$ SRAM?*

Our current Winterfell 0.13 `CovenantTransitionAir` achieves $(w=15, N=64, d=3)$ at `~96–100` bits conjectured security (`~22 KiB` proof). Tightening to full proven `128`-bit proximity-gap bounds while folding an in-circuit SHA-256 commitment check within 512 KiB SRAM remains an open optimization frontier.

### Conjecture 2 — Hash-Only Sublinear Multi-Merchant PayWord Aggregation
> *Does there exist a stateless, pure-hash (random-oracle / SHA-256 only, zero pairings, zero lattices) PayWord aggregation scheme allowing an autonomous Web4 agent to open one L1 hash-lock and stream authenticated micropayments to $M$ dynamically discovered merchants across $N$ calls with $O(\log(MN))$ on-chain settlement verification cost and zero interactive setup between merchants?*

Standard Rivest–Shamir PayWords achieve $O(1)$ per-merchant settlement for a single linear chain ($M=1$), while Merkle-tree PayWords require pre-committing leaf budgets. Removing pre-committed per-merchant caps under pure SHA-256 assumptions is an open primitive for machine commerce.

### Conjecture 3 — The Tiny-Kernel Auditability Principle for Monetary Consensus
> *Let $\mathcal{K}(\Pi) = (\text{LoC}_{\text{formal}}, \text{LoC}_{\text{AIR}}, |\mathcal{A}_{\text{ext}}|)$ denote the specification size, executing circuit size, and external axiom/library count of a consensus validity circuit $\Pi$. We conjecture that any consensus proof system whose formal specification requires external opaque libraries or exceeds human-auditable scale ($\text{LoC}_{\text{formal}} \gg 10^4$) inherits an unacceptable **specification-mismatch attack surface**—even if kernel compilation succeeds—because definitional circularities or witness-binding gaps (as exposed in the 2026 Orchard and 17M-line GRH debates) escape independent review.*

We propose $\text{LoC}_{\text{formal}} < 10^3$ with zero external library axioms (as demonstrated by `formal/NewsatoshiCovenants.lean` at `355` lines) as the target engineering standard for Layer-1 monetary soundness kernels.

---

## 7. Independent 60-Second Verification

Every claim in this paper is executable and verifiable without trusting any server:

```bash
# 1. Machine-verify all 9 Lean 4 theorems directly in Lean's kernel (< 2 seconds, zero Mathlib)
curl -fsSL https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean -o NewsatoshiCovenants.lean
lean NewsatoshiCovenants.lean

# 2. Clone & run all 31 Rust tests (NIST FIPS 205 SLH-DSA, Winterfell 0.13 STARKs, BIP-199/BOLT-11/EVM swaps)
git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons && cargo test && cargo clippy --all-targets -- -D warnings

# 3. Verify live 4-chain Sourcify exact_match EVM HTLCs & Official MCP Registry listing
curl -fsS https://sourcify.dev/server/v2/contract/8453/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987
curl -fsS "https://registry.modelcontextprotocol.io/v0/servers?search=newsatoshi"
```

---

## References

1. S. Nakamoto, *"Bitcoin: A Peer-to-Peer Electronic Cash System,"* 2008.
2. Newsatoshi, *"A Store of Order: Electronic cash everyone can verify,"* `https://newsatoshi.org/white-paper/`, Nov 2026.
3. E. Budish, *"The Economic Limits of Bitcoin and the Blockchain,"* NBER Working Paper 24717, 2018.
4. E. Ben-Sasson, I. Bentov, Y. Horesh, M. Riabzev, *"Scalable, transparent, and post-quantum secure computational integrity (STARKs),"* IACR ePrint 2018/046.
5. NIST FIPS 205, *"Stateless Hash-Based Digital Signature Standard (SLH-DSA),"* 2024.
6. R. L. Rivest and A. Shamir, *"PayWord and MicroMint: Two Simple Micropayment Schemes,"* Security Protocols Workshop, 1996.
7. Newsatoshi Commons Working Group, *"RFC-0021: OP_VERIFY_COVENANT_STARK (0xb4) & Recursive Shielded Covenants,"* `docs/RFC-0021-OP_VERIFY_COVENANT_STARK.md`, 2026.
