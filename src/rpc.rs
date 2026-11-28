use crate::crypto::{sha256_concat, Hash32};
use crate::seed::{SeedStatus, AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PROTOCOL_VERSION: &str = "NEWSATOSHI/2.1";
/// Soft-fork opcode tag for `OP_VERIFY_COVENANT_STARK` (proposed NOP5 redefinition: 0xb4).
pub const OP_VERIFY_COVENANT_STARK_HEX: &str = "b4";
/// Placeholder shielded Newsatoshi address template for copy-pasteable L1 CLI commands.
pub const DEFAULT_ANCHOR_ADDRESS_PLACEHOLDER: &str = "<COMMONS_NS1_ADDRESS>";

/// Builds ready-to-run real `newsatoshi` v1.2.5 CLI commands for anchoring `anchor_hash` on L1.
pub fn format_newsatoshi_cli_commands(network: &str, anchor_hash: &str) -> (String, String) {
    let net_flag = if network == "mainnet" {
        String::new()
    } else {
        format!("--net {network} ")
    };
    let lock_cmd = format!(
        "newsatoshi {net_flag}lock 0.01 --hash {anchor_hash} --to {DEFAULT_ANCHOR_ADDRESS_PLACEHOLDER}"
    );
    let send_memo_cmd = format!(
        "newsatoshi {net_flag}send {DEFAULT_ANCHOR_ADDRESS_PLACEHOLDER} 0.01 --memo {anchor_hash}"
    );
    (lock_cmd, send_memo_cmd)
}

/// Deterministic L1 anchor commitment produced from a `CommonsLedger` state transition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorCommitment {
    pub protocol_version: String,
    pub covenant_rule_hash: String,
    pub prev_state_root: String,
    pub new_state_root: String,
    pub tx_nonce: u64,
    pub covenant_balance_love: u64,
    pub parcels_acquired: u64,
    pub total_kwh_verified: u64,
    pub merkle_leaves_consumed: u64,
    pub anchor_hash: String,
    /// `OP_RETURN` + `OP_PUSHBYTES_32` + `<anchor_hash>` commitment script.
    pub op_return_script_hex: String,
    /// `OP_VERIFY_COVENANT_STARK` witness envelope prefix committing to rule + state transition.
    pub stark_witness_commitment_hex: String,
    /// Optional live L1 network bound into this commitment (`testnet` | `mainnet`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_network: Option<String>,
    /// Optional live L1 block height observed when creating this anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_height: Option<u64>,
    /// Optional live L1 tip block hash observed when creating this anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_tip_hash: Option<String>,
    /// Optional L1 genesis block hash (`testnet` or `mainnet`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_genesis: Option<String>,
    /// Optional Newsatoshi AuxPoW merged-mining chain ID (`20051`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auxpow_chainid: Option<u32>,
    /// Ready-to-run `newsatoshi` v0.1.0 SHA-256 hash-lock CLI command committing to `anchor_hash`.
    #[serde(default)]
    pub cli_lock_command: String,
    /// Ready-to-run `newsatoshi` v0.1.0 shielded memo send CLI command committing to `anchor_hash`.
    #[serde(default)]
    pub cli_send_memo_command: String,
}

impl AnchorCommitment {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        covenant_rule_hash: Hash32,
        prev_state_root: Hash32,
        new_state_root: Hash32,
        tx_nonce: u64,
        covenant_balance_love: u64,
        parcels_acquired: u64,
        total_kwh_verified: u64,
        merkle_leaves_consumed: u64,
    ) -> Self {
        let anchor_digest = sha256_concat(&[
            PROTOCOL_VERSION.as_bytes(),
            &covenant_rule_hash,
            &prev_state_root,
            &new_state_root,
            &tx_nonce.to_be_bytes(),
            &covenant_balance_love.to_be_bytes(),
            &parcels_acquired.to_be_bytes(),
            &total_kwh_verified.to_be_bytes(),
            &merkle_leaves_consumed.to_be_bytes(),
        ]);

        let stark_witness_digest = sha256_concat(&[
            b"OP_VERIFY_COVENANT_STARK_V2_1",
            &covenant_rule_hash,
            &prev_state_root,
            &new_state_root,
            &anchor_digest,
        ]);

        let anchor_hex = hex::encode(anchor_digest);
        // Script: OP_RETURN (0x6a) | OP_PUSHBYTES_32 (0x20) | <32-byte anchor_hash>
        let op_return_script_hex = format!("6a20{anchor_hex}");
        // Soft-fork witness envelope: OP_PUSHBYTES_32 (0x20) | <stark_witness> | OP_VERIFY_COVENANT_STARK (0xb4)
        let stark_witness_commitment_hex = format!(
            "20{}{}",
            hex::encode(stark_witness_digest),
            OP_VERIFY_COVENANT_STARK_HEX
        );
        let (cli_lock_command, cli_send_memo_command) =
            format_newsatoshi_cli_commands("testnet", &anchor_hex);

        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            covenant_rule_hash: hex::encode(covenant_rule_hash),
            prev_state_root: hex::encode(prev_state_root),
            new_state_root: hex::encode(new_state_root),
            tx_nonce,
            covenant_balance_love,
            parcels_acquired,
            total_kwh_verified,
            merkle_leaves_consumed,
            anchor_hash: anchor_hex,
            op_return_script_hex,
            stark_witness_commitment_hex,
            l1_network: None,
            l1_height: None,
            l1_tip_hash: None,
            l1_genesis: None,
            auxpow_chainid: None,
            cli_lock_command,
            cli_send_memo_command,
        }
    }

    /// Cryptographically binds a live (or genesis) Newsatoshi L1 chain tip into this commitment,
    /// updating `anchor_hash`, `op_return_script_hex`, `stark_witness_commitment_hex`, and CLI commands.
    pub fn with_l1_binding(
        mut self,
        l1_network: impl Into<String>,
        l1_height: u64,
        l1_tip_hash: impl Into<String>,
        l1_genesis: impl Into<String>,
        auxpow_chainid: u32,
    ) -> Self {
        let net = l1_network.into();
        let tip = l1_tip_hash.into();
        let genesis = l1_genesis.into();

        let bound_anchor = sha256_concat(&[
            b"NEWSATOSHI_L1_TIP_BOUND_ANCHOR_V2_1",
            self.anchor_hash.as_bytes(),
            net.as_bytes(),
            &l1_height.to_be_bytes(),
            tip.as_bytes(),
            genesis.as_bytes(),
            &auxpow_chainid.to_be_bytes(),
        ]);

        let stark_witness_digest = sha256_concat(&[
            b"OP_VERIFY_COVENANT_STARK_V2_1",
            self.covenant_rule_hash.as_bytes(),
            self.prev_state_root.as_bytes(),
            self.new_state_root.as_bytes(),
            &bound_anchor,
        ]);

        let anchor_hex = hex::encode(bound_anchor);
        let (cli_lock_command, cli_send_memo_command) =
            format_newsatoshi_cli_commands(&net, &anchor_hex);

        self.anchor_hash = anchor_hex.clone();
        self.op_return_script_hex = format!("6a20{anchor_hex}");
        self.stark_witness_commitment_hex = format!(
            "20{}{}",
            hex::encode(stark_witness_digest),
            OP_VERIFY_COVENANT_STARK_HEX
        );
        self.l1_network = Some(net);
        self.l1_height = Some(l1_height);
        self.l1_tip_hash = Some(tip);
        self.l1_genesis = Some(genesis);
        self.auxpow_chainid = Some(auxpow_chainid);
        self.cli_lock_command = cli_lock_command;
        self.cli_send_memo_command = cli_send_memo_command;
        self
    }

    /// Convenience binding directly from a polled `SeedStatus`.
    pub fn with_seed_status(self, status: &SeedStatus) -> Self {
        self.with_l1_binding(
            status.network.clone(),
            status.height,
            status.tip.clone(),
            status.genesis.clone(),
            status.auxpow_chainid,
        )
    }

    /// Convenience binding to verified Newsatoshi v0.1.0 mainnet genesis (height 0).
    pub fn with_mainnet_genesis(self) -> Self {
        self.with_l1_binding(
            "mainnet",
            0,
            MAINNET_GENESIS_HASH,
            MAINNET_GENESIS_HASH,
            AUXPOW_CHAIN_ID,
        )
    }
}

/// Standard JSON-RPC 2.0 request payload for a `newsatoshi` L1 node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorRpcParams {
    pub network: String,
    pub commitment: AnchorCommitment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyStarkRpcParams {
    pub network: String,
    pub opcode: String,
    pub covenant_rule_hash: String,
    pub prev_state_root: String,
    pub new_state_root: String,
    pub stark_witness_commitment_hex: String,
}

/// Deterministic receipt returned for a prepared (unbroadcast) or confirmed L1 anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorReceipt {
    pub mode: String,
    #[serde(default = "default_unbroadcast")]
    pub broadcast_status: String,
    pub endpoint: String,
    pub network: String,
    pub rpc_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_txid: Option<String>,
    pub anchor_hash: String,
    pub op_return_script_hex: String,
    #[serde(default)]
    pub stark_witness_commitment_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_tip_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_genesis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auxpow_chainid: Option<u32>,
    #[serde(default)]
    pub cli_lock_command: String,
    #[serde(default)]
    pub cli_send_memo_command: String,
    #[serde(default)]
    pub tx_nonce: u64,
    #[serde(default)]
    pub parcels_acquired: u64,
    #[serde(default)]
    pub total_kwh_verified: u64,
    #[serde(default)]
    pub created_at_unix: u64,
    /// True only when independently verified via live Newsatoshi L1 `/api/lock?hash=<anchor_hash>`.
    #[serde(default)]
    pub verified_on_l1: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_lock_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_confirmed_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_confirmations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_lock_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l1_verified_at_unix: Option<u64>,
}

fn default_unbroadcast() -> String {
    "unbroadcast".to_string()
}

/// Client for building and optionally posting deterministic JSON-RPC 2.0 state-root
/// anchor payloads to a running `newsatoshi` L1 node.
#[derive(Clone, Debug)]
pub struct JsonRpcAnchorClient {
    pub endpoint_url: String,
    pub network: String,
    pub rpc_user: Option<String>,
    pub rpc_pass: Option<String>,
}

impl JsonRpcAnchorClient {
    pub fn new(endpoint_url: impl Into<String>, network: impl Into<String>) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
            network: network.into(),
            rpc_user: None,
            rpc_pass: None,
        }
    }

    pub fn testnet_default() -> Self {
        Self::new("http://127.0.0.1:18332/", "testnet")
    }

    pub fn mainnet_default() -> Self {
        Self::new("http://127.0.0.1:8332/", "mainnet")
    }

    /// Builds a deterministic `newsatoshi_anchorCovenantRoot` JSON-RPC 2.0 request.
    pub fn build_anchor_request(
        &self,
        commitment: &AnchorCommitment,
        id: u64,
    ) -> JsonRpcRequest<AnchorRpcParams> {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: "newsatoshi_anchorCovenantRoot".to_string(),
            params: AnchorRpcParams {
                network: self.network.clone(),
                commitment: commitment.clone(),
            },
        }
    }

    /// Builds a deterministic `newsatoshi_verifyCovenantStark` JSON-RPC 2.0 request.
    pub fn build_verify_stark_request(
        &self,
        commitment: &AnchorCommitment,
        id: u64,
    ) -> JsonRpcRequest<VerifyStarkRpcParams> {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: "newsatoshi_verifyCovenantStark".to_string(),
            params: VerifyStarkRpcParams {
                network: self.network.clone(),
                opcode: "OP_VERIFY_COVENANT_STARK".to_string(),
                covenant_rule_hash: commitment.covenant_rule_hash.clone(),
                prev_state_root: commitment.prev_state_root.clone(),
                new_state_root: commitment.new_state_root.clone(),
                stark_witness_commitment_hex: commitment.stark_witness_commitment_hex.clone(),
            },
        }
    }

    /// Serializes the anchor request into canonical compact JSON.
    pub fn anchor_payload_json(&self, commitment: &AnchorCommitment, id: u64) -> String {
        let req = self.build_anchor_request(commitment, id);
        serde_json::to_string(&req).expect("AnchorRpcParams serialization is infallible")
    }

    /// Serializes the anchor request into pretty-printed JSON for CLI output / inspection.
    pub fn anchor_payload_pretty(&self, commitment: &AnchorCommitment, id: u64) -> String {
        let req = self.build_anchor_request(commitment, id);
        serde_json::to_string_pretty(&req).expect("AnchorRpcParams serialization is infallible")
    }

    /// Prepares a deterministic, unbroadcast anchor receipt bound to L1 commitment data.
    /// Does NOT fabricate a fake L1 txid: `l1_txid` remains `None` until actually broadcast on L1.
    pub fn submit_dry_run(&self, commitment: &AnchorCommitment, id: u64) -> AnchorReceipt {
        let (default_lock, default_memo) =
            format_newsatoshi_cli_commands(&self.network, &commitment.anchor_hash);
        let cli_lock_command = if commitment.cli_lock_command.is_empty() {
            default_lock
        } else {
            commitment.cli_lock_command.clone()
        };
        let cli_send_memo_command = if commitment.cli_send_memo_command.is_empty() {
            default_memo
        } else {
            commitment.cli_send_memo_command.clone()
        };

        let mode = if commitment.l1_height.is_some() {
            "l1-tip-bound-unbroadcast".to_string()
        } else {
            "unbroadcast".to_string()
        };

        AnchorReceipt {
            mode,
            broadcast_status: "unbroadcast".to_string(),
            endpoint: self.endpoint_url.clone(),
            network: self.network.clone(),
            rpc_id: id,
            l1_txid: None,
            anchor_hash: commitment.anchor_hash.clone(),
            op_return_script_hex: commitment.op_return_script_hex.clone(),
            stark_witness_commitment_hex: commitment.stark_witness_commitment_hex.clone(),
            l1_height: commitment.l1_height,
            l1_tip_hash: commitment.l1_tip_hash.clone(),
            l1_genesis: commitment.l1_genesis.clone(),
            auxpow_chainid: commitment.auxpow_chainid,
            cli_lock_command,
            cli_send_memo_command,
            tx_nonce: commitment.tx_nonce,
            parcels_acquired: commitment.parcels_acquired,
            total_kwh_verified: commitment.total_kwh_verified,
            created_at_unix: unix_now(),
            verified_on_l1: false,
            l1_lock_id: None,
            l1_confirmed_height: None,
            l1_confirmations: None,
            l1_lock_state: None,
            l1_verified_at_unix: None,
        }
    }

    /// Sends the JSON-RPC 2.0 anchor payload over raw HTTP/1.1 `TcpStream` to an `http://host:port/path`
    /// Newsatoshi node (zero external HTTP runtime dependencies).
    pub fn submit_live_http(
        &self,
        commitment: &AnchorCommitment,
        id: u64,
        timeout: Duration,
    ) -> Result<String, String> {
        let (host_port, path) = parse_http_endpoint(&self.endpoint_url)?;
        let body = self.anchor_payload_json(commitment, id);

        let mut stream = TcpStream::connect(&host_port)
            .map_err(|e| format!("failed to connect to Newsatoshi node at {host_port}: {e}"))?;
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));

        let http_request = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: {host_port}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n\
             {body}",
            body.len()
        );

        stream
            .write_all(http_request.as_bytes())
            .map_err(|e| format!("failed to send JSON-RPC payload: {e}"))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| format!("failed to read JSON-RPC response: {e}"))?;

        Ok(response)
    }
}

fn parse_http_endpoint(url: &str) -> Result<(String, String), String> {
    let stripped = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("only plain http:// endpoints are supported (got '{url}')"))?;

    let (host_port, path) = match stripped.find('/') {
        Some(idx) => (&stripped[..idx], &stripped[idx..]),
        None => (stripped, "/"),
    };

    if host_port.is_empty() {
        return Err("empty host in JSON-RPC endpoint URL".to_string());
    }

    let host_port = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:80")
    };

    Ok((host_port, path.to_string()))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
