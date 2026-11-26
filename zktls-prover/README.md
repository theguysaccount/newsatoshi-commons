# Newsatoshi zkTLS / MPC-TLS Prover & Notary Adapter Kit

Cryptographically proves real HTTPS / TLS 1.3 responses (Wise, Revolut, Stripe, GitHub, bank transfers, API receipts) with **selective disclosure** (redacting auth cookies/tokens while opening exact transfer fields) and verifies them directly against `POST https://commons.newsatoshi.org/api/zktls/verify` or MCP tool `newsatoshi_verify_zktls_proof` to produce canonical L1 `RFC-0024` `BondAttest` receipt digests.

## Supported Real Cryptographic Protocols

1. **`reclaim_v1` (Reclaim Protocol):** Exact `getIdentifierFromClaimInfo` (`keccak256`) + `createSignDataForClaim` + EIP-191 personal_sign + real `k256` secp256k1 `ecrecover` against trusted Reclaim attestor Ethereum addresses.
2. **`tlsnotary_v1` (TLSNotary / PSE MPC-TLS):** Real `Ed25519` or `secp256k1` notary signature over canonical `SessionHeader` (`server_name`, TLS 1.3 version, cipher suite, leaf X.509 SHA-256 fingerprint, transcript Merkle root) + salted SHA-256 selective-disclosure Merkle opening proofs (`nsTLSL` / `nsTLSN`).
3. **`newsatoshi_pq_notary_v1` (Post-Quantum NIST FIPS 205 Notary):** Same selective-disclosure Merkle transcript proof signed with `SLH-DSA-SHA2-128f`.

## Quickstart

```bash
# 1. Local selective-disclosure proof self-test (redacts auth header, opens JSON payload)
node zktls-prover/notary-prover.mjs --self-test

# 2. Notarize a live TLS 1.3 HTTPS endpoint and verify it against commons.newsatoshi.org
node zktls-prover/notary-prover.mjs https://commons.newsatoshi.org/api/health \
  | curl -sS -X POST https://commons.newsatoshi.org/api/zktls/verify \
      -H 'Content-Type: application/json' --data-binary @-
```

## Production MPC-TLS Notary Deployment Note

For trust-minimized third-party P2P banking ramps, run the **Prover** on the buyer's device and the **Notary** on an independent bonded operator (or connect upstream [TLSNotary (`tlsn`)](https://tlsnotary.org) / [Reclaim Protocol](https://reclaimprotocol.org) attestor networks) so the notary signs the `SessionHeader` without ever seeing decrypted bank credentials.
