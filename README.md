# `newsatoshi-commons` — Newsatoshi 2.1 Autonomous Abundance Full Stack

Complete Client-Side Validated (CSV) Rust engine, NIST FIPS 205 SLH-DSA + XMSS post-quantum signing layer, real **Winterfell 0.13** STARK prover & verifier, live `newsatoshi-seed` L1 bridge, standalone hardware telemetry signer daemon, formal **RFC-0021** + **SP1 Hypercube / Miden VM** zkVM recursive IVC guest programs, **Live Thermodynamic Telemetry Mirror** web app, and **Parcel #1 Legal-Cryptographic Airlock & Sub-$50k Planning Kit** for **`NEWSATOSHI/2.1` (`LOVE`)**.

- **Live Production Mirror:** `https://commons.newsatoshi.org` (Fly.io fallback: `https://newsatoshi-commons.fly.dev`)
- **Live L1 Seed Reference:** `https://newsatoshi-seed.fly.dev/status` (Testnet) & `https://newsatoshi.org/mainnet/status` (Mainnet, `auxpow_chainid = 20051`)
- **RFC-0021 L1 Soft-Fork Prototype (`0xb4`):** `https://github.com/theguysaccount/newsatoshi-rfc0021` (`feat/rfc-0021-op-verify-covenant-stark`)

> **Core Invariant:** Keep Newsatoshi L1 issuance and SHA-256 merged-mining ordering (`auxpow_chainid = 20051`) **blind and ossified** — never mint base currency directly for physical work (avoiding Budish 2018 rented-hashpower 51% attacks and Sybil wash-farming). Run recursive Commons treasuries as a CSV engine anchored to live L1 blocks today, then activate `OP_VERIFY_COVENANT_STARK` (`0xb4`) via soft fork after the 90-day adversarial testnet & zkVM audit.

> **Zero Fake Data Invariant:** Fresh deployments start at genuine `0 LOVE`, `0` UTXOs, `0` parcels, `0` verified kWh, `0` STARK proofs, and empty telemetry histories. Every balance, parcel acquisition, signed telemetry packet, STARK proof, and L1 anchor receipt reflects a real state transition or live upstream L1 node reading.

---

## Repository Architecture (All 4 Pillars)

```text
newsatoshi-commons/
├── Cargo.toml                         # v0.4.0: slh-dsa 0.2.0-rc.5, winterfell 0.13.1, tiny_http, ureq
├── Dockerfile                         # 3-stage Node 22 + Rust + debian-slim production container (COMMONS_VERIFY_ONLY=1 + dual-chain polling)
├── fly.toml                           # Fly.io deployment config (persistent /data volume + /api/health check)
├── src/
│   ├── crypto.rs                      # Pure SHA-256 Lamport OTS, XMSS Merkle-Lamport (64–4,096 leaves), NIST FIPS 205 SLH-DSA-SHA2-128f
│   ├── lib.rs                         # CommonsLedger, CovenantRule, CheckedWorkProof, PayWord StreamingChannel, UTXO consolidation
│   ├── stark.rs                       # Real Winterfell 0.13 AIR prover & verifier (15-col x 64-row, 63-bit bit-decomp, 99-bit PQ security)
│   ├── storage.rs                     # Atomic fsync+rename JSON persistence + strict --verify-only key isolation (zero private seeds on server)
│   ├── seed.rs                        # Live dual-chain HTTPS poller (testnet + mainnet /status) & live /api/lock?hash= on-chain verifier
│   ├── rpc.rs                         # Deterministic AnchorCommitment bound to live L1 tip + on-chain lock verification metadata
│   ├── hardware.rs                    # HardwareTelemetrySigner + ExternalTelemetryPacket + ExternalDeedPacket (offline XMSS/SLH-DSA)
│   ├── server.rs                      # CORS HTTP API + COMMONS_ADMIN_TOKEN + --verify-only + POST /api/deeds + POST /api/actions/verify-anchor
│   ├── main.rs                        # CLI (`serve`, `simulate`, `telemetry-sign`, `anchor`, `export-legal-schedule`)
│   └── bin/newsatoshi-hw-signer.rs    # Standalone Pi / Linux / embedded hardware telemetry signer daemon (`provision`, `sign`, `daemon`)
├── tests/integration_tests.rs         # 18 integration tests (+1 unit test = 19 total: SLH-DSA, STARKs, verify-only, dual-chain, L1 lock verify)
├── docs/
│   ├── RFC-0021-OP_VERIFY_COVENANT_STARK.md  # Formal NEWSATOSHI/2.1 soft-fork specification & AuxPoW signaling
│   └── AUXPOW-MAINNET-MINING-GUIDE.md        # Live-verified Mainnet & Bitcoin AuxPoW merged-mining pool onboarding guide (chainid 20051)
├── zkvm/
│   ├── covenant-circuit-core/         # Pure-Rust `no_std`-compatible recursive IVC verification kernel (14/14 tests, 0 clippy warnings)
│   ├── sp1-covenant-guest/            # SP1 Hypercube RISC-V guest & host prover/verifier driver
│   ├── miden-covenant/                # Native Goldilocks STARK Miden Assembly (`covenant_transition.masm`)
│   └── testnet-harness/               # 90-day soak & adversarial verification script (`adversarial_suite.sh`)
├── web/                               # Live Thermodynamic Telemetry Mirror & Abundance Cockpit (Vite + React + Tailwind v4)
└── legal/
    ├── perpetual-purpose-trust-charter.md     # Perpetual Purpose Trust Instrument (Section 4.2 Deterministic Cryptographic Execution)
    ├── wyoming-duna-clt-operating-agreement.md # Wyoming DUNA + 99-Year CLT Ground Lease + PayWord Offtake Addendum
    ├── parcel-1-unit-economics.md             # Illustrative sub-$50k ($49,500) Parcel #1 planning worksheet (pre-deployment assumptions)
    ├── parcel-1-model.json                    # Machine-readable exact-Decimal planning model
    ├── render_airlock.py                      # Zero-dep renderer binding live crypto hashes into legal execution drafts
    ├── schedule-generated.json                # Live cryptographic schedule exported by `newsatoshi-commons export-legal-schedule`
    └── rendered/                              # Hash-bound legal drafts + SHA-256 `airlock-manifest.json`
```

---

## Live Newsatoshi L1 Infrastructure Reference

| Network | Endpoint / Port | Genesis Hash | Status |
|---|---|---|---|
| **Testnet** (`auxpow_chainid = 20051`) | `https://newsatoshi-seed.fly.dev/status` (`:18444` P2P) | `00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394` | Live & mining (faucet enabled, height `22,100+`) |
| **Mainnet** (`auxpow_chainid = 20051`) | `newsatoshi-seed.fly.dev:8444` (P2P) | `000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7` | Live genesis (`height = 0`, awaiting external SHA-256 merged-miners) |

---

## Quickstart

### 1. Run All Rust & zkVM Tests
```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Core engine, NIST FIPS 205 SLH-DSA, Winterfell 0.13 STARKs, auth, persistence, HTTP API, hw-signer (17 tests)
cargo test && cargo clippy --all-targets -- -D warnings

# Pure-Rust no_std zkVM circuit verification kernel (14 tests)
(cd zkvm/covenant-circuit-core && cargo test && cargo check --no-default-features)
```

### 2. Launch the Live Thermodynamic Telemetry Mirror + Rust Commons Engine
```bash
# Build frontend bundle
(cd web && npm ci && npm run build)

# Start unified Rust API + live newsatoshi-seed poller + Telemetry Mirror UI
# Set COMMONS_ADMIN_TOKEN (or --admin-token) on public hosts to protect operator write actions
# while keeping reads, STARK verification, and cryptographically signed hardware telemetry public.
COMMONS_ADMIN_TOKEN="replace-with-strong-random-token" \
cargo run --bin newsatoshi-commons -- serve \
  --port 8791 \
  --state-file ./data/commons-ledger.json \
  --seed-url https://newsatoshi-seed.fly.dev/status \
  --static-dir ./web/dist
```
Then open `http://127.0.0.1:8791` in your browser.

### 3. Run the Standalone Hardware Telemetry Signer (`newsatoshi-hw-signer`)
```bash
# Provision persistent device keyfile (XMSS Merkle-Lamport + NIST FIPS 205 SLH-DSA-SHA2-128f)
cargo run --bin newsatoshi-hw-signer -- provision \
  --keyfile ./data/hw-inverter-1.json \
  --device-id pi-solar-parcel-1 \
  --parcel 1 \
  --default-engine-seed --json

# Stream real JSONL meter readings into the running Commons Engine (never synthesizes readings)
printf '{"kwh_delivered":420,"soil_health_score":91,"payout_love":100}\n' | \
  cargo run --bin newsatoshi-hw-signer -- daemon \
    --keyfile ./data/hw-inverter-1.json \
    --endpoint http://127.0.0.1:8791/api/telemetry \
    --scheme slh_dsa \
    --stdin-jsonl
```

### 4. Export Live Cryptographic Schedule & Render Bound Legal Airlock
```bash
cargo run --bin newsatoshi-commons -- export-legal-schedule --out legal/schedule-generated.json
python3 legal/render_airlock.py --schedule legal/schedule-generated.json
```

### 5. Run the 90-Day Adversarial Testnet Verification Harness
```bash
./zkvm/testnet-harness/adversarial_suite.sh --api-url http://127.0.0.1:8791
```

### 6. Production Deployment (Fly.io)
```bash
flyctl apps create newsatoshi-commons --org personal
flyctl volumes create commons_data --region iad --size 1 -a newsatoshi-commons -y
flyctl secrets set COMMONS_ADMIN_TOKEN="$(openssl rand -hex 32)" -a newsatoshi-commons
flyctl deploy -a newsatoshi-commons
```
