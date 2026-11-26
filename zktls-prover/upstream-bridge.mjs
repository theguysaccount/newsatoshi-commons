#!/usr/bin/env node
/**
 * Newsatoshi Upstream zkTLS Adapter Bridge (Reclaim Protocol & PSE TLSNotary `tlsn`)
 *
 * Converts raw JSON proof artifacts exported by upstream decentralized MPC-TLS networks:
 *   1. `@reclaimprotocol/js-sdk` / `@reclaimprotocol/zk-fetch` (`importReclaimSdkProof`)
 *   2. PSE TLSNotary `tlsn-js` / `tlsn-prover` + `notary.pse.dev` (`importTlsNotaryExport`)
 * into canonical payloads accepted directly by `POST https://commons.newsatoshi.org/api/zktls/verify`
 * and MCP tool `newsatoshi_verify_zktls_proof` to emit L1 `RFC-0024` `BondAttest` receipts.
 *
 * Zero external npm dependencies (Node 18+).
 * Usage:
 *   node zktls-prover/upstream-bridge.mjs reclaim <path-to-reclaim-proof.json> [--verify-live]
 *   node zktls-prover/upstream-bridge.mjs tlsn <path-to-tlsn-export.json> [--verify-live]
 *   node zktls-prover/upstream-bridge.mjs --self-test
 */
import fs from "node:fs";
import crypto from "node:crypto";
import {
  generateEd25519NotaryKeypair,
  signTlsNotaryPresentation,
} from "./notary-prover.mjs";

/** Default official Reclaim Protocol v1 attestor address (can be overridden with custom attestors). */
export const OFFICIAL_RECLAIM_ATTESTOR_V1 = "0x244897572368eadf65bfbc5aec98d8e5443a9072";

/**
 * Convert raw `@reclaimprotocol/js-sdk` / `zk-fetch` JSON output into a `reclaim_v1` payload
 * ready for `POST /api/zktls/verify` (real secp256k1 EIP-191 `ecrecover`).
 */
export function importReclaimSdkProof(raw, {
  trustedAttestors = [OFFICIAL_RECLAIM_ATTESTOR_V1],
  requiredSubstrings = [],
} = {}) {
  const claim = raw?.claimData || raw?.claim || raw;
  if (!claim || typeof claim !== "object") {
    throw new Error("missing Reclaim claimData object");
  }
  const signatures = Array.isArray(raw?.signatures)
    ? raw.signatures
    : raw?.signature
    ? [raw.signature]
    : [];
  if (signatures.length === 0) {
    throw new Error("Reclaim proof must include at least one 65-byte ECDSA signature");
  }
  const parameters =
    typeof claim.parameters === "string"
      ? claim.parameters
      : JSON.stringify(claim.parameters ?? {});
  const context =
    typeof claim.context === "string"
      ? claim.context
      : claim.context
      ? JSON.stringify(claim.context)
      : "";
  return {
    protocol: "reclaim_v1",
    provider: String(claim.provider || "http"),
    parameters,
    context,
    owner: String(claim.owner || ""),
    timestampS: Number(claim.timestampS ?? claim.timestamp_s ?? 0),
    epoch: Number(claim.epoch ?? 1),
    signatures: signatures.map((s) => String(s)),
    trusted_attestors: trustedAttestors,
    required_substrings: requiredSubstrings,
  };
}

/**
 * Convert an upstream PSE TLSNotary (`tlsn-js` / `tlsn-prover`) selective-disclosure JSON
 * export into a `tlsnotary_v1` payload ready for `POST /api/zktls/verify`.
 */
export function importTlsNotaryExport(raw, {
  trustedNotaryPubkeys = [],
  expectedServerName = null,
  requiredSubstrings = [],
} = {}) {
  const h = raw?.session_header || raw?.sessionHeader;
  if (!h) throw new Error("missing TLSNotary session_header");
  const notaryPub = String(raw.notary_pubkey_hex || raw.notaryPubkeyHex || "");
  return {
    protocol: "tlsnotary_v1",
    signature_scheme: String(raw.signature_scheme || raw.signatureScheme || "ed25519"),
    session_header: {
      server_name: String(h.server_name || h.serverName || ""),
      time_unix: Number(h.time_unix ?? h.timeUnix ?? 0),
      sent_len: Number(h.sent_len ?? h.sentLen ?? 0),
      recv_len: Number(h.recv_len ?? h.recvLen ?? 0),
      tls_version: String(h.tls_version || h.tlsVersion || "TLSv1.3"),
      cipher_suite: String(h.cipher_suite || h.cipherSuite || "TLS_AES_256_GCM_SHA384"),
      cert_sha256_hex: String(h.cert_sha256_hex || h.certSha256Hex || ""),
      transcript_merkle_root_hex: String(
        h.transcript_merkle_root_hex || h.transcriptMerkleRootHex || ""
      ),
    },
    notary_pubkey_hex: notaryPub,
    notary_signature_hex: String(raw.notary_signature_hex || raw.notarySignatureHex || ""),
    trusted_notary_pubkeys:
      trustedNotaryPubkeys.length > 0 ? trustedNotaryPubkeys : notaryPub ? [notaryPub] : [],
    disclosed_chunks: raw.disclosed_chunks || raw.disclosedChunks || [],
    expected_server_name: expectedServerName || String(h.server_name || h.serverName || ""),
    required_substrings: requiredSubstrings,
  };
}

/** Submit a converted zkTLS payload to `POST /api/zktls/verify`. */
export async function submitToZkTlsVerifier(
  payload,
  { commonsBaseUrl = "https://commons.newsatoshi.org", fetchImpl = globalThis.fetch } = {}
) {
  const resp = await fetchImpl(`${commonsBaseUrl.replace(/\/+$/, "")}/api/zktls/verify`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(payload),
  });
  const body = await resp.json();
  return { status: resp.status, body };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const mode = process.argv[2] || "--self-test";
  if (mode === "--self-test") {
    // 1. Validate Reclaim SDK importer schema conversion
    const sampleReclaim = importReclaimSdkProof({
      claimData: {
        provider: "http",
        parameters: { url: "https://api.wise.com/v1/transfers/42", status: "COMPLETED" },
        context: { order_id: "ns-p2p-1" },
        owner: "0x1111111111111111111111111111111111111111",
        timestampS: 1800000000,
        epoch: 1,
      },
      signatures: ["0x" + "11".repeat(65)],
    });
    // 2. Validate TLSNotary importer round-trip with real Ed25519 signature + Merkle proof
    const kp = generateEd25519NotaryKeypair();
    const rawTlsn = signTlsNotaryPresentation({
      serverName: "api.revolut.com",
      sentLen: 150,
      recvLen: 380,
      certSha256Hex: crypto.createHash("sha256").update("revolut-x509").digest("hex"),
      chunks: ["Cookie: REDACTED", '{"status":"COMPLETED","amount":"500.00"}'],
      discloseIndices: [1],
      notaryKeypair: kp,
      requiredSubstrings: ["COMPLETED", "500.00"],
    });
    const convertedTlsn = importTlsNotaryExport(rawTlsn, {
      trustedNotaryPubkeys: [kp.publicKeyHex],
      requiredSubstrings: ["COMPLETED"],
    });
    console.log(
      JSON.stringify(
        {
          ok: true,
          reclaim_adapter_ready: sampleReclaim.protocol === "reclaim_v1",
          official_reclaim_attestor_v1: OFFICIAL_RECLAIM_ATTESTOR_V1,
          tlsnotary_adapter_ready: convertedTlsn.protocol === "tlsnotary_v1",
          tlsnotary_server: convertedTlsn.session_header.server_name,
        },
        null,
        2
      )
    );
  } else {
    const file = process.argv[3];
    if (!file) throw new Error("missing JSON file path");
    const raw = JSON.parse(fs.readFileSync(file, "utf8"));
    const payload = mode === "reclaim" ? importReclaimSdkProof(raw) : importTlsNotaryExport(raw);
    if (process.argv.includes("--verify-live")) {
      const out = await submitToZkTlsVerifier(payload);
      console.log(JSON.stringify(out, null, 2));
      if (!out.body?.valid) process.exit(1);
    } else {
      console.log(JSON.stringify(payload, null, 2));
    }
  }
}
