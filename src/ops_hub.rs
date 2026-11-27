//! Secret, Token-Gated Operations, Success Telemetry & Narrative Push Hub (`src/ops_hub.rs`).
//!
//! Security & Stealth Properties:
//! - Mounted ONLY when BOTH `OPS_HUB_SLUG` and `OPS_HUB_TOKEN` env vars are set.
//! - Secret login URL: `GET /ops/<OPS_HUB_SLUG>` (unlisted in sitemap.xml, robots.txt, llms.txt).
//! - Constant-time SHA-256 verification for both `Authorization: Bearer <OPS_HUB_TOKEN>` and
//!   `HttpOnly; Secure; SameSite=Strict` session cookie (`ns_ops_session`).
//! - Every response carries `X-Robots-Tag: noindex, nofollow, noarchive` and `Cache-Control: no-store`.
//! - Zero fake data: all telemetry comes strictly from live Mainnet v2 seed, AuxPoW pool,
//!   4-chain Backing Vault, GitHub API, and authenticated snapshots pushed by `success_tracker.py`.

use crate::backing::BackingProofSummary;
use crate::crypto::sha256;
use crate::seed::SeedSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, StatusCode};

const OPS_HUB_UI_HTML: &str = include_str!("ops_hub_ui.html");
const SESSION_COOKIE_NAME: &str = "ns_ops_session";
const MAX_OPS_BODY_BYTES: u64 = 262_144;

#[derive(Clone, Debug)]
pub struct OpsHubAuth {
    pub slug: String,
    token_digest: [u8; 32],
    session_cookie_hex: String,
}

impl OpsHubAuth {
    pub fn from_env() -> Option<Self> {
        let slug = std::env::var("OPS_HUB_SLUG")
            .ok()
            .map(|s| s.trim().trim_matches('/').to_string())
            .filter(|s| s.len() >= 8 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))?;
        let token = std::env::var("OPS_HUB_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| s.len() >= 12)?;
        Some(Self::new(slug, &token))
    }

    pub fn new(slug: String, token: &str) -> Self {
        let token_digest = sha256(token.as_bytes());
        let mut cookie_preimage = Vec::with_capacity(24 + token.len());
        cookie_preimage.extend_from_slice(b"ns-ops-session-v1:");
        cookie_preimage.extend_from_slice(token.as_bytes());
        let session_digest = sha256(&cookie_preimage);
        Self {
            slug,
            token_digest,
            session_cookie_hex: hex::encode(session_digest),
        }
    }

    pub fn verify_token(&self, candidate: &str) -> bool {
        let actual = sha256(candidate.trim().as_bytes());
        ct_eq_bytes(&actual, &self.token_digest)
    }

    pub fn verify_cookie(&self, cookie_val: &str) -> bool {
        ct_eq_str(cookie_val.trim(), &self.session_cookie_hex)
    }

    pub fn is_authorized(&self, req: &Request) -> bool {
        // 1. Check Authorization: Bearer <OPS_HUB_TOKEN>
        if let Some(bearer) = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str().trim())
            .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        {
            if self.verify_token(bearer) {
                return true;
            }
        }
        // 2. Check HttpOnly cookie `ns_ops_session`
        for h in req.headers().iter().filter(|h| h.field.equiv("Cookie")) {
            for part in h.value.as_str().split(';') {
                let kv = part.trim();
                if let Some(val) = kv.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
                    if self.verify_cookie(val) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn hub_path(&self) -> String {
        format!("/ops/{}", self.slug)
    }
}

fn ct_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    ct_eq_bytes(a.as_bytes(), b.as_bytes())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpsMetricSnapshot {
    pub timestamp_utc: String,
    pub timestamp_unix: u64,
    pub source: String,
    pub height: u64,
    pub difficulty: f64,
    pub issued_love: f64,
    pub peers: u64,
    pub shielded_notes: u64,
    pub nullifiers: u64,
    pub open_channels: u64,
    pub open_priors: u64,
    pub recent_merge_mined_pct: f64,
    pub tip: String,
    pub pool_online: bool,
    pub pool_workers: u64,
    pub pool_hashrate_ghs: f64,
    pub pool_aux_blocks: u64,
    pub vault_eth: String,
    pub vault_usdc: String,
    pub vault_deposits: u64,
    pub web4_services: usize,
    pub gh_l1_stars: u64,
    pub gh_l1_forks: u64,
    pub gh_l1_issues: u64,
    pub gh_l1_clones_14d: u64,
    pub gh_l1_unique_clones_14d: u64,
    pub gh_commons_stars: u64,
    pub gh_commons_forks: u64,
    pub gh_commons_clones_14d: u64,
    pub gh_commons_unique_clones_14d: u64,
    pub bounty_love: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutreachTarget {
    pub id: String,
    pub category: String,
    pub target: String,
    pub angle: String,
    pub channel: String,
    pub status: String, // Queued | Drafted | Sent | In Talks | Integrated
    pub notes: String,
    pub updated_utc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiMilestone {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: String, // Open | In Progress | Shipped
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpsHubState {
    pub snapshots: Vec<OpsMetricSnapshot>,
    pub outreach: Vec<OutreachTarget>,
    pub ai_milestones: Vec<AiMilestone>,
    pub narrative_notes: String,
}

impl Default for OpsHubState {
    fn default() -> Self {
        let now = now_iso_utc();
        Self {
            snapshots: Vec::new(),
            outreach: default_outreach_targets(&now),
            ai_milestones: default_ai_milestones(),
            narrative_notes: String::new(),
        }
    }
}

pub struct OpsHubManager {
    pub auth: Option<OpsHubAuth>,
    store_path: PathBuf,
    state: Mutex<OpsHubState>,
}

impl OpsHubManager {
    pub fn init(state_file: &Path) -> Self {
        let auth = OpsHubAuth::from_env();
        let store_path = state_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("ops-hub-state.json");
        let loaded = load_ops_state(&store_path).unwrap_or_default();
        Self {
            auth,
            store_path,
            state: Mutex::new(loaded),
        }
    }

    #[cfg(test)]
    pub fn init_with_auth(store_path: PathBuf, auth: OpsHubAuth) -> Self {
        let loaded = load_ops_state(&store_path).unwrap_or_default();
        Self {
            auth: Some(auth),
            store_path,
            state: Mutex::new(loaded),
        }
    }

    fn persist_locked(&self, st: &OpsHubState) -> Result<(), String> {
        if let Some(parent) = self.store_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;
        }
        let tmp = self.store_path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(st).map_err(|e| format!("serialize failed: {e}"))?;
        fs::write(&tmp, bytes).map_err(|e| format!("write tmp failed: {e}"))?;
        fs::rename(&tmp, &self.store_path).map_err(|e| format!("rename failed: {e}"))?;
        Ok(())
    }

    pub fn record_snapshot(&self, snap: OpsMetricSnapshot) -> Result<usize, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.snapshots.push(snap);
        if st.snapshots.len() > 2000 {
            let excess = st.snapshots.len() - 2000;
            st.snapshots.drain(0..excess);
        }
        let count = st.snapshots.len();
        self.persist_locked(&st)?;
        Ok(count)
    }

    pub fn upsert_outreach(&self, item: OutreachTarget) -> Result<Vec<OutreachTarget>, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = st.outreach.iter_mut().find(|x| x.id == item.id) {
            *existing = item;
        } else {
            st.outreach.push(item);
        }
        self.persist_locked(&st)?;
        Ok(st.outreach.clone())
    }

    pub fn delete_outreach(&self, id: &str) -> Result<Vec<OutreachTarget>, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.outreach.retain(|x| x.id != id);
        self.persist_locked(&st)?;
        Ok(st.outreach.clone())
    }

    pub fn update_milestone(&self, id: &str, status: &str) -> Result<Vec<AiMilestone>, String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(m) = st.ai_milestones.iter_mut().find(|x| x.id == id) {
            m.status = status.to_string();
        }
        self.persist_locked(&st)?;
        Ok(st.ai_milestones.clone())
    }

    pub fn update_notes(&self, notes: String) -> Result<(), String> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.narrative_notes = notes;
        self.persist_locked(&st)
    }

    pub fn snapshot_state(&self) -> OpsHubState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Attempts to handle `/ops/*` and `/api/ops/*` requests. Returns `Some(())` if handled,
/// or `None` if the request path is outside the Ops Hub namespace.
pub fn try_handle_ops_request(
    req: Request,
    path: &str,
    ops: &OpsHubManager,
    get_seed: impl FnOnce() -> SeedSnapshot,
    get_backing: impl FnOnce() -> BackingProofSummary,
) -> Option<Request> {
    let is_ops_path = path.starts_with("/ops/") || path == "/ops" || path.starts_with("/api/ops/");
    if !is_ops_path {
        return Some(req);
    }

    let Some(auth) = ops.auth.as_ref() else {
        // Unconfigured -> act like route does not exist (stealth 404).
        respond_stealth_404(req);
        return None;
    };

    let method = req.method().clone();
    let expected_page = auth.hub_path();

    // 1. Secret War-Room Portal Page (`GET /ops/<OPS_HUB_SLUG>`)
    if method == Method::Get && (path == expected_page || path == format!("{expected_page}/")) {
        if auth.is_authorized(&req) {
            let html = OPS_HUB_UI_HTML.replace("__OPS_HUB_PATH__", &expected_page);
            respond_private_html(req, 200, html, None);
        } else {
            respond_private_html(req, 200, render_login_gate_html(&expected_page), None);
        }
        return None;
    }

    // Any other `/ops/...` path -> stealth 404
    if path == "/ops" || path.starts_with("/ops/") {
        respond_stealth_404(req);
        return None;
    }

    // 2. Login Endpoint (`POST /api/ops/login`)
    if method == Method::Post && path == "/api/ops/login" {
        let mut req = req;
        let Ok(body) = read_body_limited(&mut req) else {
            respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid body"}), None);
            return None;
        };
        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let token = parsed.get("token").and_then(|v| v.as_str()).unwrap_or("");
        if auth.verify_token(token) {
            let cookie = format!(
                "{SESSION_COOKIE_NAME}={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=2592000",
                auth.session_cookie_hex
            );
            respond_private_json(
                req,
                200,
                &json!({"ok": true, "hub_path": expected_page}),
                Some(cookie),
            );
        } else {
            respond_private_json(
                req,
                401,
                &json!({"ok": false, "error": "Invalid War-Room passphrase"}),
                None,
            );
        }
        return None;
    }

    // 3. Logout Endpoint (`POST /api/ops/logout`)
    if method == Method::Post && path == "/api/ops/logout" {
        let clear = format!(
            "{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0"
        );
        respond_private_json(req, 200, &json!({"ok": true}), Some(clear));
        return None;
    }

    // All remaining `/api/ops/*` routes require strict auth (Cookie or Bearer Token)
    if !auth.is_authorized(&req) {
        respond_private_json(
            req,
            401,
            &json!({"ok": false, "error": "Authentication required"}),
            None,
        );
        return None;
    }

    match (method, path) {
        // Live overview + history + outreach + AI scorecard
        (Method::Get, "/api/ops/overview") => {
            let seed = get_seed();
            let backing = get_backing();
            let prev = ops.snapshot_state();
            let last_snap = prev.snapshots.last();
            let live_snap = build_live_server_snapshot(&seed, &backing, last_snap);

            // Auto-record a live snapshot every 15 minutes (or on first ever view)
            let should_autorecord = match last_snap {
                None => true,
                Some(prev_s) => live_snap.timestamp_unix.saturating_sub(prev_s.timestamp_unix) >= 900,
            };
            if should_autorecord {
                let _ = ops.record_snapshot(live_snap.clone());
            }

            let current = ops.snapshot_state();
            respond_private_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "live": live_snap,
                    "snapshots": current.snapshots,
                    "outreach": current.outreach,
                    "ai_milestones": current.ai_milestones,
                    "narrative_notes": current.narrative_notes,
                    "hub_path": expected_page
                }),
                None,
            );
        }

        // Capture a live snapshot immediately OR accept a rich telemetry sync from `success_tracker.py`
        (Method::Post, "/api/ops/snapshot") => {
            let mut req = req;
            let body = read_body_limited(&mut req).unwrap_or_default();
            let seed = get_seed();
            let backing = get_backing();
            let prev = ops.snapshot_state();
            let mut snap = build_live_server_snapshot(&seed, &backing, prev.snapshots.last());

            if let Ok(val) = serde_json::from_str::<Value>(&body) {
                if let Some(obj) = val.as_object() {
                    if obj.contains_key("gh_l1_clones_14d") || obj.contains_key("bounty_love") {
                        snap.source = obj
                            .get("source")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tracker_sync")
                            .to_string();
                        if let Some(v) = obj.get("gh_l1_stars").and_then(|x| x.as_u64()) {
                            snap.gh_l1_stars = v;
                        }
                        if let Some(v) = obj.get("gh_l1_forks").and_then(|x| x.as_u64()) {
                            snap.gh_l1_forks = v;
                        }
                        if let Some(v) = obj.get("gh_l1_issues").and_then(|x| x.as_u64()) {
                            snap.gh_l1_issues = v;
                        }
                        if let Some(v) = obj.get("gh_l1_clones_14d").and_then(|x| x.as_u64()) {
                            snap.gh_l1_clones_14d = v;
                        }
                        if let Some(v) = obj.get("gh_l1_unique_clones_14d").and_then(|x| x.as_u64()) {
                            snap.gh_l1_unique_clones_14d = v;
                        }
                        if let Some(v) = obj.get("gh_commons_stars").and_then(|x| x.as_u64()) {
                            snap.gh_commons_stars = v;
                        }
                        if let Some(v) = obj.get("gh_commons_forks").and_then(|x| x.as_u64()) {
                            snap.gh_commons_forks = v;
                        }
                        if let Some(v) = obj.get("gh_commons_clones_14d").and_then(|x| x.as_u64()) {
                            snap.gh_commons_clones_14d = v;
                        }
                        if let Some(v) = obj.get("gh_commons_unique_clones_14d").and_then(|x| x.as_u64()) {
                            snap.gh_commons_unique_clones_14d = v;
                        }
                        if let Some(v) = obj.get("bounty_love").and_then(|x| x.as_f64()) {
                            if v.is_finite() && v >= 0.0 {
                                snap.bounty_love = v;
                            }
                        }
                    }
                }
            }

            match ops.record_snapshot(snap.clone()) {
                Ok(total) => respond_private_json(
                    req,
                    200,
                    &json!({"ok": true, "total_snapshots": total, "snapshot": snap}),
                    None,
                ),
                Err(e) => respond_private_json(req, 500, &json!({"ok": false, "error": e}), None),
            }
        }

        // Upsert an outreach target (status, notes, or new entry)
        (Method::Post, "/api/ops/outreach") => {
            let mut req = req;
            let Ok(body) = read_body_limited(&mut req) else {
                respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid body"}), None);
                return None;
            };
            let Ok(mut item) = serde_json::from_str::<OutreachTarget>(&body) else {
                respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid outreach JSON"}), None);
                return None;
            };
            if item.id.trim().is_empty() {
                item.id = format!("custom-{}", now_unix());
            }
            item.updated_utc = now_iso_utc();
            match ops.upsert_outreach(item) {
                Ok(list) => respond_private_json(req, 200, &json!({"ok": true, "outreach": list}), None),
                Err(e) => respond_private_json(req, 500, &json!({"ok": false, "error": e}), None),
            }
        }

        // Delete an outreach target
        (Method::Post, "/api/ops/outreach/delete") => {
            let mut req = req;
            let Ok(body) = read_body_limited(&mut req) else {
                respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid body"}), None);
                return None;
            };
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");
            match ops.delete_outreach(id) {
                Ok(list) => respond_private_json(req, 200, &json!({"ok": true, "outreach": list}), None),
                Err(e) => respond_private_json(req, 500, &json!({"ok": false, "error": e}), None),
            }
        }

        // Toggle an AI SOTA milestone status
        (Method::Post, "/api/ops/milestone") => {
            let mut req = req;
            let Ok(body) = read_body_limited(&mut req) else {
                respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid body"}), None);
                return None;
            };
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("Open");
            match ops.update_milestone(id, status) {
                Ok(list) => respond_private_json(req, 200, &json!({"ok": true, "ai_milestones": list}), None),
                Err(e) => respond_private_json(req, 500, &json!({"ok": false, "error": e}), None),
            }
        }

        // Save founder narrative scratchpad notes
        (Method::Post, "/api/ops/notes") => {
            let mut req = req;
            let Ok(body) = read_body_limited(&mut req) else {
                respond_private_json(req, 400, &json!({"ok": false, "error": "Invalid body"}), None);
                return None;
            };
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let notes = parsed
                .get("narrative_notes")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match ops.update_notes(notes) {
                Ok(()) => respond_private_json(req, 200, &json!({"ok": true}), None),
                Err(e) => respond_private_json(req, 500, &json!({"ok": false, "error": e}), None),
            }
        }

        _ => respond_stealth_404(req),
    }

    None
}

fn build_live_server_snapshot(
    seed: &SeedSnapshot,
    backing: &BackingProofSummary,
    prev: Option<&OpsMetricSnapshot>,
) -> OpsMetricSnapshot {
    let ms = seed.mainnet_status.as_ref();
    let height = ms.map(|s| s.height).unwrap_or(0);
    let difficulty = ms.map(|s| (s.difficulty * 100.0).round() / 100.0).unwrap_or(0.0);
    let issued_love = ms.map(|s| ((s.issued as f64 / 1e8) * 100.0).round() / 100.0).unwrap_or(0.0);
    let peers = ms.map(|s| s.peers).unwrap_or(0);
    let shielded_notes = ms.map(|s| s.notes).unwrap_or(0);
    let nullifiers = ms.map(|s| s.nullifiers).unwrap_or(0);
    let seed_raw = fetch_json_short("https://newsatoshi-seed-v2.fly.dev/status");
    let open_channels = seed_raw
        .as_ref()
        .and_then(|v| v.get("channels"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let open_priors = seed_raw
        .as_ref()
        .and_then(|v| v.get("priors"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tip = ms.map(|s| s.tip.clone()).unwrap_or_default();
    let recent_merge_mined_pct = ms
        .map(|s| {
            if s.recent.is_empty() {
                0.0
            } else {
                let mm = s.recent.iter().filter(|b| b.merge_mined).count();
                ((mm as f64 * 1000.0) / s.recent.len() as f64).round() / 10.0
            }
        })
        .unwrap_or(0.0);

    // Live AuxPoW pool status
    let pool = fetch_json_short("https://pool.newsatoshi.org/");
    let pool_online = pool.as_ref().and_then(|v| v.get("online")).and_then(|v| v.as_bool()).unwrap_or(false);
    let pool_workers = pool.as_ref().and_then(|v| v.get("connected_workers")).and_then(|v| v.as_u64()).unwrap_or(0);
    let pool_hashrate_ghs = pool.as_ref().and_then(|v| v.get("estimated_pool_hashrate_ghs")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let pool_aux_blocks = pool.as_ref().and_then(|v| v.get("aux_blocks_accepted")).and_then(|v| v.as_u64()).unwrap_or(0);

    // Public GitHub stars/forks/issues (best effort; carries forward traffic clones & bounty from last tracker_sync)
    let gh_l1 = fetch_json_short("https://api.github.com/repos/theguysaccount/newsatoshi-rfc0021");
    let gh_com = fetch_json_short("https://api.github.com/repos/theguysaccount/newsatoshi-commons");

    let gh_l1_stars = gh_l1
        .as_ref()
        .and_then(|v| v.get("stargazers_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| prev.map(|p| p.gh_l1_stars).unwrap_or(0));
    let gh_l1_forks = gh_l1
        .as_ref()
        .and_then(|v| v.get("forks_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| prev.map(|p| p.gh_l1_forks).unwrap_or(0));
    let gh_l1_issues = gh_l1
        .as_ref()
        .and_then(|v| v.get("open_issues_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| prev.map(|p| p.gh_l1_issues).unwrap_or(0));
    let gh_commons_stars = gh_com
        .as_ref()
        .and_then(|v| v.get("stargazers_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| prev.map(|p| p.gh_commons_stars).unwrap_or(0));
    let gh_commons_forks = gh_com
        .as_ref()
        .and_then(|v| v.get("forks_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| prev.map(|p| p.gh_commons_forks).unwrap_or(0));

    OpsMetricSnapshot {
        timestamp_utc: now_iso_utc(),
        timestamp_unix: now_unix(),
        source: "server_live".to_string(),
        height,
        difficulty,
        issued_love,
        peers,
        shielded_notes,
        nullifiers,
        open_channels,
        open_priors,
        recent_merge_mined_pct,
        tip,
        pool_online,
        pool_workers,
        pool_hashrate_ghs,
        pool_aux_blocks,
        vault_eth: backing.total_eth_formatted.clone(),
        vault_usdc: backing.total_usdc_formatted.clone(),
        vault_deposits: backing.total_vault_deposits,
        web4_services: backing.registered_web4_services,
        gh_l1_stars,
        gh_l1_forks,
        gh_l1_issues,
        gh_l1_clones_14d: prev.map(|p| p.gh_l1_clones_14d).unwrap_or(0),
        gh_l1_unique_clones_14d: prev.map(|p| p.gh_l1_unique_clones_14d).unwrap_or(0),
        gh_commons_stars,
        gh_commons_forks,
        gh_commons_clones_14d: prev.map(|p| p.gh_commons_clones_14d).unwrap_or(0),
        gh_commons_unique_clones_14d: prev.map(|p| p.gh_commons_unique_clones_14d).unwrap_or(0),
        bounty_love: prev.map(|p| p.bounty_love).unwrap_or(0.0),
    }
}

fn fetch_json_short(url: &str) -> Option<Value> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent();
    let mut resp = agent
        .get(url)
        .header("User-Agent", "newsatoshi-commons-ops/1.0")
        .header("Accept", "application/json")
        .call()
        .ok()?;
    let text = resp.body_mut().read_to_string().ok()?;
    serde_json::from_str(&text).ok()
}

fn load_ops_state(path: &Path) -> Option<OpsHubState> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_body_limited(req: &mut Request) -> Result<String, String> {
    use std::io::Read;
    let mut buf = Vec::new();
    req.as_reader()
        .take(MAX_OPS_BODY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read error: {e}"))?;
    if buf.len() as u64 > MAX_OPS_BODY_BYTES {
        return Err("body too large".to_string());
    }
    String::from_utf8(buf).map_err(|_| "invalid utf-8".to_string())
}

fn security_headers(extra_cookie: Option<String>) -> Vec<Header> {
    let mut headers = vec![
        Header::from_bytes("X-Robots-Tag", "noindex, nofollow, noarchive").unwrap(),
        Header::from_bytes("Cache-Control", "no-store, private, max-age=0").unwrap(),
        Header::from_bytes("Referrer-Policy", "no-referrer").unwrap(),
        Header::from_bytes("X-Frame-Options", "DENY").unwrap(),
        Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap(),
    ];
    if let Some(c) = extra_cookie {
        if let Ok(h) = Header::from_bytes("Set-Cookie", c) {
            headers.push(h);
        }
    }
    headers
}

fn respond_private_html(req: Request, status: u16, html: String, cookie: Option<String>) {
    let mut resp = Response::from_string(html)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    for h in security_headers(cookie) {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn respond_private_json(req: Request, status: u16, payload: &Value, cookie: Option<String>) {
    let body = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    let mut resp = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap());
    for h in security_headers(cookie) {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn respond_stealth_404(req: Request) {
    let mut resp = Response::from_string("Not Found")
        .with_status_code(StatusCode(404))
        .with_header(Header::from_bytes("Content-Type", "text/plain; charset=utf-8").unwrap());
    for h in security_headers(None) {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_iso_utc() -> String {
    // Pure stdlib UTC ISO-8601 formatter (Howard Hinnant civil-from-days algorithm)
    let secs = now_unix() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3_600;
    let min = (rem % 3_600) / 60;
    let sec = rem % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn render_login_gate_html(hub_path: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="robots" content="noindex,nofollow,noarchive">
<title>Newsatoshi War Room · Authenticate</title>
<style>
:root{{--bg:#07080a;--card:#0f1116;--border:#232734;--ink:#eef2f8;--mute:#8c95a8;--accent:#10b981;--red:#ef4444}}
*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}}
.box{{width:min(420px,92vw);background:var(--card);border:1px solid var(--border);border-radius:16px;padding:28px;box-shadow:0 24px 70px rgba(0,0,0,.6)}}
.badge{{display:inline-flex;align-items:center;gap:8px;font-size:11px;letter-spacing:.12em;text-transform:uppercase;color:var(--accent);margin-bottom:14px}}
.dot{{width:8px;height:8px;border-radius:99px;background:var(--accent)}}
h1{{margin:0 0 8px;font-size:20px;font-weight:600}}p{{margin:0 0 20px;color:var(--mute);font-size:12px}}
input{{width:100%;padding:12px 14px;border-radius:10px;border:1px solid var(--border);background:#090b0f;color:var(--ink);font:inherit;margin-bottom:12px}}
input:focus{{outline:none;border-color:var(--accent)}}
button{{width:100%;padding:12px;border:0;border-radius:10px;background:var(--accent);color:#03130d;font:inherit;font-weight:700;cursor:pointer}}
button:hover{{filter:brightness(1.08)}}
.err{{color:var(--red);font-size:12px;min-height:18px;margin-top:10px}}
</style>
</head>
<body>
<form class="box" id="f">
  <div class="badge"><span class="dot"></span>INTERNAL OPS WAR ROOM</div>
  <h1>Authenticate Session</h1>
  <p>Private Newsatoshi Mainnet v2 telemetry, AI SOTA tracker &amp; narrative command hub. Passphrase stored in <code>agent-vault</code> (<code>newsatoshi/ops-hub</code>).</p>
  <input id="tok" type="password" placeholder="Enter Ops Hub Passphrase" autocomplete="current-password" required autofocus>
  <button type="submit">Unlock War Room</button>
  <div class="err" id="err"></div>
</form>
<script>
document.getElementById('f').addEventListener('submit', async (e) => {{
  e.preventDefault();
  const err = document.getElementById('err');
  err.textContent = '';
  const token = document.getElementById('tok').value;
  try {{
    const r = await fetch('/api/ops/login', {{
      method: 'POST',
      headers: {{'Content-Type': 'application/json'}},
      body: JSON.stringify({{ token }})
    }});
    const d = await r.json();
    if (r.ok && d.ok) {{
      window.location.replace('{hub_path}');
    }} else {{
      err.textContent = d.error || 'Authentication failed';
    }}
  }} catch (ex) {{
    err.textContent = 'Network error: ' + ex.message;
  }}
}});
</script>
</body>
</html>"#
    )
}

fn default_outreach_targets(now: &str) -> Vec<OutreachTarget> {
    let rows = [
        ("btc-01", "Underground Bitcoin", "Bitcointalk [ANN] Alternative Cryptocurrencies", "Classic Satoshi-style BBCode drop: 8-page PDF, BTC #977,738 anchor, 0% premine, pure SHA-2 STARKs, 0%-fee AuxPoW, LOVE Bug Bounty", "Bitcointalk Forum", "Drafted", "BBCode ready in Narrative Copy Vault"),
        ("btc-02", "Underground Bitcoin", "Delving Bitcoin / Cryptography & Cypherpunks Mailing Lists", "Pure SHA-2/SHA-384 Quartic FRI STARKs + Lean 4 249-constraint Rust extraction + Namecoin-compatible AuxPoW chainid=20051", "Delving Bitcoin / Mailing List", "Drafted", "Technical note ready in Copy Vault"),
        ("btc-03", "Underground Bitcoin", "Nostr (#bitcoin #cypherpunk #bitaxe) & Stacker News (~tech)", "8-page Satoshi PDF + live browser WASM SHA-384 STARK verifier + 0%-fee Bitaxe pool", "Nostr / Stacker News", "Drafted", "Ready in Copy Vault"),
        ("mine-01", "Bitcoin AuxPoW Mining", "Bitaxe / OSMU Community (Open Source Miners United)", "Zero-fee solo AuxPoW pool (pool.newsatoshi.org:3333) — earn real 50 LOVE blocks on existing Bitaxe hardware", "OSMU Discord / X / r/Bitaxe", "Drafted", "30-second Bitaxe config ready"),
        ("mine-02", "Bitcoin AuxPoW Mining", "OCEAN Pool / Braiins / Public-Pool / CKPool Operators", "Drop-in AuxPoW (chainid=20051) extra revenue for Bitcoin miners with zero extra electricity", "Direct email / GitHub", "Queued", "Link auxpow-stratum/MULTI_POOL_ONBOARDING.md"),
        ("sec-01", "Cryptographers & Auditors", "Hacker News (Show HN) & Lobste.rs", "Show HN: Private post-quantum cash using pure SHA-2 STARKs, mechanized in Lean 4, merge-mined by Bitcoin", "Hacker News / Lobste.rs", "Drafted", "Title + author comment ready"),
        ("sec-02", "Cryptographers & Auditors", "Immunefi / Code4rena / Independent ZK & Lean 4 Researchers", "Public LOVE Security Bug Bounty — break 249-constraint pure SHA-384 STARK or zero-sorry Lean 4 proofs", "X / EthResearch / Email", "Queued", "Fund bounty wallet with ASIC-mined LOVE first"),
        ("ai-01", "AI Agent Payments", "x402 Ecosystem, OpenClaw, ElizaOS, Venice.ai & NanoGPT", "Zero-volatility USD-quoted O(1) PayWords + nsUSD (RFC-0028) so agents pay privately without KYC or freeze keys", "Direct email / Discord / X", "Drafted", "Direct founder pitch ready"),
        ("ai-02", "AI Agent Payments", "Awesome-MCP-Servers & Official MCP Directories", "List Newsatoshi Commons MCP (30 zero-auth tools: nsUSD quote, PayWords HTTP 402, atomic BTC/EVM/Solana swaps)", "GitHub PR", "Drafted", "PR snippet ready"),
        ("app-01", "Privacy & Stablecoin Apps", "Privacy-First VPNs, Cloud VPS & Dev APIs (Mullvad-style / LNVPN)", "Accept private quantum-proof USD-settled payments (nsUSD / JIT atomic swaps) with zero custody freeze risk", "Direct email / GitHub", "Queued", "Pitch live /api/v2/nsusd/quote + PayWords SDK"),
        ("app-02", "Crypto Directories & Explorers", "MiningPoolStats, CoinPaprika, Awesome-ZK, Awesome-Post-Quantum", "Index Mainnet v2 (nsM2), live block explorer, and Satoshi-format v2.2 whitepaper PDF", "GitHub PR / Listing Forms", "Queued", "Drives persistent organic discovery"),
    ];
    rows.into_iter()
        .map(|(id, cat, target, angle, channel, status, notes)| OutreachTarget {
            id: id.to_string(),
            category: cat.to_string(),
            target: target.to_string(),
            angle: angle.to_string(),
            channel: channel.to_string(),
            status: status.to_string(),
            notes: notes.to_string(),
            updated_utc: now.to_string(),
        })
        .collect()
}

fn default_ai_milestones() -> Vec<AiMilestone> {
    vec![
        AiMilestone {
            id: "ms-lean-soundness".to_string(),
            title: "End-to-End Semantic Lean 4 Soundness Proof".to_string(),
            detail: "Prove in SpendCircuitFull.lean that any accepted STARK trace + boundary assertions imply ownership, Merkle membership, nullifier uniqueness, and value conservation (beyond polynomial identity equality).".to_string(),
            status: "Open".to_string(),
        },
        AiMilestone {
            id: "ms-compact-or-recursion".to_string(),
            title: "Compact Pure-Hash Proofs (WHIR/Binius) or Full 152-Query Recursive Verifier".to_string(),
            detail: "Close the ~603 KB single-spend proof size gap vs Zcash Orchard (~5 KB) & Neptune (~30 KB) via reserved BIP-9 BIT_FUTURE_COMPACT_PROOF = 5 or full OOD/152-query GPU recursion.".to_string(),
            status: "Open".to_string(),
        },
        AiMilestone {
            id: "ms-audit-and-nsusd".to_string(),
            title: "External Audit of Winterfell ZK Masking + Live Stress-Tested Sovereign nsUSD CDP".to_string(),
            detail: "Secure 1-2 independent ZK audits on spend.rs + Haböck-Al Kindi masking patch, fund LOVE Bug Bounty via ASIC mining, and ship stress-tested PriorLock-oracle BTC redemption/liquidation for Tier-2 nsUSD.".to_string(),
            status: "Open".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_hub_auth_and_persistence_roundtrip() {
        let auth = OpsHubAuth::new("secret-warroom-test".to_string(), "super-strong-passphrase-123");
        assert!(auth.verify_token("super-strong-passphrase-123"));
        assert!(!auth.verify_token("wrong-passphrase"));
        assert!(auth.verify_cookie(&auth.session_cookie_hex));
        assert!(!auth.verify_cookie("0000"));
        assert_eq!(auth.hub_path(), "/ops/secret-warroom-test");

        let tmp = std::env::temp_dir().join(format!("ops-hub-test-{}.json", now_unix()));
        let _ = fs::remove_file(&tmp);
        let mgr = OpsHubManager::init_with_auth(tmp.clone(), auth);

        let snap = OpsMetricSnapshot {
            timestamp_utc: now_iso_utc(),
            timestamp_unix: now_unix(),
            source: "test".to_string(),
            height: 71,
            difficulty: 310.46,
            issued_love: 3550.0,
            peers: 6,
            shielded_notes: 0,
            nullifiers: 0,
            open_channels: 0,
            open_priors: 0,
            recent_merge_mined_pct: 100.0,
            tip: "abc".to_string(),
            pool_online: true,
            pool_workers: 0,
            pool_hashrate_ghs: 0.0,
            pool_aux_blocks: 0,
            vault_eth: "0.001350".to_string(),
            vault_usdc: "0.765311".to_string(),
            vault_deposits: 6,
            web4_services: 5,
            gh_l1_stars: 0,
            gh_l1_forks: 0,
            gh_l1_issues: 0,
            gh_l1_clones_14d: 54,
            gh_l1_unique_clones_14d: 33,
            gh_commons_stars: 0,
            gh_commons_forks: 0,
            gh_commons_clones_14d: 444,
            gh_commons_unique_clones_14d: 111,
            bounty_love: 0.0,
        };
        assert_eq!(mgr.record_snapshot(snap).unwrap(), 1);
        mgr.update_milestone("ms-lean-soundness", "In Progress").unwrap();
        mgr.update_notes("Day 1 launch push".to_string()).unwrap();

        let reloaded = load_ops_state(&tmp).expect("persisted json");
        assert_eq!(reloaded.snapshots.len(), 1);
        assert_eq!(reloaded.snapshots[0].height, 71);
        assert_eq!(reloaded.narrative_notes, "Day 1 launch push");
        assert_eq!(
            reloaded.ai_milestones.iter().find(|m| m.id == "ms-lean-soundness").unwrap().status,
            "In Progress"
        );
        let _ = fs::remove_file(&tmp);
    }
}
