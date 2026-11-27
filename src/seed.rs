use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SEED_STATUS_URL: &str = "https://newsatoshi-seed.fly.dev/status";
pub const DEFAULT_MAINNET_SEED_STATUS_URL: &str = "https://newsatoshi-seed-v2.fly.dev/status";
pub const MAINNET_GENESIS_HASH: &str =
    "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8";
pub const TESTNET_GENESIS_HASH: &str =
    "00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394";
pub const AUXPOW_CHAIN_ID: u32 = 20051;
pub const MAINNET_BLOCK_REWARD_LOVE: u64 = 50;
pub const MAINNET_P2P_PORT: u16 = 8445;
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

/// Live on-chain hash-lock record returned by `GET /api/lock?hash=<64hex>` on Newsatoshi v0.1.0.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct L1LockRecord {
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub amount: u64,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub timeout: u64,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub refund: String,
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub confirmations: u64,
    #[serde(default)]
    pub height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct L1LockListResponse {
    #[serde(default)]
    locks: Vec<L1LockRecord>,
}

/// Verified Newsatoshi v0.1.0 mainnet genesis metadata (dynamically refreshed when live mainnet status responds).
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
            network: "mainnet-v2".to_string(),
            genesis: MAINNET_GENESIS_HASH.to_string(),
            height: 0,
            auxpow_chainid: AUXPOW_CHAIN_ID,
            block_reward_love: MAINNET_BLOCK_REWARD_LOVE,
            p2p_port: MAINNET_P2P_PORT,
            note: "Newsatoshi v1.2.0 Mainnet v2 (nsM2) canonical genesis anchored to Bitcoin #977,738 (50 LOVE/block, AuxPoW chainid 20051, bits 1d00ffff, zero premine; seeds do not self-mine).".to_string(),
        }
    }

    pub fn from_live_status(s: &SeedStatus) -> Self {
        let note = if s.height == 0 {
            "Live Newsatoshi v1.2.0 Mainnet v2 (nsM2) online at canonical Bitcoin #977,738 genesis height 0 (zero premine; awaiting external SHA-256 / Bitcoin AuxPoW miners on chainid 20051).".to_string()
        } else {
            format!(
                "Live Newsatoshi v{} Mainnet v2 active at height #{} (AuxPoW chainid {}, {} peers, {} locks).",
                s.version, s.height, s.auxpow_chainid, s.peers, s.locks
            )
        };
        Self {
            network: s.network.clone(),
            genesis: s.genesis.clone(),
            height: s.height,
            auxpow_chainid: s.auxpow_chainid,
            block_reward_love: MAINNET_BLOCK_REWARD_LOVE,
            p2p_port: MAINNET_P2P_PORT,
            note,
        }
    }
}

/// Envelope returned by `GET /api/seed` and used by L1 anchor binding.
/// When a seed is unreachable, `live` is `false`, `status` is `None`, and `error` reports why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeedSnapshot {
    pub live: bool,
    pub endpoint: String,
    pub fetched_at_unix: u64,
    pub status: Option<SeedStatus>,
    pub mainnet_info: MainnetInfo,
    #[serde(default)]
    pub mainnet_live: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainnet_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainnet_status: Option<SeedStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainnet_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// HTTP/HTTPS client polling a live Newsatoshi L1 seed `/status` and `/api/lock` endpoints via `ureq`.
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

    pub fn default_mainnet() -> Self {
        Self::new(DEFAULT_MAINNET_SEED_STATUS_URL, Duration::from_secs(5))
    }

    /// Parses raw JSON text from `/status` into a typed `SeedStatus`.
    pub fn parse_status_json(json_str: &str) -> Result<SeedStatus, String> {
        serde_json::from_str::<SeedStatus>(json_str)
            .map_err(|e| format!("failed to parse newsatoshi-seed status JSON: {e}"))
    }

    /// Derives the corresponding `/api/lock?hash=<hash_hex>` URL from a `/status` endpoint URL.
    ///
    /// Examples:
    /// - `https://newsatoshi-seed.fly.dev/status` -> `https://newsatoshi-seed.fly.dev/api/lock?hash=...`
    /// - `https://newsatoshi.org/mainnet/status` -> `https://newsatoshi.org/mainnet/api/lock?hash=...`
    pub fn lock_by_hash_url(&self, hash_hex: &str) -> String {
        let trimmed = self.endpoint_url.trim_end_matches('/');
        let base = if let Some(prefix) = trimmed.strip_suffix("/status") {
            prefix
        } else {
            trimmed
        };
        format!("{base}/api/lock?hash={hash_hex}")
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
            .header("User-Agent", "newsatoshi-commons/0.4.0")
            .call()
            .map_err(|e| format!("seed fetch from '{}' failed: {e}", self.endpoint_url))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("failed to read seed response body: {e}"))?;

        Self::parse_status_json(&body)
    }

    /// Queries the live L1 node's `GET /api/lock?hash=<hash_hex>` endpoint to verify whether an
    /// anchor hash is confirmed on-chain as a Newsatoshi SHA-256 hash-lock.
    pub fn fetch_locks_for_hash(&self, hash_hex: &str) -> Result<Vec<L1LockRecord>, String> {
        let clean_bytes = crate::crypto::hash32_from_hex(hash_hex)
            .map_err(|e| format!("invalid 64-hex hash for L1 lock lookup: {e}"))?;
        let normalized = hex::encode(clean_bytes);
        let url = self.lock_by_hash_url(&normalized);
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build();
        let agent: ureq::Agent = config.into();

        let mut resp = agent
            .get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "newsatoshi-commons/0.4.0")
            .call()
            .map_err(|e| format!("L1 lock verification fetch from '{url}' failed: {e}"))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("failed to read L1 lock response body: {e}"))?;

        let parsed = serde_json::from_str::<L1LockListResponse>(&body)
            .map_err(|e| format!("failed to parse L1 /api/lock response JSON: {e}"))?;
        Ok(parsed.locks)
    }

    /// Fetches live seed status; returns `status: None` with the exact error if unreachable.
    pub fn fetch_snapshot(&self) -> SeedSnapshot {
        self.fetch_dual_snapshot(None)
    }

    /// Polls primary (testnet) and optional live mainnet seed endpoints simultaneously.
    pub fn fetch_dual_snapshot(&self, mainnet_client: Option<&SeedStatusClient>) -> SeedSnapshot {
        let now = unix_now();
        let (live, status, error) = match self.fetch() {
            Ok(s) => (true, Some(s), None),
            Err(err) => (false, None, Some(err)),
        };

        let (mainnet_live, mainnet_endpoint, mainnet_status, mainnet_error, mainnet_info) =
            match mainnet_client {
                Some(mc) => match mc.fetch() {
                    Ok(ms) => {
                        let info = MainnetInfo::from_live_status(&ms);
                        (
                            true,
                            Some(mc.endpoint_url.clone()),
                            Some(ms),
                            None,
                            info,
                        )
                    }
                    Err(merr) => (
                        false,
                        Some(mc.endpoint_url.clone()),
                        None,
                        Some(merr),
                        MainnetInfo::verified_v0_1_0(),
                    ),
                },
                None => (false, None, None, None, MainnetInfo::verified_v0_1_0()),
            };

        SeedSnapshot {
            live,
            endpoint: self.endpoint_url.clone(),
            fetched_at_unix: now,
            status,
            mainnet_info,
            mainnet_live,
            mainnet_endpoint,
            mainnet_status,
            mainnet_error,
            error,
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
