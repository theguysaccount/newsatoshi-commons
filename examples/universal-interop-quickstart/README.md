# Universal Interoperability Quickstart (Zero Dependencies — JS & Python)

Run an end-to-end live check against `https://commons.newsatoshi.org` covering:
1. Live 6-chain parallel RPC matrix (Ethereum L1, Base, Optimism, Arbitrum, Polygon, Newsatoshi Mainnet v2)
2. Canonical **ERC-7683** `GaslessCrossChainOrder` + Keccak-256 EIP-712 signing digest
3. **11-Rail Universal SHA-256 HTLC** (`sui_move` ↔ `cosmos_ibc` ↔ `newsatoshi_l1`)
4. **ISO 20022 `pacs.008.001.08`** Bank Wire XML ↔ RFC-0028 Branded Stablecoin (`BANKUSD`)
5. **4-Domain Winterfell 0.13 STARK** Interop Proof (`ok: true`, zero live state mutation)

```bash
node examples/universal-interop-quickstart/demo.mjs
python3 examples/universal-interop-quickstart/demo.py
```
