use crate::crypto::{sha256, sha256_concat};
use crate::hardware::{ExternalDeedPacket, ExternalTelemetryPacket};
use crate::rpc::{AnchorReceipt, JsonRpcAnchorClient, PROTOCOL_VERSION};
use crate::seed::{
    L1LockRecord, SeedSnapshot, SeedStatusClient, DEFAULT_MAINNET_SEED_STATUS_URL,
    DEFAULT_SEED_STATUS_URL,
};
use crate::stark::{
    prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind,
    StarkProofArtifact,
};
use crate::storage::{
    load_or_init_state_with_mode, save_state_atomic, ApiStateView, CommonsEngine,
    ExternalPublicKeysConfig, FundingRecord, ParcelRecord, StreamingRecord, TelemetryRecord,
};
use crate::{CheckedWorkProof, StreamingChannel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// Configuration for the Commons Engine HTTP API server (`newsatoshi-commons serve`).
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub state_file: PathBuf,
    pub seed_url: String,
    /// Optional live Newsatoshi L1 mainnet `/status` URL (default: `https://newsatoshi.org/mainnet/status`).
    pub mainnet_seed_url: Option<String>,
    pub static_dir: Option<PathBuf>,
    /// Optional bearer token protecting operator write endpoints on public deployments.
    pub admin_token: Option<String>,
    /// Strict Mainnet key-isolation mode: strips all private signing keys from server memory & disk.
    pub verify_only: bool,
    /// Optional external public-key roots used when initializing a fresh `--verify-only` ledger.
    pub external_public_keys: Option<ExternalPublicKeysConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8787,
            state_file: PathBuf::from("./data/commons-ledger.json"),
            seed_url: DEFAULT_SEED_STATUS_URL.to_string(),
            mainnet_seed_url: Some(DEFAULT_MAINNET_SEED_STATUS_URL.to_string()),
            static_dir: None,
            admin_token: None,
            verify_only: false,
            external_public_keys: None,
        }
    }
}

struct CachedSeed {
    fetched_at: Instant,
    snapshot: SeedSnapshot,
}

struct AppState {
    engine: Mutex<CommonsEngine>,
    state_file: PathBuf,
    seed_client: SeedStatusClient,
    mainnet_seed_client: Option<SeedStatusClient>,
    seed_cache: Mutex<Option<CachedSeed>>,
    static_dir: Option<PathBuf>,
    admin_token_digest: Option<[u8; 32]>,
    verify_only: bool,
    external_public_keys: Option<ExternalPublicKeysConfig>,
    started_at: Instant,
}

impl AppState {
    fn get_seed_snapshot(&self, force_refresh: bool) -> SeedSnapshot {
        let mut cache = self.seed_cache.lock().unwrap();
        if !force_refresh {
            if let Some(ref c) = *cache {
                if c.fetched_at.elapsed() < Duration::from_secs(5) {
                    return c.snapshot.clone();
                }
            }
        }
        let snapshot = self
            .seed_client
            .fetch_dual_snapshot(self.mainnet_seed_client.as_ref());
        *cache = Some(CachedSeed {
            fetched_at: Instant::now(),
            snapshot: snapshot.clone(),
        });
        snapshot
    }

    fn client_for_network(&self, network: &str) -> Option<&SeedStatusClient> {
        if network.eq_ignore_ascii_case("mainnet") {
            self.mainnet_seed_client.as_ref()
        } else {
            Some(&self.seed_client)
        }
    }
}

/// Running background HTTP server handle (primarily for integration tests & embedding).
pub struct RunningServer {
    pub addr: String,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RunningServer {
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn stop(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Starts the Commons Engine HTTP server in a background thread (supports `port: 0` for tests).
pub fn spawn_server(config: ServerConfig) -> Result<RunningServer, String> {
    let bind_addr = format!("{}:{}", config.host, config.port);
    let server = Server::http(&bind_addr)
        .map_err(|e| format!("failed to bind HTTP server on {bind_addr}: {e}"))?;
    let actual_addr = server
        .server_addr()
        .to_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| bind_addr.clone());

    let engine = load_or_init_state_with_mode(
        &config.state_file,
        config.verify_only,
        config.external_public_keys.as_ref(),
    )?;
    let admin_token_digest = config
        .admin_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| sha256(s.as_bytes()));
    let mainnet_seed_client = config
        .mainnet_seed_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|u| SeedStatusClient::new(u, Duration::from_secs(5)));
    let verify_only = engine.verify_only || config.verify_only;
    let state = Arc::new(AppState {
        engine: Mutex::new(engine),
        state_file: config.state_file,
        seed_client: SeedStatusClient::new(config.seed_url, Duration::from_secs(5)),
        mainnet_seed_client,
        seed_cache: Mutex::new(None),
        static_dir: config.static_dir,
        admin_token_digest,
        verify_only,
        external_public_keys: config.external_public_keys,
        started_at: Instant::now(),
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_loop = Arc::clone(&shutdown);
    let handle = thread::spawn(move || {
        while !shutdown_loop.load(Ordering::SeqCst) {
            match server.recv_timeout(Duration::from_millis(50)) {
                Ok(Some(req)) => {
                    handle_request(req, &state);
                }
                Ok(None) => continue,
                Err(_) => break,
            }
        }
    });

    Ok(RunningServer {
        addr: actual_addr,
        shutdown,
        handle: Some(handle),
    })
}

/// Runs the Commons Engine HTTP API server in the foreground until terminated.
pub fn run_server_blocking(config: ServerConfig) -> Result<(), String> {
    let bind_addr = format!("{}:{}", config.host, config.port);
    let server = Server::http(&bind_addr)
        .map_err(|e| format!("failed to bind HTTP server on {bind_addr}: {e}"))?;
    let engine = load_or_init_state_with_mode(
        &config.state_file,
        config.verify_only,
        config.external_public_keys.as_ref(),
    )?;

    println!("=== NEWSATOSHI 2.1 COMMONS ENGINE HTTP SERVER ===");
    println!("Listening    : http://{bind_addr}");
    println!("State file   : {}", config.state_file.display());
    println!("Seed URL     : {}", config.seed_url);
    if let Some(ref m_url) = config.mainnet_seed_url {
        println!("Mainnet URL  : {m_url}");
    }
    if let Some(ref dir) = config.static_dir {
        println!("Static UI    : {}", dir.display());
    }
    println!(
        "Rule Hash    : {}",
        hex::encode(engine.rule.rule_hash())
    );
    println!(
        "Key Mode     : {}",
        if engine.verify_only || !engine.has_signing_keys() {
            "VERIFY-ONLY (zero private signing keys loaded)"
        } else {
            "local/testnet signing keys enabled"
        }
    );
    println!(
        "Treasury     : {} LOVE | Nonce {}",
        engine.ledger.total_covenant_balance(&engine.rule),
        engine.ledger.tx_nonce
    );

    let admin_token_digest = config
        .admin_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| sha256(s.as_bytes()));
    println!(
        "Operator Auth: {}",
        if admin_token_digest.is_some() {
            "enabled (Bearer token required for operator writes)"
        } else {
            "disabled (local development mode)"
        }
    );

    let mainnet_seed_client = config
        .mainnet_seed_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|u| SeedStatusClient::new(u, Duration::from_secs(5)));
    let verify_only = engine.verify_only || config.verify_only;
    let state = Arc::new(AppState {
        engine: Mutex::new(engine),
        state_file: config.state_file,
        seed_client: SeedStatusClient::new(config.seed_url, Duration::from_secs(5)),
        mainnet_seed_client,
        seed_cache: Mutex::new(None),
        static_dir: config.static_dir,
        admin_token_digest,
        verify_only,
        external_public_keys: config.external_public_keys,
        started_at: Instant::now(),
    });

    for req in server.incoming_requests() {
        handle_request(req, &state);
    }

    Ok(())
}

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type, Authorization").unwrap(),
    ]
}

fn respond_json<T: Serialize>(req: Request, status: u16, payload: &T) {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap());
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn respond_empty(req: Request, status: u16) {
    let mut resp = Response::empty(StatusCode(status));
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn read_body_string(req: &mut Request) -> Result<String, String> {
    let mut body = String::new();
    req.as_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("failed to read HTTP request body: {e}"))?;
    Ok(body)
}

fn is_authorized_operator(req: &Request, state: &AppState) -> bool {
    let Some(expected) = state.admin_token_digest else {
        return true;
    };
    let provided = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().trim())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim);
    let Some(token) = provided else {
        return false;
    };
    let actual = sha256(token.as_bytes());
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= actual[i] ^ expected[i];
    }
    diff == 0
}

fn respond_unauthorized(req: Request) {
    respond_json(
        req,
        401,
        &json!({
            "ok": false,
            "error": "Operator authentication required (provide Authorization: Bearer <COMMONS_ADMIN_TOKEN>)"
        }),
    );
}

fn handle_request(mut req: Request, state: &AppState) {
    let method = req.method().clone();
    let raw_url = req.url().to_string();
    let path = raw_url.split('?').next().unwrap_or(&raw_url).to_string();

    if method == Method::Options {
        respond_empty(req, 204);
        return;
    }

    match (method, path.as_str()) {
        (Method::Get, "/api/health") => {
            let uptime_secs = state.started_at.elapsed().as_secs();
            let signing_keys_loaded = {
                let engine = state.engine.lock().unwrap();
                engine.has_signing_keys()
            };
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocol_version": PROTOCOL_VERSION,
                    "uptime_secs": uptime_secs,
                    "auth_required": state.admin_token_digest.is_some(),
                    "verify_only": state.verify_only || !signing_keys_loaded,
                    "signing_keys_loaded": signing_keys_loaded,
                    "mainnet_seed_url": state.mainnet_seed_client.as_ref().map(|c| c.endpoint_url.as_str()),
                }),
            );
        }
        (Method::Get, "/api/state") => {
            let api_state = {
                let engine = state.engine.lock().unwrap();
                engine.to_api_state()
            };
            respond_json(req, 200, &api_state);
        }
        (Method::Get, "/api/seed") => {
            let force = raw_url.contains("refresh=1") || raw_url.contains("refresh=true");
            let snapshot = state.get_seed_snapshot(force);
            respond_json(req, 200, &snapshot);
        }
        (Method::Post, "/api/actions/record-funding") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: RecordFundingRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_record_funding(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/confirm-anchor") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: ConfirmAnchorRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_confirm_anchor(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/acquire-parcel") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            if state.verify_only {
                respond_json(
                    req,
                    403,
                    &json!({
                        "ok": false,
                        "error": "Server is running in --verify-only mode (zero private signing keys loaded); submit an offline-signed ExternalDeedPacket to POST /api/deeds"
                    }),
                );
                return;
            }
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: AcquireParcelRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_acquire_parcel(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/deeds") => {
            // Cryptographically self-authenticating offline-signed land deed packets (XMSS / SLH-DSA)
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: ExternalDeedPacket = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid external deed packet: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_external_deed(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/verify-anchor") => {
            // Public on-chain L1 cross-check against live Newsatoshi /api/lock?hash=<anchor_hash>
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: VerifyAnchorRequest = if body.trim().is_empty() {
                VerifyAnchorRequest::default()
            } else {
                match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(e) => {
                        respond_json(
                            req,
                            400,
                            &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                        );
                        return;
                    }
                }
            };
            let resp = handle_verify_anchor(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/telemetry") | (Method::Post, "/api/telemetry") => {
            let authorized = is_authorized_operator(&req, state);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let value: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = if value.get("merkle_sig").is_some() || value.get("slh_dsa_sig_hex").is_some()
            {
                // Cryptographically self-authenticating hardware packets (XMSS / SLH-DSA)
                match serde_json::from_value::<ExternalTelemetryPacket>(value) {
                    Ok(ext_pkt) => handle_external_telemetry(state, ext_pkt),
                    Err(e) => {
                        respond_json(
                            req,
                            400,
                            &json!({ "ok": false, "error": format!("invalid external telemetry packet: {e}") }),
                        );
                        return;
                    }
                }
            } else {
                if !authorized {
                    respond_unauthorized(req);
                    return;
                }
                if state.verify_only {
                    respond_json(
                        req,
                        403,
                        &json!({
                            "ok": false,
                            "error": "Server is running in --verify-only mode (zero private signing keys loaded); submit cryptographically signed telemetry packets from newsatoshi-hw-signer to POST /api/telemetry"
                        }),
                    );
                    return;
                }
                match serde_json::from_value::<TelemetryActionRequest>(value) {
                    Ok(sim_req) => handle_simulated_telemetry(state, sim_req),
                    Err(e) => {
                        respond_json(
                            req,
                            400,
                            &json!({ "ok": false, "error": format!("invalid telemetry request: {e}") }),
                        );
                        return;
                    }
                }
            };
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/stream-settle") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: StreamSettleRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_stream_settle(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/anchor") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: AnchorActionRequest = if body.trim().is_empty() {
                AnchorActionRequest::default()
            } else {
                serde_json::from_str(&body).unwrap_or_default()
            };
            let resp = handle_anchor(state, parsed);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/verify-stark") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: VerifyStarkRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_verify_stark(state, &parsed.proof_id);
            respond_json(req, 200, &resp);
        }
        (Method::Post, "/api/actions/reset") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let resp = handle_reset(state);
            respond_json(req, 200, &resp);
        }
        (Method::Get, _) => {
            if let Some(ref static_dir) = state.static_dir {
                serve_static_file(req, static_dir, &path);
            } else {
                respond_json(
                    req,
                    404,
                    &json!({ "ok": false, "error": format!("route not found: {path}") }),
                );
            }
        }
        _ => {
            respond_json(
                req,
                404,
                &json!({ "ok": false, "error": format!("unsupported route: {path}") }),
            );
        }
    }
}

#[derive(Debug, Deserialize)]
struct RecordFundingRequest {
    l1_network: String,
    l1_txid: String,
    amount_love: u64,
    #[serde(default)]
    note: String,
}

#[derive(Serialize)]
struct RecordFundingResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    funding: Option<FundingRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_record_funding(state: &AppState, req: RecordFundingRequest) -> RecordFundingResponse {
    let mut engine = state.engine.lock().unwrap();
    let net = req.l1_network.trim().to_ascii_lowercase();
    if net != "testnet" && net != "mainnet" && net != "regtest" {
        return RecordFundingResponse {
            ok: false,
            funding: None,
            error: Some("l1_network must be 'testnet', 'mainnet', or 'regtest'".to_string()),
            state: engine.to_api_state(),
        };
    }
    match engine.record_l1_funding(net, req.l1_txid, req.amount_love, req.note.trim().to_string(), unix_now()) {
        Ok(rec) => {
            if let Err(e) = save_state_atomic(&state.state_file, &engine.to_persisted()) {
                return RecordFundingResponse {
                    ok: false,
                    funding: Some(rec),
                    error: Some(format!("failed to save state: {e}")),
                    state: engine.to_api_state(),
                };
            }
            RecordFundingResponse {
                ok: true,
                funding: Some(rec),
                error: None,
                state: engine.to_api_state(),
            }
        }
        Err(err) => RecordFundingResponse {
            ok: false,
            funding: None,
            error: Some(err),
            state: engine.to_api_state(),
        },
    }
}

#[derive(Debug, Deserialize)]
struct ConfirmAnchorRequest {
    anchor_hash: String,
    l1_txid: String,
}

#[derive(Serialize)]
struct ConfirmAnchorResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<AnchorReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn apply_l1_lock_verification(receipt: &mut AnchorReceipt, locks: &[L1LockRecord]) -> Option<L1LockRecord> {
    let matching = locks
        .iter()
        .filter(|l| l.found && l.hash.eq_ignore_ascii_case(&receipt.anchor_hash))
        .max_by_key(|l| (l.confirmed as u8, l.confirmations, l.at))
        .cloned()?;

    receipt.l1_lock_id = Some(matching.id.clone());
    receipt.l1_confirmed_height = Some(matching.at);
    receipt.l1_confirmations = Some(matching.confirmations);
    receipt.l1_lock_state = Some(matching.state.clone());
    receipt.l1_verified_at_unix = Some(unix_now());
    if matching.confirmed {
        receipt.verified_on_l1 = true;
        receipt.broadcast_status = "broadcast".to_string();
        receipt.mode = "l1-verified-on-chain".to_string();
    }
    Some(matching)
}

fn handle_confirm_anchor(state: &AppState, req: ConfirmAnchorRequest) -> ConfirmAnchorResponse {
    let txid_bytes = match crate::crypto::hash32_from_hex(&req.l1_txid) {
        Ok(b) => b,
        Err(e) => {
            let engine = state.engine.lock().unwrap();
            return ConfirmAnchorResponse {
                ok: false,
                receipt: None,
                error: Some(format!("l1_txid must be a valid 64-char hex hash: {e}")),
                state: engine.to_api_state(),
            };
        }
    };
    let normalized_txid = hex::encode(txid_bytes);
    let target = req.anchor_hash.trim().to_ascii_lowercase();

    let receipt_network = {
        let engine = state.engine.lock().unwrap();
        engine
            .anchor_receipts
            .iter()
            .find(|r| r.anchor_hash.eq_ignore_ascii_case(&target))
            .map(|r| r.network.clone())
    };
    let Some(network) = receipt_network else {
        let engine = state.engine.lock().unwrap();
        return ConfirmAnchorResponse {
            ok: false,
            receipt: None,
            error: Some(format!("anchor_hash '{target}' not found")),
            state: engine.to_api_state(),
        };
    };

    // Best-effort live L1 lock verification outside the engine mutex
    let live_locks = state
        .client_for_network(&network)
        .and_then(|c| c.fetch_locks_for_hash(&target).ok());

    let mut engine = state.engine.lock().unwrap();
    let Some(receipt) = engine
        .anchor_receipts
        .iter_mut()
        .find(|r| r.anchor_hash.eq_ignore_ascii_case(&target))
    else {
        return ConfirmAnchorResponse {
            ok: false,
            receipt: None,
            error: Some(format!("anchor_hash '{target}' not found")),
            state: engine.to_api_state(),
        };
    };

    receipt.l1_txid = Some(normalized_txid);
    receipt.broadcast_status = "broadcast".to_string();
    receipt.mode = "l1-broadcast-confirmed".to_string();
    if let Some(ref locks) = live_locks {
        let _ = apply_l1_lock_verification(receipt, locks);
    }
    let updated = receipt.clone();
    if let Err(e) = save_state_atomic(&state.state_file, &engine.to_persisted()) {
        return ConfirmAnchorResponse {
            ok: false,
            receipt: Some(updated),
            error: Some(format!("failed to save state: {e}")),
            state: engine.to_api_state(),
        };
    }

    ConfirmAnchorResponse {
        ok: true,
        receipt: Some(updated),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct VerifyAnchorRequest {
    anchor_hash: Option<String>,
}

#[derive(Serialize)]
struct VerifyAnchorResponse {
    ok: bool,
    verified_on_l1: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock: Option<L1LockRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<AnchorReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_verify_anchor(state: &AppState, req: VerifyAnchorRequest) -> VerifyAnchorResponse {
    let target_info = {
        let engine = state.engine.lock().unwrap();
        let found = match req.anchor_hash.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(h) => engine
                .anchor_receipts
                .iter()
                .find(|r| r.anchor_hash.eq_ignore_ascii_case(h)),
            None => engine.anchor_receipts.last(),
        };
        found.map(|r| (r.anchor_hash.clone(), r.network.clone()))
    };

    let Some((anchor_hash, network)) = target_info else {
        let engine = state.engine.lock().unwrap();
        return VerifyAnchorResponse {
            ok: false,
            verified_on_l1: false,
            lock: None,
            receipt: None,
            error: Some("No matching anchor receipt found to verify".to_string()),
            state: engine.to_api_state(),
        };
    };

    let Some(client) = state.client_for_network(&network) else {
        let engine = state.engine.lock().unwrap();
        return VerifyAnchorResponse {
            ok: false,
            verified_on_l1: false,
            lock: None,
            receipt: None,
            error: Some(format!("No live L1 seed endpoint configured for '{network}'")),
            state: engine.to_api_state(),
        };
    };

    let locks = match client.fetch_locks_for_hash(&anchor_hash) {
        Ok(l) => l,
        Err(e) => {
            let engine = state.engine.lock().unwrap();
            return VerifyAnchorResponse {
                ok: false,
                verified_on_l1: false,
                lock: None,
                receipt: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };

    let mut engine = state.engine.lock().unwrap();
    let Some(receipt) = engine
        .anchor_receipts
        .iter_mut()
        .find(|r| r.anchor_hash.eq_ignore_ascii_case(&anchor_hash))
    else {
        return VerifyAnchorResponse {
            ok: false,
            verified_on_l1: false,
            lock: None,
            receipt: None,
            error: Some("Anchor receipt disappeared during verification".to_string()),
            state: engine.to_api_state(),
        };
    };

    let matched_lock = apply_l1_lock_verification(receipt, &locks);
    let verified_on_l1 = receipt.verified_on_l1;
    let updated_receipt = receipt.clone();
    if matched_lock.is_some() {
        let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
    }

    let err = if matched_lock.is_none() {
        Some(format!(
            "No on-chain hash-lock found yet on {network} for anchor_hash {anchor_hash}"
        ))
    } else {
        None
    };

    VerifyAnchorResponse {
        ok: true,
        verified_on_l1,
        lock: matched_lock,
        receipt: Some(updated_receipt),
        error: err,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Deserialize)]
struct AcquireParcelRequest {
    parcel_id: u64,
    acres: f64,
    price_love: u64,
    deed_label: String,
    signature_scheme: Option<String>,
}

#[derive(Serialize)]
struct AcquireParcelResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    txid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parcel: Option<ParcelRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stark_proof: Option<StarkProofArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_acquire_parcel(state: &AppState, req: AcquireParcelRequest) -> AcquireParcelResponse {
    let mut engine = state.engine.lock().unwrap();
    if req.parcel_id == 0 {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("parcel_id must be greater than 0".to_string()),
            state: engine.to_api_state(),
        };
    }
    if !req.acres.is_finite() || req.acres <= 0.0 {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("acres must be greater than 0".to_string()),
            state: engine.to_api_state(),
        };
    }
    let deed_label = req.deed_label.trim().to_string();
    if deed_label.is_empty() {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("deed_label is required".to_string()),
            state: engine.to_api_state(),
        };
    }
    let parcel_id = req.parcel_id;
    let acres = req.acres;
    let price_love = req.price_love;
    let scheme = normalize_scheme(req.signature_scheme.as_deref());

    let input_ref = match engine.ensure_single_covenant_utxo() {
        Ok(r) => r,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };

    let input_amount = engine
        .ledger
        .utxos
        .get(&input_ref)
        .map(|u| u.amount)
        .unwrap_or(0);
    if price_love == 0 || price_love > input_amount {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some(format!(
                "Invalid parcel price {price_love} LOVE (available treasury: {input_amount} LOVE)"
            )),
            state: engine.to_api_state(),
        };
    }

    let prev_state_root_hex = hex::encode(engine.ledger.state_root);
    let deed_doc_hash = sha256_concat(&[
        b"COUNTY_DEED_PERPETUAL_PURPOSE_TRUST_V2_1",
        &parcel_id.to_be_bytes(),
        deed_label.as_bytes(),
        &engine.ledger.tx_nonce.to_be_bytes(),
    ]);

    let (proof, leaf_index, slh_nonce) = {
        let Some(keys) = engine.signing_keys.as_mut() else {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(
                    "Server is in --verify-only mode (no private deed oracle keys loaded); submit offline-signed packets to POST /api/deeds"
                        .to_string(),
                ),
                state: engine.to_api_state(),
            };
        };
        match scheme.as_str() {
            "slh_dsa" => {
                let nonce = keys.deed_oracle_slh_next_nonce;
                let msg = CheckedWorkProof::deed_msg_with_nonce(parcel_id, deed_doc_hash, nonce);
                let sig = keys.deed_oracle_slh.sign_hash(msg);
                (
                    CheckedWorkProof::slh_dsa_land_deed(
                        parcel_id,
                        deed_doc_hash,
                        nonce,
                        keys.deed_oracle_slh.pk.clone(),
                        sig,
                    ),
                    None,
                    Some(nonce),
                )
            }
            _ => {
                let msg = CheckedWorkProof::deed_msg(parcel_id, deed_doc_hash);
                let sig = match keys.deed_oracle_xmss.sign_next(msg) {
                    Ok(s) => s,
                    Err(e) => {
                        return AcquireParcelResponse {
                            ok: false,
                            txid: None,
                            parcel: None,
                            stark_proof: None,
                            error: Some(e.to_string()),
                            state: engine.to_api_state(),
                        };
                    }
                };
                let idx = sig.leaf_index;
                (
                    CheckedWorkProof::merkle_land_deed(parcel_id, deed_doc_hash, sig),
                    Some(idx),
                    None,
                )
            }
        }
    };

    let rule_clone = engine.rule.clone();
    let recipient_tag = format!("PARCEL_{parcel_id}_SELLER");
    let spend_res = engine.ledger.spend_covenant(
        input_ref,
        &rule_clone,
        price_love,
        &recipient_tag,
        Some(rule_clone.rule_hash()),
        proof,
    );

    let (txid, _) = match spend_res {
        Ok(res) => res,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(e.to_string()),
                state: engine.to_api_state(),
            };
        }
    };

    if scheme == "slh_dsa" {
        if let Some(ref mut keys) = engine.signing_keys {
            keys.deed_oracle_slh_next_nonce += 1;
        }
    }

    let change_amount = input_amount - price_love;
    let rule_hash_hex = hex::encode(rule_clone.rule_hash());
    let stark_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::LandDeed,
        label: format!("Acquire Parcel #{parcel_id} ({price_love} LOVE, {scheme})"),
        tx_nonce: engine.ledger.tx_nonce,
        covenant_rule_hash: rule_hash_hex.clone(),
        change_covenant_rule_hash: rule_hash_hex,
        prev_state_root: prev_state_root_hex,
        new_state_root: hex::encode(engine.ledger.state_root),
        input_amount,
        payout_amount: price_love,
        change_amount,
        max_bounty_payout: rule_clone.max_bounty_payout,
        reinvest_bps: rule_clone.reinvest_bps,
        min_soil_health_score: rule_clone.min_soil_health_score,
        soil_health_score: 0,
    };

    let stark_proof = match prove_covenant_transition(stark_inputs) {
        Ok(p) => p,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: Some(hex::encode(txid)),
                parcel: None,
                stark_proof: None,
                error: Some(format!("STARK proof generation failed: {e}")),
                state: engine.to_api_state(),
            };
        }
    };

    let parcel = ParcelRecord {
        parcel_id,
        acres,
        price_love,
        deed_label,
        deed_document_hash: hex::encode(deed_doc_hash),
        txid: hex::encode(txid),
        signature_scheme: scheme,
        leaf_index,
        slh_nonce,
        stark_proof_id: stark_proof.proof_id.clone(),
        acquired_at_unix: unix_now(),
    };

    engine.parcels.push(parcel.clone());
    engine.stark_proofs.push(stark_proof.clone());
    engine.record_telemetry_series_point(format!("Acquire Parcel #{parcel_id}"));

    if let Err(e) = save_state_atomic(&state.state_file, &engine.to_persisted()) {
        return AcquireParcelResponse {
            ok: false,
            txid: Some(hex::encode(txid)),
            parcel: Some(parcel),
            stark_proof: Some(stark_proof),
            error: Some(format!("failed to save state: {e}")),
            state: engine.to_api_state(),
        };
    }

    AcquireParcelResponse {
        ok: true,
        txid: Some(hex::encode(txid)),
        parcel: Some(parcel),
        stark_proof: Some(stark_proof),
        error: None,
        state: engine.to_api_state(),
    }
}

fn handle_external_deed(state: &AppState, pkt: ExternalDeedPacket) -> AcquireParcelResponse {
    let mut engine = state.engine.lock().unwrap();
    if pkt.parcel_id == 0 {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("parcel_id must be greater than 0".to_string()),
            state: engine.to_api_state(),
        };
    }
    if !pkt.acres.is_finite() || pkt.acres <= 0.0 {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("acres must be greater than 0".to_string()),
            state: engine.to_api_state(),
        };
    }
    let deed_label = pkt.deed_label.trim().to_string();
    if deed_label.is_empty() {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some("deed_label is required".to_string()),
            state: engine.to_api_state(),
        };
    }
    let (deed_doc_hash, proof) = match pkt.to_checked_work_proof() {
        Ok(v) => v,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };

    let input_ref = match engine.ensure_single_covenant_utxo() {
        Ok(r) => r,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };
    let input_amount = engine
        .ledger
        .utxos
        .get(&input_ref)
        .map(|u| u.amount)
        .unwrap_or(0);
    if pkt.price_love == 0 || pkt.price_love > input_amount {
        return AcquireParcelResponse {
            ok: false,
            txid: None,
            parcel: None,
            stark_proof: None,
            error: Some(format!(
                "Invalid parcel price {} LOVE (available treasury: {input_amount} LOVE)",
                pkt.price_love
            )),
            state: engine.to_api_state(),
        };
    }

    let prev_state_root_hex = hex::encode(engine.ledger.state_root);
    let rule_clone = engine.rule.clone();
    let recipient_tag = format!("PARCEL_{}_SELLER", pkt.parcel_id);
    let (txid, _) = match engine.ledger.spend_covenant(
        input_ref,
        &rule_clone,
        pkt.price_love,
        &recipient_tag,
        Some(rule_clone.rule_hash()),
        proof,
    ) {
        Ok(res) => res,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: None,
                parcel: None,
                stark_proof: None,
                error: Some(e.to_string()),
                state: engine.to_api_state(),
            };
        }
    };

    let leaf_index = pkt.merkle_sig.as_ref().map(|s| s.leaf_index);
    let slh_nonce = pkt.slh_dsa_nonce;
    if let Some(ref mut keys) = engine.signing_keys {
        if let Some(idx) = leaf_index {
            let target_next = idx.saturating_add(1);
            if keys.deed_oracle_xmss.next_index() < target_next {
                let delta = target_next - keys.deed_oracle_xmss.next_index();
                let _ = keys.deed_oracle_xmss.skip_leaves(delta);
            }
        }
        if let Some(n) = slh_nonce {
            let target_next = n.saturating_add(1);
            if keys.deed_oracle_slh_next_nonce < target_next {
                keys.deed_oracle_slh_next_nonce = target_next;
            }
        }
    }

    let change_amount = input_amount - pkt.price_love;
    let rule_hash_hex = hex::encode(rule_clone.rule_hash());
    let stark_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::LandDeed,
        label: format!(
            "External Deed Parcel #{} ({} LOVE, {})",
            pkt.parcel_id, pkt.price_love, pkt.signature_scheme
        ),
        tx_nonce: engine.ledger.tx_nonce,
        covenant_rule_hash: rule_hash_hex.clone(),
        change_covenant_rule_hash: rule_hash_hex,
        prev_state_root: prev_state_root_hex,
        new_state_root: hex::encode(engine.ledger.state_root),
        input_amount,
        payout_amount: pkt.price_love,
        change_amount,
        max_bounty_payout: rule_clone.max_bounty_payout,
        reinvest_bps: rule_clone.reinvest_bps,
        min_soil_health_score: rule_clone.min_soil_health_score,
        soil_health_score: 0,
    };

    let stark_proof = match prove_covenant_transition(stark_inputs) {
        Ok(p) => p,
        Err(e) => {
            return AcquireParcelResponse {
                ok: false,
                txid: Some(hex::encode(txid)),
                parcel: None,
                stark_proof: None,
                error: Some(format!("STARK proof generation failed: {e}")),
                state: engine.to_api_state(),
            };
        }
    };

    let parcel = ParcelRecord {
        parcel_id: pkt.parcel_id,
        acres: pkt.acres,
        price_love: pkt.price_love,
        deed_label,
        deed_document_hash: hex::encode(deed_doc_hash),
        txid: hex::encode(txid),
        signature_scheme: pkt.signature_scheme,
        leaf_index,
        slh_nonce,
        stark_proof_id: stark_proof.proof_id.clone(),
        acquired_at_unix: unix_now(),
    };

    engine.parcels.push(parcel.clone());
    engine.stark_proofs.push(stark_proof.clone());
    engine.record_telemetry_series_point(format!("External Deed Parcel #{}", pkt.parcel_id));
    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());

    AcquireParcelResponse {
        ok: true,
        txid: Some(hex::encode(txid)),
        parcel: Some(parcel),
        stark_proof: Some(stark_proof),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Deserialize)]
struct TelemetryActionRequest {
    parcel_id: Option<u64>,
    kwh_delivered: u64,
    soil_health_score: u16,
    payout_love: Option<u64>,
    device_id: Option<String>,
    signature_scheme: Option<String>,
    simulate_attack: Option<String>,
}

#[derive(Serialize)]
struct TelemetryActionResponse {
    ok: bool,
    blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    packet: Option<TelemetryRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stark_proof: Option<StarkProofArtifact>,
    state: ApiStateView,
}

fn handle_simulated_telemetry(
    state: &AppState,
    req: TelemetryActionRequest,
) -> TelemetryActionResponse {
    let mut engine = state.engine.lock().unwrap();
    let attack = req
        .simulate_attack
        .as_deref()
        .unwrap_or("none")
        .trim()
        .to_ascii_lowercase();
    let Some(parcel_id) = req.parcel_id.filter(|&id| id > 0) else {
        return TelemetryActionResponse {
            ok: false,
            blocked: true,
            error: Some("parcel_id is required and must be > 0".to_string()),
            packet: None,
            stark_proof: None,
            state: engine.to_api_state(),
        };
    };
    let Some(device_id) = req.device_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
        return TelemetryActionResponse {
            ok: false,
            blocked: true,
            error: Some("device_id is required".to_string()),
            packet: None,
            stark_proof: None,
            state: engine.to_api_state(),
        };
    };
    let Some(mut payout_love) = req.payout_love else {
        return TelemetryActionResponse {
            ok: false,
            blocked: true,
            error: Some("payout_love is required".to_string()),
            packet: None,
            stark_proof: None,
            state: engine.to_api_state(),
        };
    };
    let scheme = normalize_scheme(req.signature_scheme.as_deref());

    let rule_clone = engine.rule.clone();
    let kwh_delivered = req.kwh_delivered;
    let mut soil_health_score = req.soil_health_score;

    // Explicit red-team harness paths only when requested via `simulate_attack`
    if attack == "low_soil" && soil_health_score >= rule_clone.min_soil_health_score {
        soil_health_score = rule_clone.min_soil_health_score.saturating_sub(18);
    }
    if attack == "over_cap" && payout_love <= rule_clone.max_bounty_payout {
        payout_love = rule_clone.max_bounty_payout.saturating_add(5_000);
    }

    let input_ref = match engine.ensure_single_covenant_utxo() {
        Ok(r) => r,
        Err(e) => {
            return TelemetryActionResponse {
                ok: false,
                blocked: true,
                error: Some(e),
                packet: None,
                stark_proof: None,
                state: engine.to_api_state(),
            };
        }
    };
    let input_amount = engine
        .ledger
        .utxos
        .get(&input_ref)
        .map(|u| u.amount)
        .unwrap_or(0);

    if engine.signing_keys.is_none() {
        return TelemetryActionResponse {
            ok: false,
            blocked: true,
            error: Some(
                "Server is in --verify-only mode (no private hardware keys loaded); submit signed packets from newsatoshi-hw-signer to POST /api/telemetry"
                    .to_string(),
            ),
            packet: None,
            stark_proof: None,
            state: engine.to_api_state(),
        };
    }

    // Handle explicit replay attack cleanly without corrupting live signer cursors
    if attack == "replay_leaf" {
        let err_msg = match scheme.as_str() {
            "slh_dsa" => {
                let (commit, proof) = {
                    let keys = engine.signing_keys.as_ref().unwrap();
                    let commit = keys.hardware_slh.pk.commitment();
                    let replay_nonce = 0u64;
                    let msg = CheckedWorkProof::energy_msg_with_nonce(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        replay_nonce,
                    );
                    let sig = keys.hardware_slh.sign_hash(msg);
                    let proof = CheckedWorkProof::slh_dsa_energy_deployment(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        replay_nonce,
                        keys.hardware_slh.pk.clone(),
                        sig,
                    );
                    (commit, proof)
                };
                let safe_payout = payout_love.min(rule_clone.max_bounty_payout).min(input_amount).max(1);
                if engine.ledger.next_slh_nonce_watermark(commit) > 0 {
                    engine
                        .ledger
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            safe_payout,
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof,
                        )
                        .expect_err("live SLH-DSA nonce replay must fail")
                        .to_string()
                } else {
                    let mut sandbox = engine.ledger.clone();
                    let (_, ch) = sandbox
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            safe_payout.min(input_amount / 2).max(1),
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof.clone(),
                        )
                        .expect("initial sandbox spend succeeds");
                    let next_ref = ch.unwrap_or(input_ref);
                    sandbox
                        .spend_covenant(
                            next_ref,
                            &rule_clone,
                            safe_payout.min(input_amount / 2).max(1),
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof,
                        )
                        .expect_err("replayed SLH-DSA nonce must fail")
                        .to_string()
                }
            }
            _ => {
                let (root, proof) = {
                    let keys = engine.signing_keys.as_ref().unwrap();
                    let root = keys.hardware_xmss.root();
                    let msg = CheckedWorkProof::energy_msg(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                    );
                    let replay_sig = keys
                        .hardware_xmss
                        .sign_at_index_unchecked(0, msg)
                        .expect("leaf 0 exists");
                    let proof = CheckedWorkProof::merkle_energy_deployment(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        replay_sig,
                    );
                    (root, proof)
                };
                let safe_payout = payout_love.min(rule_clone.max_bounty_payout).min(input_amount).max(1);
                if engine.ledger.next_merkle_leaf_watermark(root) > 0 {
                    engine
                        .ledger
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            safe_payout,
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof,
                        )
                        .expect_err("live Merkle leaf #0 replay must fail")
                        .to_string()
                } else {
                    let mut sandbox = engine.ledger.clone();
                    let (_, ch) = sandbox
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            safe_payout.min(input_amount / 2).max(1),
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof.clone(),
                        )
                        .expect("initial sandbox spend succeeds");
                    let next_ref = ch.unwrap_or(input_ref);
                    sandbox
                        .spend_covenant(
                            next_ref,
                            &rule_clone,
                            safe_payout.min(input_amount / 2).max(1),
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof,
                        )
                        .expect_err("replayed Merkle leaf #0 must fail")
                        .to_string()
                }
            }
        };

        let record = TelemetryRecord {
            id: format!("tel-blk-{}", engine.telemetry_history.len() + 1),
            device_id,
            parcel_id,
            kwh_delivered,
            soil_health_score,
            payout_love,
            signature_scheme: scheme.clone(),
            leaf_index: if scheme == "xmss" { Some(0) } else { None },
            slh_nonce: if scheme == "slh_dsa" { Some(0) } else { None },
            blocked: true,
            attack_type: Some("replay_leaf".to_string()),
            error: Some(err_msg.clone()),
            txid: None,
            stark_proof_id: None,
            timestamp_unix: unix_now(),
        };
        engine.telemetry_history.push(record.clone());
        let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
        return TelemetryActionResponse {
            ok: false,
            blocked: true,
            error: Some(err_msg),
            packet: Some(record),
            stark_proof: None,
            state: engine.to_api_state(),
        };
    }

    // Build proof without prematurely advancing XMSS leaf cursor if the transaction rejects
    let (proof, leaf_index, slh_nonce) = {
        let keys = engine.signing_keys.as_ref().unwrap();
        match scheme.as_str() {
            "slh_dsa" => {
                let nonce = keys.hardware_slh_next_nonce;
                let msg = CheckedWorkProof::energy_msg_with_nonce(
                    parcel_id,
                    kwh_delivered,
                    soil_health_score,
                    nonce,
                );
                let sig = keys.hardware_slh.sign_hash(msg);
                (
                    CheckedWorkProof::slh_dsa_energy_deployment(
                        parcel_id,
                        kwh_delivered,
                        soil_health_score,
                        nonce,
                        keys.hardware_slh.pk.clone(),
                        sig,
                    ),
                    None,
                    Some(nonce),
                )
            }
            _ => {
                let idx = keys.hardware_xmss.next_index();
                let msg = CheckedWorkProof::energy_msg(parcel_id, kwh_delivered, soil_health_score);
                let sig = match keys.hardware_xmss.sign_at_index_unchecked(idx, msg) {
                    Ok(s) => s,
                    Err(e) => {
                        return TelemetryActionResponse {
                            ok: false,
                            blocked: true,
                            error: Some(e.to_string()),
                            packet: None,
                            stark_proof: None,
                            state: engine.to_api_state(),
                        };
                    }
                };
                (
                    CheckedWorkProof::merkle_energy_deployment(
                        parcel_id,
                        kwh_delivered,
                        soil_health_score,
                        sig,
                    ),
                    Some(idx),
                    None,
                )
            }
        }
    };

    let proposed_change_covenant = if attack == "strip_covenant" {
        // Ensure change > 0 so covenant-stripping check fires cleanly
        if payout_love >= input_amount && input_amount > 1 {
            payout_love = (input_amount / 2).min(rule_clone.max_bounty_payout);
        }
        None
    } else {
        Some(rule_clone.rule_hash())
    };

    let prev_state_root_hex = hex::encode(engine.ledger.state_root);
    let spend_res = engine.ledger.spend_covenant(
        input_ref,
        &rule_clone,
        payout_love,
        &device_id,
        proposed_change_covenant,
        proof,
    );

    match spend_res {
        Err(err_msg) => {
            let attack_tag = if attack == "none" {
                None
            } else {
                Some(attack)
            };
            let record = TelemetryRecord {
                id: format!("tel-blk-{}", engine.telemetry_history.len() + 1),
                device_id,
                parcel_id,
                kwh_delivered,
                soil_health_score,
                payout_love,
                signature_scheme: scheme,
                leaf_index,
                slh_nonce,
                blocked: true,
                attack_type: attack_tag,
                error: Some(err_msg.to_string()),
                txid: None,
                stark_proof_id: None,
                timestamp_unix: unix_now(),
            };
            engine.telemetry_history.push(record.clone());
            let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
            TelemetryActionResponse {
                ok: false,
                blocked: true,
                error: Some(err_msg.to_string()),
                packet: Some(record),
                stark_proof: None,
                state: engine.to_api_state(),
            }
        }
        Ok((txid, _)) => {
            // Advance local signer cursor now that ledger committed the spend
            if let Some(ref mut keys) = engine.signing_keys {
                if scheme == "slh_dsa" {
                    keys.hardware_slh_next_nonce += 1;
                } else {
                    let _ = keys.hardware_xmss.skip_leaves(1);
                }
            }

            let change_amount = input_amount - payout_love;
            let rule_hash_hex = hex::encode(rule_clone.rule_hash());
            let stark_inputs = CovenantPublicInputs {
                tx_kind: CovenantTxKind::TelemetryBounty,
                label: format!(
                    "Telemetry {kwh_delivered} kWh / Soil {soil_health_score} ({scheme})"
                ),
                tx_nonce: engine.ledger.tx_nonce,
                covenant_rule_hash: rule_hash_hex.clone(),
                change_covenant_rule_hash: rule_hash_hex,
                prev_state_root: prev_state_root_hex,
                new_state_root: hex::encode(engine.ledger.state_root),
                input_amount,
                payout_amount: payout_love,
                change_amount,
                max_bounty_payout: rule_clone.max_bounty_payout,
                reinvest_bps: rule_clone.reinvest_bps,
                min_soil_health_score: rule_clone.min_soil_health_score,
                soil_health_score,
            };

            let stark_proof = match prove_covenant_transition(stark_inputs) {
                Ok(p) => p,
                Err(e) => {
                    return TelemetryActionResponse {
                        ok: false,
                        blocked: false,
                        error: Some(format!("STARK proving failed: {e}")),
                        packet: None,
                        stark_proof: None,
                        state: engine.to_api_state(),
                    };
                }
            };

            let record = TelemetryRecord {
                id: format!("tel-{}", engine.telemetry_history.len() + 1),
                device_id,
                parcel_id,
                kwh_delivered,
                soil_health_score,
                payout_love,
                signature_scheme: scheme,
                leaf_index,
                slh_nonce,
                blocked: false,
                attack_type: None,
                error: None,
                txid: Some(hex::encode(txid)),
                stark_proof_id: Some(stark_proof.proof_id.clone()),
                timestamp_unix: unix_now(),
            };
            engine.total_energy_bounty_love = engine
                .total_energy_bounty_love
                .saturating_add(payout_love);
            engine.telemetry_history.push(record.clone());
            engine.stark_proofs.push(stark_proof.clone());
            engine.record_telemetry_series_point(format!("Telemetry +{kwh_delivered} kWh"));

            let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
            TelemetryActionResponse {
                ok: true,
                blocked: false,
                error: None,
                packet: Some(record),
                stark_proof: Some(stark_proof),
                state: engine.to_api_state(),
            }
        }
    }
}

fn handle_external_telemetry(
    state: &AppState,
    pkt: ExternalTelemetryPacket,
) -> TelemetryActionResponse {
    let mut engine = state.engine.lock().unwrap();
    let proof = match pkt.to_checked_work_proof() {
        Ok(p) => p,
        Err(e) => {
            return TelemetryActionResponse {
                ok: false,
                blocked: true,
                error: Some(e),
                packet: None,
                stark_proof: None,
                state: engine.to_api_state(),
            };
        }
    };

    let input_ref = match engine.ensure_single_covenant_utxo() {
        Ok(r) => r,
        Err(e) => {
            return TelemetryActionResponse {
                ok: false,
                blocked: true,
                error: Some(e),
                packet: None,
                stark_proof: None,
                state: engine.to_api_state(),
            };
        }
    };
    let input_amount = engine
        .ledger
        .utxos
        .get(&input_ref)
        .map(|u| u.amount)
        .unwrap_or(0);

    let rule_clone = engine.rule.clone();
    let prev_state_root_hex = hex::encode(engine.ledger.state_root);
    let leaf_index = pkt.merkle_sig.as_ref().map(|s| s.leaf_index);
    let slh_nonce = pkt.slh_dsa_nonce;

    match engine.ledger.spend_covenant(
        input_ref,
        &rule_clone,
        pkt.payout_love,
        &pkt.device_id,
        Some(rule_clone.rule_hash()),
        proof,
    ) {
        Err(err_msg) => {
            let record = TelemetryRecord {
                id: format!("hw-blk-{}", engine.telemetry_history.len() + 1),
                device_id: pkt.device_id,
                parcel_id: pkt.parcel_id,
                kwh_delivered: pkt.kwh_delivered,
                soil_health_score: pkt.soil_health_score,
                payout_love: pkt.payout_love,
                signature_scheme: pkt.signature_scheme,
                leaf_index,
                slh_nonce,
                blocked: true,
                attack_type: Some("external_rejected".to_string()),
                error: Some(err_msg.to_string()),
                txid: None,
                stark_proof_id: None,
                timestamp_unix: pkt.timestamp_unix,
            };
            engine.telemetry_history.push(record.clone());
            let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
            TelemetryActionResponse {
                ok: false,
                blocked: true,
                error: Some(err_msg.to_string()),
                packet: Some(record),
                stark_proof: None,
                state: engine.to_api_state(),
            }
        }
        Ok((txid, _)) => {
            // Keep internal simulator cursors monotonically synchronized with external device (if loaded)
            if let Some(ref mut keys) = engine.signing_keys {
                if let Some(idx) = leaf_index {
                    let target_next = idx.saturating_add(1);
                    if keys.hardware_xmss.next_index() < target_next {
                        let delta = target_next - keys.hardware_xmss.next_index();
                        let _ = keys.hardware_xmss.skip_leaves(delta);
                    }
                }
                if let Some(n) = slh_nonce {
                    let target_next = n.saturating_add(1);
                    if keys.hardware_slh_next_nonce < target_next {
                        keys.hardware_slh_next_nonce = target_next;
                    }
                }
            }

            let change_amount = input_amount - pkt.payout_love;
            let rule_hash_hex = hex::encode(rule_clone.rule_hash());
            let stark_inputs = CovenantPublicInputs {
                tx_kind: CovenantTxKind::TelemetryBounty,
                label: format!(
                    "HW Signer {} ({} kWh, soil {})",
                    pkt.device_id, pkt.kwh_delivered, pkt.soil_health_score
                ),
                tx_nonce: engine.ledger.tx_nonce,
                covenant_rule_hash: rule_hash_hex.clone(),
                change_covenant_rule_hash: rule_hash_hex,
                prev_state_root: prev_state_root_hex,
                new_state_root: hex::encode(engine.ledger.state_root),
                input_amount,
                payout_amount: pkt.payout_love,
                change_amount,
                max_bounty_payout: rule_clone.max_bounty_payout,
                reinvest_bps: rule_clone.reinvest_bps,
                min_soil_health_score: rule_clone.min_soil_health_score,
                soil_health_score: pkt.soil_health_score,
            };

            let stark_proof = match prove_covenant_transition(stark_inputs) {
                Ok(p) => p,
                Err(e) => {
                    return TelemetryActionResponse {
                        ok: false,
                        blocked: false,
                        error: Some(format!("STARK proving failed: {e}")),
                        packet: None,
                        stark_proof: None,
                        state: engine.to_api_state(),
                    };
                }
            };

            let record = TelemetryRecord {
                id: format!("hw-{}", engine.telemetry_history.len() + 1),
                device_id: pkt.device_id,
                parcel_id: pkt.parcel_id,
                kwh_delivered: pkt.kwh_delivered,
                soil_health_score: pkt.soil_health_score,
                payout_love: pkt.payout_love,
                signature_scheme: pkt.signature_scheme,
                leaf_index,
                slh_nonce,
                blocked: false,
                attack_type: None,
                error: None,
                txid: Some(hex::encode(txid)),
                stark_proof_id: Some(stark_proof.proof_id.clone()),
                timestamp_unix: pkt.timestamp_unix,
            };
            engine.total_energy_bounty_love = engine
                .total_energy_bounty_love
                .saturating_add(pkt.payout_love);
            engine.telemetry_history.push(record.clone());
            engine.stark_proofs.push(stark_proof.clone());
            engine.record_telemetry_series_point(format!("HW Telemetry +{} kWh", pkt.kwh_delivered));
            let _ = save_state_atomic(&state.state_file, &engine.to_persisted());

            TelemetryActionResponse {
                ok: true,
                blocked: false,
                error: None,
                packet: Some(record),
                stark_proof: Some(stark_proof),
                state: engine.to_api_state(),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct StreamSettleRequest {
    ticks: u64,
    unit_price: u64,
    operator_tag: Option<String>,
}

#[derive(Serialize)]
struct StreamSettleResponse {
    ok: bool,
    gross_love: u64,
    reinvested_love: u64,
    operator_paid_love: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    stark_proof: Option<StarkProofArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_stream_settle(state: &AppState, req: StreamSettleRequest) -> StreamSettleResponse {
    let mut engine = state.engine.lock().unwrap();
    if req.ticks == 0 || req.unit_price == 0 || req.ticks > 50_000 {
        return StreamSettleResponse {
            ok: false,
            gross_love: 0,
            reinvested_love: 0,
            operator_paid_love: 0,
            stark_proof: None,
            error: Some("ticks must be 1..=50000 and unit_price > 0".to_string()),
            state: engine.to_api_state(),
        };
    }

    let Some(operator_tag) = req
        .operator_tag
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return StreamSettleResponse {
            ok: false,
            gross_love: 0,
            reinvested_love: 0,
            operator_paid_love: 0,
            stark_proof: None,
            error: Some("operator_tag is required".to_string()),
            state: engine.to_api_state(),
        };
    };

    let max_ticks = req.ticks.saturating_add(10);
    let seed = sha256_concat(&[
        b"NEWSATOSHI_PAYWORD_CHANNEL_V2_1",
        &engine.ledger.state_root,
        &req.ticks.to_be_bytes(),
        &req.unit_price.to_be_bytes(),
        operator_tag.as_bytes(),
    ]);
    let (channel, secret_chain) =
        StreamingChannel::create_with_secret_chain(seed, max_ticks, req.unit_price);
    let revealed_preimage = secret_chain[(max_ticks - req.ticks) as usize];

    let prev_state_root_hex = hex::encode(engine.ledger.state_root);
    let rule_clone = engine.rule.clone();
    let (reinvested_love, operator_paid_love) = match engine.ledger.settle_streaming_channel(
        &channel,
        req.ticks,
        revealed_preimage,
        &rule_clone,
        &operator_tag,
    ) {
        Ok(res) => res,
        Err(e) => {
            return StreamSettleResponse {
                ok: false,
                gross_love: 0,
                reinvested_love: 0,
                operator_paid_love: 0,
                stark_proof: None,
                error: Some(e.to_string()),
                state: engine.to_api_state(),
            };
        }
    };

    // Auto-consolidate covenant UTXOs so reinvested surplus is immediately spendable on Parcel #N
    let refs = engine.ledger.covenant_utxo_refs(&rule_clone);
    if refs.len() >= 2 {
        let _ = engine.ledger.consolidate_covenant_utxos(&refs, &rule_clone);
    }

    let gross_love = reinvested_love + operator_paid_love;
    let rule_hash_hex = hex::encode(rule_clone.rule_hash());
    let stark_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::StreamingSettlement,
        label: format!(
            "PayWord Stream {}x{} LOVE (+{} reinvested)",
            req.ticks, req.unit_price, reinvested_love
        ),
        tx_nonce: engine.ledger.tx_nonce,
        covenant_rule_hash: rule_hash_hex.clone(),
        change_covenant_rule_hash: rule_hash_hex,
        prev_state_root: prev_state_root_hex,
        new_state_root: hex::encode(engine.ledger.state_root),
        input_amount: gross_love,
        payout_amount: operator_paid_love,
        change_amount: reinvested_love,
        max_bounty_payout: rule_clone.max_bounty_payout,
        reinvest_bps: rule_clone.reinvest_bps,
        min_soil_health_score: rule_clone.min_soil_health_score,
        soil_health_score: 0,
    };

    let stark_proof = match prove_covenant_transition(stark_inputs) {
        Ok(p) => p,
        Err(e) => {
            return StreamSettleResponse {
                ok: false,
                gross_love,
                reinvested_love,
                operator_paid_love,
                stark_proof: None,
                error: Some(format!("STARK proving failed: {e}")),
                state: engine.to_api_state(),
            };
        }
    };

    engine.total_streaming_gross_love = engine.total_streaming_gross_love.saturating_add(gross_love);
    engine.total_streaming_reinvested_love = engine
        .total_streaming_reinvested_love
        .saturating_add(reinvested_love);

    let rec = StreamingRecord {
        id: format!("stream-{}", engine.streaming_history.len() + 1),
        ticks: req.ticks,
        unit_price: req.unit_price,
        gross_love,
        reinvested_love,
        operator_paid_love,
        operator_tag,
        channel_tip_hex: hex::encode(channel.tip),
        stark_proof_id: stark_proof.proof_id.clone(),
        timestamp_unix: unix_now(),
    };
    engine.streaming_history.push(rec);
    engine.stark_proofs.push(stark_proof.clone());
    engine.record_telemetry_series_point(format!("Stream +{reinvested_love} LOVE"));
    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());

    StreamSettleResponse {
        ok: true,
        gross_love,
        reinvested_love,
        operator_paid_love,
        stark_proof: Some(stark_proof),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct AnchorActionRequest {
    network: Option<String>,
}

#[derive(Serialize)]
struct AnchorActionResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<AnchorReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_anchor(state: &AppState, req: AnchorActionRequest) -> AnchorActionResponse {
    let network = req
        .network
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s == "mainnet" || s == "testnet")
        .unwrap_or_else(|| "testnet".to_string());

    let seed_snapshot = state.get_seed_snapshot(true);

    let mut engine = state.engine.lock().unwrap();
    let base_commitment = engine.ledger.build_anchor_commitment(&engine.rule);
    let bound_commitment = match network.as_str() {
        "mainnet" => match seed_snapshot.mainnet_status.as_ref() {
            Some(ms) => base_commitment.with_seed_status(ms),
            None => base_commitment.with_mainnet_genesis(),
        },
        _ => match seed_snapshot.status.as_ref() {
            Some(status) => base_commitment.with_seed_status(status),
            None => {
                return AnchorActionResponse {
                    ok: false,
                    receipt: None,
                    error: Some(format!(
                        "Cannot bind live testnet L1 tip — seed unreachable ({})",
                        seed_snapshot.error.as_deref().unwrap_or("connection failed")
                    )),
                    state: engine.to_api_state(),
                };
            }
        },
    };

    let rpc_endpoint = if network == "mainnet" {
        state
            .mainnet_seed_client
            .as_ref()
            .map(|c| c.endpoint_url.clone())
            .unwrap_or_else(|| "http://127.0.0.1:8332/".to_string())
    } else {
        state.seed_client.endpoint_url.clone()
    };
    let rpc_client = JsonRpcAnchorClient::new(rpc_endpoint, &network);
    let rpc_id = (engine.anchor_receipts.len() as u64) + 1;
    let receipt = rpc_client.submit_dry_run(&bound_commitment, rpc_id);

    engine.anchor_receipts.push(receipt.clone());
    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());

    AnchorActionResponse {
        ok: true,
        receipt: Some(receipt),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Deserialize)]
struct VerifyStarkRequest {
    proof_id: String,
}

#[derive(Serialize)]
struct VerifyStarkResponse {
    ok: bool,
    valid: bool,
    proof_id: String,
    verify_ms: u64,
    proof_bytes_len: usize,
    security_bits: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn handle_verify_stark(state: &AppState, proof_id: &str) -> VerifyStarkResponse {
    let artifact = {
        let engine = state.engine.lock().unwrap();
        engine
            .stark_proofs
            .iter()
            .find(|p| p.proof_id == proof_id)
            .cloned()
    };

    let Some(artifact) = artifact else {
        return VerifyStarkResponse {
            ok: false,
            valid: false,
            proof_id: proof_id.to_string(),
            verify_ms: 0,
            proof_bytes_len: 0,
            security_bits: 0,
            error: Some(format!("STARK proof '{proof_id}' not found")),
        };
    };

    match verify_covenant_stark(&artifact) {
        Ok(res) => VerifyStarkResponse {
            ok: true,
            valid: res.valid,
            proof_id: res.proof_id,
            verify_ms: res.verify_ms,
            proof_bytes_len: res.proof_bytes_len,
            security_bits: res.security_bits,
            error: res.error,
        },
        Err(e) => VerifyStarkResponse {
            ok: false,
            valid: false,
            proof_id: proof_id.to_string(),
            verify_ms: 0,
            proof_bytes_len: artifact.proof_bytes_len,
            security_bits: artifact.security_bits,
            error: Some(e),
        },
    }
}

#[derive(Serialize)]
struct ResetResponse {
    ok: bool,
    state: ApiStateView,
}

fn handle_reset(state: &AppState) -> ResetResponse {
    let mut engine = state.engine.lock().unwrap();
    let fresh = match &state.external_public_keys {
        Some(ext) => CommonsEngine::new_verify_only(ext).unwrap_or_else(|_| CommonsEngine::new_empty()),
        None => CommonsEngine::new_empty(),
    };
    *engine = fresh;
    if state.verify_only {
        engine.enable_verify_only_strip_keys();
    }
    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
    ResetResponse {
        ok: true,
        state: engine.to_api_state(),
    }
}

fn normalize_scheme(scheme: Option<&str>) -> String {
    match scheme.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("slh_dsa") | Some("slh-dsa") | Some("fips205") => "slh_dsa".to_string(),
        _ => "xmss".to_string(),
    }
}

fn serve_static_file(req: Request, static_dir: &Path, req_path: &str) {
    let Ok(root_canon) = fs::canonicalize(static_dir) else {
        respond_json(
            req,
            404,
            &json!({ "ok": false, "error": "static directory not found" }),
        );
        return;
    };

    let rel = req_path.trim_start_matches('/');
    let candidate = if rel.is_empty() {
        root_canon.join("index.html")
    } else {
        root_canon.join(rel)
    };

    let resolved = match fs::canonicalize(&candidate) {
        Ok(p) if p.starts_with(&root_canon) && p.is_file() => p,
        _ => {
            // SPA fallback to index.html for client-side routes
            let fallback = root_canon.join("index.html");
            if fallback.is_file() {
                fallback
            } else {
                respond_json(req, 404, &json!({ "ok": false, "error": "file not found" }));
                return;
            }
        }
    };

    match fs::read(&resolved) {
        Ok(bytes) => {
            let mime = content_type_for_path(&resolved);
            let len = bytes.len();
            let mut resp = Response::new(
                StatusCode(200),
                vec![Header::from_bytes("Content-Type", mime).unwrap()],
                Cursor::new(bytes),
                Some(len),
                None,
            );
            for h in cors_headers() {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        }
        Err(_) => {
            respond_json(
                req,
                500,
                &json!({ "ok": false, "error": "failed to read static asset" }),
            );
        }
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _unused_sha256_ref() -> [u8; 32] {
    sha256(b"")
}
