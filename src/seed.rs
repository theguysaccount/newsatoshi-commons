use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SEED_STATUS_URL: &str = "https://newsatoshi-seed.fly.dev/status";
pub const MAINNET_GENESIS_HASH: &str =
    "000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7";
pub const TESTNET_GENESIS_HASH: &str =
    "00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394";
pub const AUXPOW_CHAIN_ID: u32 = 20051;
pub const MAINNET_BLOCK_REWARD_LOVE: u64 = 50;
pub const MAINNET_P2P_PORT: u16 = 8444;
pub const TESTNET_P2P_PORT: u16 = 18444;

/// Single block entry inside the `recent` array of `/status`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedRecentBlock {
    pub height: u64,
    pub hash: String,
    #[serde(default)]
    pub time: u64,
    #[serde(default)]
    pub txs: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub merge_mined: bool,
}

/// Live JSON status returned by a Newsatoshi v0.1.0 seed node (`/status`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeedStatus {
    pub network: String,
    #[serde(default)]
    pub proofs: String,
    #[serde(default)]
    pub fee: u64,
    #[serde(default)]
    pub version: String,
    pub genesis: String,
    pub height: u64,
    pub tip: String,
    #[serde(default)]
    pub tip_time: u64,
    #[serde(default)]
    pub now: u64,
    #[serde(default)]
    pub bits: String,
    #[serde(default)]
    pub difficulty: f64,
    #[serde(default)]
    pub chainwork: String,
    #[serde(default)]
    pub peers: u64,
    #[serde(default)]
    pub known_peers: u64,
    #[serde(default)]
    pub best_header: u64,
    #[serde(default)]
    pub mempool: u64,
    #[serde(default)]
    pub issued: u64,
    #[serde(default)]
    pub notes: u64,
    #[serde(default)]
    pub nullifiers: u64,
    #[serde(default)]
    pub locks: u64,
    #[serde(default)]
    pub conditions: u64,
    #[serde(default = "default_auxpow_chainid")]
    pub auxpow_chainid: u32,
    #[serde(default)]
    pub recent: Vec<SeedRecentBlock>,
}

fn default_auxpow_chainid() -> u32 {
    AUXPOW_CHAIN_ID
}

/// Verified Newsatoshi v0.1.0 mainnet genesis constants (inspected from newsatoshi-seed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainnetInfo {
    pub network: String,
    pub genesis: String,
    pub height: u64,
    pub auxpow_chainid: u32,
    pub block_reward_love: u64,
    pub p2p_port: u16,
    pub note: String,
}

impl MainnetInfo {
    pub fn verified_v0_1_0() -> Self {
        Self {
            network: "mainnet".to_string(),
            genesis: MAINNET_GENESIS_HASH.to_string(),
            height: 0,
            auxpow_chainid: AUXPOW_CHAIN_ID,
            block_reward_love: MAINNET_BLOCK_REWARD_LOVE,
            p2p_port: MAINNET_P2P_PORT,
            note: "Newsatoshi v0.1.0 mainnet genesis (height 0, 50 LOVE/block, AuxPoW chainid 20051; seed does not self-mine mainnet).".to_string(),
        }
    }
}

/// Envelope returned by `GET /api/seed` and used by L1 anchor binding.
/// When the seed is unreachable, `live` is `false`, `status` is `None`, and `error` reports why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeedSnapshot {
    pub live: bool,
    pub endpoint: String,
    pub fetched_at_unix: u64,
    pub status: Option<SeedStatus>,
    pub mainnet_info: MainnetInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// HTTP/HTTPS client polling a live Newsatoshi L1 seed `/status` endpoint via `ureq`.
#[derive(Clone, Debug)]
pub struct SeedStatusClient {
    pub endpoint_url: String,
    pub timeout: Duration,
}

impl SeedStatusClient {
    pub fn new(endpoint_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
            timeout,
        }
    }

    pub fn default_testnet() -> Self {
        Self::new(DEFAULT_SEED_STATUS_URL, Duration::from_secs(5))
    }

    /// Parses raw JSON text from `/status` into a typed `SeedStatus`.
    pub fn parse_status_json(json_str: &str) -> Result<SeedStatus, String> {
        serde_json::from_str::<SeedStatus>(json_str)
            .map_err(|e| format!("failed to parse newsatoshi-seed status JSON: {e}"))
    }

    /// Fetches live JSON from the configured seed endpoint (strict timeout).
    pub fn fetch(&self) -> Result<SeedStatus, String> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build();
        let agent: ureq::Agent = config.into();

        let mut resp = agent
            .get(&self.endpoint_url)
            .header("Accept", "application/json")
            .header("User-Agent", "newsatoshi-commons/0.3.0")
            .call()
            .map_err(|e| format!("seed fetch from '{}' failed: {e}", self.endpoint_url))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("failed to read seed response body: {e}"))?;

        Self::parse_status_json(&body)
    }

    /// Fetches live seed status; returns `status: None` with the exact error if unreachable.
    pub fn fetch_snapshot(&self) -> SeedSnapshot {
        let now = unix_now();
        match self.fetch() {
            Ok(status) => SeedSnapshot {
                live: true,
                endpoint: self.endpoint_url.clone(),
                fetched_at_unix: now,
                status: Some(status),
                mainnet_info: MainnetInfo::verified_v0_1_0(),
                error: None,
            },
            Err(err) => SeedSnapshot {
                live: false,
                endpoint: self.endpoint_url.clone(),
                fetched_at_unix: now,
                status: None,
                mainnet_info: MainnetInfo::verified_v0_1_0(),
                error: Some(err),
            },
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
