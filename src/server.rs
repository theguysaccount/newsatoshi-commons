use crate::agent::{
    build_http_402_challenge, compute_network_metrics, handle_mcp_jsonrpc, render_agent_card_json,
    render_agent_skills_index_json, render_api_catalog_json, render_llms_full_txt,
    render_llms_txt, render_mcp_manifest_json, render_mcp_server_card_json, render_og_image_svg,
    render_openapi_json, render_public_page, render_robots_txt, render_sitemap_xml,
    verify_payword_token, AGENT_SKILL_MARKDOWN, DEFAULT_PUBLIC_BASE_URL, INSTALL_SCRIPT_SH,
    LEAN_FORMAL_PROOF, WHITEPAPER_LATEX, WHITEPAPER_MARKDOWN,
};
use crate::backing::{fetch_live_backing_summary, BackingProofSummary, CANONICAL_BACKING_VAULT};
use crate::crypto::{sha256, sha256_concat};
use crate::crypto_tools::inspect_evm_contract;
use crate::hardware::{ExternalDeedPacket, ExternalTelemetryPacket};
use crate::ops_hub::{try_handle_ops_request, OpsHubManager};
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
use crate::swap::{
    compile_atomic_swap, verify_swap_on_chain, AtomicSwapContract, CompileSwapRequest,
    DualChainSwapVerification,
};
use crate::web4::{
    compile_web4_bounty, compile_web4_charter, render_erc8004_registration,
    render_gateway_erc8004, validate_web4_service, CreateBountyRequest, RegisterServiceRequest,
    SpawnCharterRequest,
};
use crate::x402::{
    authorize_dual_402, build_bridge_quote_response, build_x402_v2_challenge,
    render_well_known_x402, route_spec_bridge_quote, route_spec_compile_htlc,
    route_spec_evm_security_scan, route_spec_verify_payword,
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

struct CachedBacking {
    fetched_at: Instant,
    summary: BackingProofSummary,
}

struct AppState {
    engine: Mutex<CommonsEngine>,
    state_file: PathBuf,
    seed_client: SeedStatusClient,
    mainnet_seed_client: Option<SeedStatusClient>,
    seed_cache: Mutex<Option<CachedSeed>>,
    backing_cache: Mutex<Option<CachedBacking>>,
    static_dir: Option<PathBuf>,
    admin_token_digest: Option<[u8; 32]>,
    verify_only: bool,
    external_public_keys: Option<ExternalPublicKeysConfig>,
    ops_hub: OpsHubManager,
    started_at: Instant,
}

impl AppState {
    fn get_seed_snapshot(&self, force_refresh: bool) -> SeedSnapshot {
        let mut cache = self.seed_cache.lock().unwrap_or_else(|e| e.into_inner());
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

    fn get_backing_summary(&self, force_refresh: bool) -> BackingProofSummary {
        let cache = self.backing_cache.lock().unwrap_or_else(|e| e.into_inner());
        if !force_refresh {
            if let Some(ref c) = *cache {
                if c.fetched_at.elapsed() < Duration::from_secs(30) {
                    return c.summary.clone();
                }
            }
        }
        drop(cache);
        let seed = self.get_seed_snapshot(false);
        let api_state = {
            let engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            engine.to_api_state()
        };
        let summary = fetch_live_backing_summary(&api_state, &seed, false);
        let mut cache = self.backing_cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some(CachedBacking {
            fetched_at: Instant::now(),
            summary: summary.clone(),
        });
        summary
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
    let ops_hub = OpsHubManager::init(&config.state_file);
    let state = Arc::new(AppState {
        engine: Mutex::new(engine),
        state_file: config.state_file,
        seed_client: SeedStatusClient::new(config.seed_url, Duration::from_secs(5)),
        mainnet_seed_client,
        seed_cache: Mutex::new(None),
        backing_cache: Mutex::new(None),
        static_dir: config.static_dir,
        admin_token_digest,
        verify_only,
        external_public_keys: config.external_public_keys,
        ops_hub,
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
    let ops_hub = OpsHubManager::init(&config.state_file);
    let state = Arc::new(AppState {
        engine: Mutex::new(engine),
        state_file: config.state_file,
        seed_client: SeedStatusClient::new(config.seed_url, Duration::from_secs(5)),
        mainnet_seed_client,
        seed_cache: Mutex::new(None),
        backing_cache: Mutex::new(None),
        static_dir: config.static_dir,
        admin_token_digest,
        verify_only,
        external_public_keys: config.external_public_keys,
        ops_hub,
        started_at: Instant::now(),
    });

    for req in server.incoming_requests() {
        handle_request(req, &state);
    }

    Ok(())
}

/// Hard cap on inbound HTTP POST request body size (1 MiB) to prevent unauthenticated OOM DoS.
const MAX_HTTP_BODY_BYTES: u64 = 1_048_576;

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, Accept, Mcp-Session-Id, Payment-Signature, X-Payment, X-Newsatoshi-Payment, X-Newsatoshi-PayWord",
        )
        .unwrap(),
        Header::from_bytes(
            "Access-Control-Expose-Headers",
            "Payment-Required, Payment-Response, X-Payment-Response, WWW-Authenticate",
        )
        .unwrap(),
        Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap(),
        Header::from_bytes("X-Frame-Options", "DENY").unwrap(),
        Header::from_bytes("Referrer-Policy", "no-referrer").unwrap(),
    ]
}

fn respond_text(req: Request, status: u16, content_type: &str, body: String) {
    let mut resp = Response::from_data(body.into_bytes())
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .with_header(Header::from_bytes("Vary", "Accept").unwrap());
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn is_valid_host_header(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 128
        && h.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
        && !h.starts_with('.')
        && !h.contains("..")
}

fn infer_base_url(req: &Request) -> String {
    let host = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.as_str().trim())
        .filter(|h| is_valid_host_header(h));
    let proto = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Forwarded-Proto"))
        .map(|h| h.value.as_str().trim())
        .filter(|p| *p == "http" || *p == "https");
    match host {
        Some(h) => {
            let scheme = proto.unwrap_or_else(|| {
                if h.starts_with("127.0.0.1") || h.starts_with("localhost") {
                    "http"
                } else {
                    "https"
                }
            });
            format!("{scheme}://{h}")
        }
        None => DEFAULT_PUBLIC_BASE_URL.to_string(),
    }
}

fn wants_markdown(req: &Request) -> bool {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Accept"))
        .map(|h| {
            let v = h.value.as_str().to_ascii_lowercase();
            v.contains("text/markdown") && !v.contains("text/html")
        })
        .unwrap_or(false)
}

fn query_param(raw_url: &str, key: &str) -> Option<String> {
    let query = raw_url.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.replace("%2F", "/").replace("%2f", "/"));
            }
        }
    }
    None
}

fn respond_json<T: Serialize>(req: Request, status: u16, payload: &T) {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("X-API-Version", "2026-11").unwrap())
        .with_header(Header::from_bytes("RateLimit-Policy", "120;w=60").unwrap())
        .with_header(Header::from_bytes("RateLimit-Limit", "120").unwrap());
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn header_value_ci(req: &Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn respond_x402_challenge(req: Request, payment_required_b64: &str, body: &Value) {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(bytes)
        .with_status_code(StatusCode(402))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("Payment-Required", payment_required_b64).unwrap())
        .with_header(
            Header::from_bytes(
                "WWW-Authenticate",
                r#"NEWSATOSHI-402 scheme="payword-sha256", asset="LOVE", x402="v2", bridge_network="eip155:8453", bridge_asset="USDC""#,
            )
            .unwrap(),
        );
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn respond_x402_paid<T: Serialize>(req: Request, payload: &T, payment_response_b64: Option<&str>) {
    let bytes = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(bytes)
        .with_status_code(StatusCode(200))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
    if let Some(hdr) = payment_response_b64 {
        if let Ok(h1) = Header::from_bytes("Payment-Response", hdr) {
            resp = resp.with_header(h1);
        }
        if let Ok(h2) = Header::from_bytes("X-Payment-Response", hdr) {
            resp = resp.with_header(h2);
        }
    }
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn discovery_link_header() -> Header {
    Header::from_bytes(
        "Link",
        concat!(
            "</llms.txt>; rel=\"alternate\"; type=\"text/markdown\", ",
            "</openapi.json>; rel=\"service-desc\"; type=\"application/openapi+json\", ",
            "</.well-known/api-catalog>; rel=\"api-catalog\"; type=\"application/linkset+json\", ",
            "</.well-known/agent-card.json>; rel=\"describedby\"; type=\"application/json\", ",
            "</.well-known/x402>; rel=\"payment\"; type=\"application/json\", ",
            "</.well-known/mcp/server-card.json>; rel=\"https://modelcontextprotocol.io/rel/server-card\"; type=\"application/json\""
        ),
    )
    .unwrap()
}

fn respond_problem_404(req: Request, path: &str, base_url: &str) {
    if wants_markdown(&req) {
        let md = format!(
            "# 404 Not Found\n\n\
             The requested resource `{path}` does not exist on Newsatoshi Commons (`{base_url}`).\n\n\
             ## Recovery & Discovery Links\n\
             - [LLM & AI Agent Guide]({base_url}/llms.txt)\n\
             - [API Documentation]({base_url}/docs)\n\
             - [Developer Portal]({base_url}/developers)\n\
             - [OpenAPI 3.1 Schema]({base_url}/openapi.json)\n\
             - [Streamable HTTP MCP]({base_url}/mcp)\n\
             - [XML Sitemap]({base_url}/sitemap.xml)\n"
        );
        let mut resp = Response::from_string(md)
            .with_status_code(StatusCode(404))
            .with_header(Header::from_bytes("Content-Type", "text/markdown; charset=utf-8").unwrap())
            .with_header(Header::from_bytes("Vary", "Accept").unwrap())
            .with_header(discovery_link_header());
        for h in cors_headers() {
            resp = resp.with_header(h);
        }
        let _ = req.respond(resp);
        return;
    }

    let payload = json!({
        "type": "about:blank",
        "title": "Not Found",
        "status": 404,
        "detail": format!("Route not found: {path}"),
        "discovery": {
            "llms_txt": format!("{base_url}/llms.txt"),
            "docs": format!("{base_url}/docs"),
            "openapi": format!("{base_url}/openapi.json"),
            "api_catalog": format!("{base_url}/.well-known/api-catalog"),
            "mcp": format!("{base_url}/mcp"),
            "sitemap": format!("{base_url}/sitemap.xml")
        }
    });
    let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(body)
        .with_status_code(StatusCode(404))
        .with_header(
            Header::from_bytes("Content-Type", "application/problem+json; charset=utf-8").unwrap(),
        )
        .with_header(Header::from_bytes("Vary", "Accept").unwrap())
        .with_header(Header::from_bytes("X-API-Version", "2026-11").unwrap())
        .with_header(discovery_link_header());
    for h in cors_headers() {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn respond_json_with_ct<T: Serialize>(req: Request, status: u16, content_type: &str, payload: &T) {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap());
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
    use std::io::Read;
    if let Some(len) = req.body_length() {
        if (len as u64) > MAX_HTTP_BODY_BYTES {
            return Err(format!(
                "request body too large ({len} bytes > {MAX_HTTP_BODY_BYTES} byte limit)"
            ));
        }
    }
    let mut buf = Vec::new();
    req.as_reader()
        .take(MAX_HTTP_BODY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("failed to read HTTP request body: {e}"))?;
    if (buf.len() as u64) > MAX_HTTP_BODY_BYTES {
        return Err(format!(
            "request body exceeds maximum allowed size of {MAX_HTTP_BODY_BYTES} bytes"
        ));
    }
    String::from_utf8(buf).map_err(|_| "HTTP request body must be valid UTF-8".to_string())
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

fn handle_request(req: Request, state: &AppState) {
    let method = req.method().clone();
    let raw_url = req.url().to_string();
    let path = raw_url.split('?').next().unwrap_or(&raw_url).to_string();

    if method == Method::Options {
        respond_empty(req, 204);
        return;
    }

    let mut req = match try_handle_ops_request(
        req,
        &path,
        &state.ops_hub,
        || state.get_seed_snapshot(false),
        || state.get_backing_summary(false),
    ) {
        Some(r) => r,
        None => return,
    };

    if method == Method::Head {
        let head_ct = match path.as_str() {
            "/" | "/index.html" | "/whitepaper" | "/manifesto" | "/bounty" | "/nsusd" | "/l2" | "/docs" | "/developers" | "/about" | "/contact" | "/privacy" => {
                if wants_markdown(&req) {
                    Some("text/markdown; charset=utf-8")
                } else {
                    Some("text/html; charset=utf-8")
                }
            }
            "/og-image.svg" => Some("image/svg+xml"),
            "/whitepaper.pdf" => Some("application/pdf"),
            "/install.sh" => Some("text/x-shellscript; charset=utf-8"),
            "/sdk/newsatoshi-402.mjs" => Some("application/javascript; charset=utf-8"),
            "/sdk/newsatoshi-402.py" => Some("text/x-python; charset=utf-8"),
            "/robots.txt"
            | "/.well-known/mcp-registry-auth"
            | "/.well-known/402index-verify.txt"
            | "/formal/NewsatoshiCovenants.lean"
            | "/formal/NsUsdAndL2Perfection.lean"
            | "/whitepaper.tex" => {
                Some("text/plain; charset=utf-8")
            }
            "/sitemap.xml" => Some("application/xml; charset=utf-8"),
            "/llms.txt" | "/llms-full.txt" | "/whitepaper.md" | "/whitepaper-v2.1.md" | "/manifesto.md" | "/bounty.md" | "/nsusd.md" | "/l2.md" | "/skills/newsatoshi-agent-payments/SKILL.md" => {
                Some("text/markdown; charset=utf-8")
            }
            "/.well-known/api-catalog" => Some("application/linkset+json; charset=utf-8"),
            "/.well-known/agent.json"
            | "/.well-known/agent-card.json"
            | "/.well-known/erc8004.json"
            | "/.well-known/x402.json"
            | "/.well-known/x402"
            | "/.well-known/mcp.json"
            | "/.well-known/mcp/server-card.json"
            | "/.well-known/agent-skills/index.json"
            | "/server.json"
            | "/openapi.json"
            | "/mcp"
            | "/api/health"
            | "/api/state"
            | "/api/seed"
            | "/api/network/metrics"
            | "/api/backing/summary"
            | "/api/v2/nsusd/quote"
            | "/api/v2/nsusd/state"
            | "/api/v2/l2/status"
            | "/api/v2/x402/paywall-kit"
            | "/api/v2/paywords/cost-savings"
            | "/api/v2/paywords/channel-kit"
            | "/api/v2/paywords/try-402"
            | "/api/v2/crosschain/connectivity"
            | "/api/v2/launchpad/fair-curve"
            | "/api/v2/base/curves"
            | "/api/v2/base/channel"
            | "/api/web4/charters"
            | "/api/web4/services"
            | "/api/web4/bounties"
            | "/api/swaps" => Some("application/json; charset=utf-8"),
            _ => None,
        };
        if let Some(ct) = head_ct {
            let mut resp = Response::empty(StatusCode(200))
                .with_header(Header::from_bytes("Content-Type", ct).unwrap())
                .with_header(Header::from_bytes("Vary", "Accept").unwrap())
                .with_header(discovery_link_header());
            for h in cors_headers() {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        } else {
            let mut resp = Response::empty(StatusCode(404)).with_header(discovery_link_header());
            for h in cors_headers() {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        }
        return;
    }

    match (method, path.as_str()) {
        (Method::Get, "/og-image.svg") => {
            respond_text(
                req,
                200,
                "image/svg+xml; charset=utf-8",
                render_og_image_svg().to_string(),
            );
        }
        (Method::Get, "/install.sh") => {
            respond_text(
                req,
                200,
                "text/x-shellscript; charset=utf-8",
                INSTALL_SCRIPT_SH.to_string(),
            );
        }
        (Method::Get, "/whitepaper.pdf") => {
            let mut resp = Response::from_data(crate::agent::WHITEPAPER_V2_2_PDF.to_vec())
                .with_status_code(StatusCode(200))
                .with_header(Header::from_bytes("Content-Type", "application/pdf").unwrap())
                .with_header(
                    Header::from_bytes(
                        "Content-Disposition",
                        "inline; filename=\"newsatoshi-v2.2.pdf\"",
                    )
                    .unwrap(),
                );
            for h in cors_headers() {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        }
        (Method::Get, "/whitepaper.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                crate::agent::WHITEPAPER_V2_2_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/whitepaper-v2.1.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                WHITEPAPER_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/manifesto.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                crate::agent::MANIFESTO_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/bounty.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                crate::agent::BUG_BOUNTY_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/nsusd.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                crate::agent::NSUSD_RFC_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/l2.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                crate::l2_rollup::render_l2_architecture_markdown().to_string(),
            );
        }
        (Method::Get, "/api/v2/nsusd/state") => {
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let eng = crate::nsusd::NsUsdEngine::new();
            let seed = state.get_seed_snapshot(false);
            let backing = state.get_backing_summary(false);
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "honest_initial_state": true,
                    "engine": eng.summary_json(now_unix),
                    "live_mainnet_v2": {
                        "height": seed.mainnet_status.as_ref().map(|s| s.height),
                        "difficulty": seed.mainnet_status.as_ref().map(|s| s.difficulty),
                        "tip": seed.mainnet_status.as_ref().map(|s| s.tip.clone()),
                        "genesis": "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8"
                    },
                    "live_backing_vault": {
                        "vault_address": backing.canonical_vault_address,
                        "total_eth": backing.total_eth_formatted,
                        "total_usdc": backing.total_usdc_formatted,
                        "deposits": backing.total_vault_deposits,
                        "verified_base_proofs": {
                            "direct_usdc_deposit_tx": crate::backing::LIVE_BASE_DIRECT_USDC_TX,
                            "rfc0028_nsusd_deposit_tx": crate::backing::LIVE_BASE_NSUSD_DEPOSIT_TX,
                            "base_htlc_lock_tx": crate::backing::LIVE_BASE_HTLC_LOCK_TX,
                            "base_htlc_claim_tx": crate::backing::LIVE_BASE_HTLC_CLAIM_TX
                        }
                    },
                    "stress_test_endpoint": "POST /api/v2/nsusd/simulate-stress",
                    "verify_base_escrow_endpoint": "POST /api/v2/nsusd/verify-base-escrow",
                    "paywall_kit_endpoint": "POST /api/v2/x402/paywall-kit"
                }),
            );
        }
        (Method::Post, "/api/v2/nsusd/simulate-stress") => {
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let crash_pct = parsed.get("crash_pct").and_then(|v| v.as_u64()).unwrap_or(50);
            let report = crate::nsusd::run_adversarial_stress_suite(crash_pct);
            respond_json(req, 200, &report);
        }
        (Method::Post, "/api/v2/nsusd/verify-base-escrow") => {
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let tx_hash = parsed
                .get("tx_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if tx_hash.is_empty() {
                respond_json(
                    req,
                    400,
                    &json!({
                        "ok": false,
                        "error": "missing required 'tx_hash' (Base Mainnet 0x... USDC deposit tx hash)",
                        "example_tx_hash": crate::backing::LIVE_BASE_DIRECT_USDC_TX
                    }),
                );
            } else {
                match crate::backing::verify_base_usdc_escrow_tx(tx_hash) {
                    Ok(receipt) => respond_json(req, 200, &receipt),
                    Err(e) => respond_json(req, 422, &json!({ "ok": false, "error": e })),
                }
            }
        }
        (Method::Get, "/api/v2/x402/paywall-kit") => {
            let base_url = infer_base_url(&req);
            let framework = query_param(&raw_url, "framework").unwrap_or_else(|| "express".to_string());
            let price_usd = query_param(&raw_url, "price_usd")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.005);
            let route_path = query_param(&raw_url, "route_path")
                .unwrap_or_else(|| "/api/paid-endpoint".to_string());
            let pay_to = query_param(&raw_url, "pay_to_evm");
            let seller_ns1 = query_param(&raw_url, "seller_ns1");
            match crate::x402::build_private_x402_paywall_kit(
                &framework,
                price_usd,
                &route_path,
                pay_to.as_deref(),
                seller_ns1.as_deref(),
                &base_url,
            ) {
                Ok(kit) => respond_json(req, 200, &kit),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/v2/x402/paywall-kit") => {
            let base_url = infer_base_url(&req);
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let framework = parsed
                .get("framework")
                .and_then(|v| v.as_str())
                .unwrap_or("express");
            let price_usd = parsed
                .get("price_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.005);
            let route_path = parsed
                .get("route_path")
                .and_then(|v| v.as_str())
                .unwrap_or("/api/paid-endpoint");
            let pay_to = parsed.get("pay_to_evm").and_then(|v| v.as_str());
            let seller_ns1 = parsed.get("seller_ns1").and_then(|v| v.as_str());
            match crate::x402::build_private_x402_paywall_kit(
                framework,
                price_usd,
                route_path,
                pay_to,
                seller_ns1,
                &base_url,
            ) {
                Ok(kit) => respond_json(req, 200, &kit),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/paywords/cost-savings") => {
            let calls = query_param(&raw_url, "monthly_calls")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1_000_000);
            let price = query_param(&raw_url, "price_per_call_usd")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.005);
            let batch = query_param(&raw_url, "calls_per_batch")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1_000);
            match crate::paywords_launchpad::estimate_paywords_cost_savings(calls, price, batch) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/v2/paywords/cost-savings") => {
            let body = read_body_string(&mut req).unwrap_or_default();
            let p: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let calls = p.get("monthly_calls").and_then(|v| v.as_u64()).unwrap_or(1_000_000);
            let price = p.get("price_per_call_usd").and_then(|v| v.as_f64()).unwrap_or(0.005);
            let batch = p.get("calls_per_batch").and_then(|v| v.as_u64()).unwrap_or(1_000);
            match crate::paywords_launchpad::estimate_paywords_cost_savings(calls, price, batch) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/paywords/channel-kit") => {
            let base_url = infer_base_url(&req);
            let chain = query_param(&raw_url, "chain").unwrap_or_else(|| "base".into());
            let fw = query_param(&raw_url, "framework").unwrap_or_else(|| "cloudflare_worker".into());
            let recipient = query_param(&raw_url, "recipient_evm");
            let price = query_param(&raw_url, "price_usd")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.002);
            let steps = query_param(&raw_url, "max_steps")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1_000);
            let route = query_param(&raw_url, "route_path").unwrap_or_else(|| "/api/inference".into());
            match crate::paywords_launchpad::compile_payword_channel_setup_for_chain(
                &chain,
                &fw,
                recipient.as_deref(),
                price,
                steps,
                &route,
                &base_url,
            ) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/v2/paywords/channel-kit") => {
            let base_url = infer_base_url(&req);
            let body = read_body_string(&mut req).unwrap_or_default();
            let p: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let chain = p.get("chain").and_then(|v| v.as_str()).unwrap_or("base");
            let fw = p.get("framework").and_then(|v| v.as_str()).unwrap_or("cloudflare_worker");
            let recipient = p.get("recipient_evm").and_then(|v| v.as_str());
            let price = p.get("price_usd").and_then(|v| v.as_f64()).unwrap_or(0.002);
            let steps = p.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(1_000) as u32;
            let route = p.get("route_path").and_then(|v| v.as_str()).unwrap_or("/api/inference");
            match crate::paywords_launchpad::compile_payword_channel_setup_for_chain(
                chain, fw, recipient, price, steps, route, &base_url,
            ) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/paywords/try-402") | (Method::Post, "/api/v2/paywords/try-402") => {
            let base_url = infer_base_url(&req);
            let sig = header_value_ci(&req, "payment-signature");
            let pw = header_value_ci(&req, "x-newsatoshi-payword");
            let (status, payload) = crate::paywords_launchpad::evaluate_live_paywords_try_402(
                sig.as_deref(),
                pw.as_deref(),
                &base_url,
            );
            respond_json(req, status, &payload);
        }
        (Method::Get, "/api/v2/crosschain/connectivity") => {
            let base_url = infer_base_url(&req);
            let dir = crate::paywords_launchpad::build_crosschain_connectivity_directory(&base_url);
            respond_json(req, 200, &dir);
        }
        (Method::Post, "/api/v2/launchpad/fair-curve") => {
            let body = read_body_string(&mut req).unwrap_or_default();
            let p: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("Agent API Curve");
            let sym = p.get("symbol").and_then(|v| v.as_str()).unwrap_or("AGAPI");
            let v_usdc = p.get("virtual_reserve_usdc").and_then(|v| v.as_f64()).unwrap_or(10_000.0);
            let v_sup = p.get("virtual_supply_tokens").and_then(|v| v.as_u64()).unwrap_or(1_000_000_000);
            let fee_bps = p.get("swap_fee_bps").and_then(|v| v.as_u64()).unwrap_or(100) as u16;
            let buy_usdc = p.get("buy_usdc").and_then(|v| v.as_f64()).unwrap_or(1_000.0);
            let rev_usdc = p.get("paywords_revenue_usdc").and_then(|v| v.as_f64()).unwrap_or(500.0);
            let sink = p.get("fee_sink_evm").and_then(|v| v.as_str());
            match crate::paywords_launchpad::simulate_fair_curve_launch(
                name, sym, v_usdc, v_sup, fee_bps, buy_usdc, rev_usdc, sink,
            ) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/base/curves") => {
            let refresh = query_param(&raw_url, "refresh")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            match crate::paywords_launchpad::fetch_live_base_fair_curves(refresh) {
                Ok(v) => respond_json(req, 200, &v),
                Err(e) => respond_json(req, 502, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/base/channel") => {
            match query_param(&raw_url, "id") {
                Some(cid) => match crate::paywords_launchpad::inspect_live_base_payword_channel(&cid) {
                    Ok(v) => respond_json(req, 200, &v),
                    Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
                },
                None => respond_json(
                    req,
                    400,
                    &json!({ "ok": false, "error": "missing required query param 'id' (0x-prefixed 32-byte channel_id)" }),
                ),
            }
        }
        (Method::Get, "/sdk/newsatoshi-402.mjs") => {
            respond_text(
                req,
                200,
                "application/javascript; charset=utf-8",
                crate::paywords_launchpad::SDK_JS_SOURCE.to_string(),
            );
        }
        (Method::Get, "/sdk/newsatoshi-402.py") => {
            respond_text(
                req,
                200,
                "text/x-python; charset=utf-8",
                crate::paywords_launchpad::SDK_PY_SOURCE.to_string(),
            );
        }
        (Method::Get, "/api/v2/l2/status") => {
            let seed = state.get_seed_snapshot(false);
            let eng = crate::nsusd::NsUsdEngine::new();
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "architecture": "Hourglass L2 Shielded Rollup & Perfection Layer",
                    "version": env!("CARGO_PKG_VERSION"),
                    "layers": [
                        { "id": "L2A", "name": "O(1) SHA-256 PayWord Streaming", "max_ticks_per_channel": 1_000_000, "gas_per_call": 0 },
                        { "id": "L2B", "name": "RFC-0028 Dual-Tier nsUSD Stablecoin", "sovereign_asset_id": 1, "bridged_asset_id": 2, "mcr_bps": 15_000, "liq_bps": 12_000 },
                        { "id": "L2C", "name": "Shielded Merkle Batching + Winterfell 0.13 STARK Conservation", "proof_hash": "Winterfell 0.13 STARK (~22 KB) anchored via 32B SHA-256 L1 commitment" },
                        { "id": "L2D", "name": "Unilateral L1 Escape Hatch", "sequencer_custody": false, "exit_proof": "SHA-256 Merkle inclusion -> native L1 SHA-256 HTLC / PriorLock" }
                    ],
                    "nsusd_solvency_root_hex": hex::encode(eng.solvency_root()),
                    "live_mainnet_v2_anchor": {
                        "height": seed.mainnet_status.as_ref().map(|s| s.height),
                        "tip": seed.mainnet_status.as_ref().map(|s| s.tip.clone()),
                        "difficulty": seed.mainnet_status.as_ref().map(|s| s.difficulty),
                        "genesis": "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8"
                    },
                    "benchmark_endpoint": "POST /api/v2/l2/benchmark",
                    "docs_url": "https://commons.newsatoshi.org/l2"
                }),
            );
        }
        (Method::Post, "/api/v2/l2/benchmark") => {
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let tx_count = parsed.get("tx_count").and_then(|v| v.as_u64()).unwrap_or(2_000) as usize;
            let seed = state.get_seed_snapshot(false);
            let eng = crate::nsusd::NsUsdEngine::new();
            match crate::l2_rollup::run_live_l2_batch_benchmark(tx_count, &seed, eng.solvency_root()) {
                Ok(report) => respond_json(req, 200, &report),
                Err(e) => respond_json(req, 500, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/nsusd/quote") => {
            let usd = query_param(&raw_url, "usd")
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(0.01)
                .clamp(0.000001, 1_000_000.0);
            let micro_usd_ticks = (usd * 1_000_000.0).round().max(1.0) as u64;
            let seed = state.get_seed_snapshot(false);
            let backing = state.get_backing_summary(false);
            let payload = serde_json::json!({
                "ok": true,
                "spec": "RFC-0028",
                "asset": "nsUSD",
                "usd_amount": (micro_usd_ticks as f64) / 1_000_000.0,
                "micro_usd_ticks": micro_usd_ticks,
                "payword_unit_usd": 0.000001,
                "volatility_exposure": "0% (USD-denominated JIT atomic settlement)",
                "tier_1_live_rails": [
                    {
                        "rail": "PAYWORDS_USD_JIT",
                        "latency": "< 1 ms (1 SHA-256 hash/call)",
                        "gas_per_call": 0,
                        "settlement": ["BTC_BIP199_HTLC", "LIGHTNING_BOLT11", "EVM_4CHAIN_HTLC", "NATIVE_LOVE_MAINNET_V2"]
                    },
                    {
                        "rail": "X402_V2_USDC_ONRAMP",
                        "networks": ["eip155:8453", "eip155:42161", "eip155:137"],
                        "discovery": "/.well-known/x402"
                    }
                ],
                "tier_2_sovereign_nsusd": {
                    "status": "ENGINE_LIVE_SANDBOX_VERIFIED",
                    "primary_collateral": "BTC (>= 150% CR, 120% liquidation)",
                    "secondary_collateral": "Native Mainnet v2 LOVE (>= 250% CR after deep spot liquidity)",
                    "algorithmic_unbacked_mint": false,
                    "freeze_switch": false,
                    "oracle": "PriorLock (RFC-0021) bonded median oracle (50% challenger slash / 50% burn)",
                    "redemption_floor": "$1.00 direct collateral redemption",
                    "settlement_architecture": "Hourglass L2 Rollup + native SHA-256 HTLC / PriorLock (zero L1 consensus fork required)"
                },
                "live_mainnet_v2": {
                    "height": seed.mainnet_status.as_ref().map(|s| s.height),
                    "difficulty": seed.mainnet_status.as_ref().map(|s| s.difficulty),
                    "genesis": "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8"
                },
                "live_backing_vault_honest": {
                    "vault_address": backing.canonical_vault_address,
                    "total_eth": backing.total_eth_formatted,
                    "total_usdc": backing.total_usdc_formatted,
                    "deposits": backing.total_vault_deposits
                },
                "docs_url": "https://commons.newsatoshi.org/nsusd"
            });
            respond_json(req, 200, &payload);
        }
        (Method::Get, "/whitepaper.tex") => {
            respond_text(
                req,
                200,
                "text/plain; charset=utf-8",
                WHITEPAPER_LATEX.to_string(),
            );
        }
        (Method::Get, "/formal/NewsatoshiCovenants.lean") => {
            respond_text(
                req,
                200,
                "text/plain; charset=utf-8",
                LEAN_FORMAL_PROOF.to_string(),
            );
        }
        (Method::Get, "/formal/NsUsdAndL2Perfection.lean") => {
            respond_text(
                req,
                200,
                "text/plain; charset=utf-8",
                crate::agent::LEAN_NSUSD_L2_PROOF.to_string(),
            );
        }
        (Method::Get, "/whitepaper" | "/manifesto" | "/bounty" | "/nsusd" | "/l2" | "/docs" | "/developers" | "/about" | "/contact" | "/privacy") => {
            let base_url = infer_base_url(&req);
            if let Some((ct, body)) = render_public_page(&path, &base_url, wants_markdown(&req)) {
                let mut resp = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(Header::from_bytes("Content-Type", ct).unwrap())
                    .with_header(Header::from_bytes("Vary", "Accept").unwrap())
                    .with_header(discovery_link_header());
                for h in cors_headers() {
                    resp = resp.with_header(h);
                }
                let _ = req.respond(resp);
            } else {
                respond_problem_404(req, &path, &base_url);
            }
        }
        (Method::Get, "/robots.txt") => {
            let base_url = infer_base_url(&req);
            respond_text(req, 200, "text/plain; charset=utf-8", render_robots_txt(&base_url));
        }
        (Method::Get, "/sitemap.xml") => {
            let base_url = infer_base_url(&req);
            respond_text(
                req,
                200,
                "application/xml; charset=utf-8",
                render_sitemap_xml(&base_url),
            );
        }
        (Method::Get, "/llms.txt") => {
            let base_url = infer_base_url(&req);
            let seed = state.get_seed_snapshot(false);
            let api_state = state.engine.lock().unwrap_or_else(|e| e.into_inner()).to_api_state();
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                render_llms_txt(&base_url, &api_state, &seed),
            );
        }
        (Method::Get, "/llms-full.txt") => {
            let base_url = infer_base_url(&req);
            let seed = state.get_seed_snapshot(false);
            let api_state = state.engine.lock().unwrap_or_else(|e| e.into_inner()).to_api_state();
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                render_llms_full_txt(&base_url, &api_state, &seed),
            );
        }
        (Method::Get, "/.well-known/mcp-registry-auth") => {
            respond_text(
                req,
                200,
                "text/plain; charset=utf-8",
                "v=MCPv1; k=ed25519; p=n0LJ+zLwShIlENsy4iOKZLHW3eeAfRyO1ItUzxWm0n4=\n".to_string(),
            );
        }
        (Method::Get, "/.well-known/agent.json") | (Method::Get, "/.well-known/agent-card.json") => {
            let base_url = infer_base_url(&req);
            let api_state = state.engine.lock().unwrap_or_else(|e| e.into_inner()).to_api_state();
            respond_json(req, 200, &render_agent_card_json(&base_url, &api_state));
        }
        (Method::Get, "/.well-known/erc8004.json") => {
            let base_url = infer_base_url(&req);
            let rule_hash = {
                let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
                hex::encode(engine.rule.rule_hash())
            };
            respond_json(req, 200, &render_gateway_erc8004(&base_url, &rule_hash));
        }
        (Method::Get, "/.well-known/x402.json") | (Method::Get, "/.well-known/x402") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_well_known_x402(&base_url));
        }
        (Method::Get, "/.well-known/402index-verify.txt") => {
            respond_text(
                req,
                200,
                "text/plain; charset=utf-8",
                "fda5f8f68094de6dcf826e7ce1971d61584372847d876452ceac2e27e6c9f0a6\n".to_string(),
            );
        }
        (Method::Get, "/api/x402/bridge-quote") => {
            let base_url = infer_base_url(&req);
            let spec = route_spec_bridge_quote();
            let (challenge_obj, hdr_b64, challenge_body) = build_x402_v2_challenge(&base_url, &spec);
            let payword_hdr = header_value_ci(&req, "X-Newsatoshi-PayWord");
            let x402_hdr = header_value_ci(&req, "Payment-Signature")
                .or_else(|| header_value_ci(&req, "X-Payment"));
            match authorize_dual_402(payword_hdr.as_deref(), x402_hdr.as_deref(), &challenge_obj) {
                Ok(Some(receipt)) => {
                    let backing = state.get_backing_summary(false);
                    let payload = build_bridge_quote_response(&backing, Some(&receipt));
                    respond_x402_paid(req, &payload, receipt.payment_response_b64.as_deref());
                }
                Ok(None) => respond_x402_challenge(req, &hdr_b64, &challenge_body),
                Err(e) => {
                    let mut err_body = challenge_body;
                    err_body["error"] = json!(e);
                    respond_x402_challenge(req, &hdr_b64, &err_body);
                }
            }
        }
        (Method::Post, "/api/x402/compile-htlc") => {
            let base_url = infer_base_url(&req);
            let spec = route_spec_compile_htlc();
            let (challenge_obj, hdr_b64, challenge_body) = build_x402_v2_challenge(&base_url, &spec);
            let payword_hdr = header_value_ci(&req, "X-Newsatoshi-PayWord");
            let x402_hdr = header_value_ci(&req, "Payment-Signature")
                .or_else(|| header_value_ci(&req, "X-Payment"));
            match authorize_dual_402(payword_hdr.as_deref(), x402_hdr.as_deref(), &challenge_obj) {
                Ok(Some(receipt)) => {
                    let body = read_body_string(&mut req).unwrap_or_default();
                    let parsed: CompileSwapRequest = match serde_json::from_str(&body) {
                        Ok(v) => v,
                        Err(e) => {
                            respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}"), "settlement": receipt }));
                            return;
                        }
                    };
                    match compile_atomic_swap(&parsed) {
                        Ok(contract) => respond_x402_paid(
                            req,
                            &json!({ "ok": true, "contract": contract, "settlement": receipt }),
                            receipt.payment_response_b64.as_deref(),
                        ),
                        Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e, "settlement": receipt })),
                    }
                }
                Ok(None) => respond_x402_challenge(req, &hdr_b64, &challenge_body),
                Err(e) => {
                    let mut err_body = challenge_body;
                    err_body["error"] = json!(e);
                    respond_x402_challenge(req, &hdr_b64, &err_body);
                }
            }
        }
        (Method::Post, "/api/x402/verify-payword") => {
            let base_url = infer_base_url(&req);
            let spec = route_spec_verify_payword();
            let (challenge_obj, hdr_b64, challenge_body) = build_x402_v2_challenge(&base_url, &spec);
            let payword_hdr = header_value_ci(&req, "X-Newsatoshi-PayWord");
            let x402_hdr = header_value_ci(&req, "Payment-Signature")
                .or_else(|| header_value_ci(&req, "X-Payment"));
            match authorize_dual_402(payword_hdr.as_deref(), x402_hdr.as_deref(), &challenge_obj) {
                Ok(Some(receipt)) => {
                    let body = read_body_string(&mut req).unwrap_or_default();
                    let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
                    let tip = parsed.get("tip_hex").and_then(|v| v.as_str()).unwrap_or("");
                    let tok = parsed.get("token_hex").and_then(|v| v.as_str()).unwrap_or("");
                    let ticks = parsed.get("ticks").and_then(|v| v.as_u64()).unwrap_or(1);
                    match verify_payword_token(tip, tok, ticks, 10_000) {
                        Ok(res) => respond_x402_paid(
                            req,
                            &json!({ "ok": true, "verification": res, "settlement": receipt }),
                            receipt.payment_response_b64.as_deref(),
                        ),
                        Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e, "settlement": receipt })),
                    }
                }
                Ok(None) => respond_x402_challenge(req, &hdr_b64, &challenge_body),
                Err(e) => {
                    let mut err_body = challenge_body;
                    err_body["error"] = json!(e);
                    respond_x402_challenge(req, &hdr_b64, &err_body);
                }
            }
        }
        (Method::Post, "/api/x402/evm-security-scan") => {
            let base_url = infer_base_url(&req);
            let spec = route_spec_evm_security_scan();
            let (challenge_obj, hdr_b64, challenge_body) = build_x402_v2_challenge(&base_url, &spec);
            let payword_hdr = header_value_ci(&req, "X-Newsatoshi-PayWord");
            let x402_hdr = header_value_ci(&req, "Payment-Signature")
                .or_else(|| header_value_ci(&req, "X-Payment"));
            match authorize_dual_402(payword_hdr.as_deref(), x402_hdr.as_deref(), &challenge_obj) {
                Ok(Some(receipt)) => {
                    let body = read_body_string(&mut req).unwrap_or_default();
                    let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
                    let chain = parsed.get("chain").and_then(|v| v.as_str()).unwrap_or("base");
                    let address = parsed.get("address").and_then(|v| v.as_str()).unwrap_or("");
                    match inspect_evm_contract(chain, address) {
                        Ok(mut scan) => {
                            if let Some(obj) = scan.as_object_mut() {
                                obj.insert("settlement".to_string(), json!(receipt));
                            }
                            respond_x402_paid(req, &scan, receipt.payment_response_b64.as_deref());
                        }
                        Err(e) => respond_json(
                            req,
                            400,
                            &json!({ "ok": false, "error": e, "settlement": receipt }),
                        ),
                    }
                }
                Ok(None) => respond_x402_challenge(req, &hdr_b64, &challenge_body),
                Err(e) => {
                    let mut err_body = challenge_body;
                    err_body["error"] = json!(e);
                    respond_x402_challenge(req, &hdr_b64, &err_body);
                }
            }
        }
        (Method::Get, "/api/pool/status") => {
            let pool_url = std::env::var("COMMONS_POOL_STATUS_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://pool.newsatoshi.org/status".to_string());
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .http_status_as_error(false)
                .build()
                .into();
            match agent
                .get(&pool_url)
                .header("User-Agent", "newsatoshi-commons/0.12")
                .call()
            {
                Ok(mut resp) if resp.status().as_u16() < 400 => {
                    let txt = resp.body_mut().read_to_string().unwrap_or_default();
                    if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                        respond_json(req, 200, &val);
                    } else {
                        respond_json(
                            req,
                            200,
                            &json!({ "ok": false, "online": false, "error": "malformed pool status JSON" }),
                        );
                    }
                }
                Ok(resp) => respond_json(
                    req,
                    200,
                    &json!({ "ok": false, "online": false, "error": format!("pool HTTP {}", resp.status().as_u16()) }),
                ),
                Err(e) => respond_json(
                    req,
                    200,
                    &json!({ "ok": false, "online": false, "error": e.to_string() }),
                ),
            }
        }
        (Method::Get, "/.well-known/mcp.json") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_mcp_manifest_json(&base_url));
        }
        (Method::Get, "/.well-known/mcp/server-card.json") | (Method::Get, "/server.json") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_mcp_server_card_json(&base_url));
        }
        (Method::Get, "/.well-known/api-catalog") => {
            let base_url = infer_base_url(&req);
            respond_json_with_ct(
                req,
                200,
                "application/linkset+json; charset=utf-8",
                &render_api_catalog_json(&base_url),
            );
        }
        (Method::Get, "/.well-known/agent-skills/index.json") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_agent_skills_index_json(&base_url));
        }
        (Method::Get, "/skills/newsatoshi-agent-payments/SKILL.md") => {
            respond_text(
                req,
                200,
                "text/markdown; charset=utf-8",
                AGENT_SKILL_MARKDOWN.to_string(),
            );
        }
        (Method::Get, "/openapi.json") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_openapi_json(&base_url));
        }
        (Method::Get, "/mcp") => {
            let base_url = infer_base_url(&req);
            respond_json(req, 200, &render_mcp_manifest_json(&base_url));
        }
        (Method::Post, "/mcp") => {
            let base_url = infer_base_url(&req);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e}}),
                    );
                    return;
                }
            };
            let rpc_val: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
                    );
                    return;
                }
            };
            let force_refresh = rpc_val
                .get("params")
                .and_then(|p| p.get("arguments"))
                .and_then(|a| a.get("refresh"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            let seed = state.get_seed_snapshot(force_refresh);
            let is_backing_tool = rpc_val
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                == Some("newsatoshi_get_backing_proof");
            let backing_opt = if is_backing_tool {
                Some(state.get_backing_summary(force_refresh))
            } else {
                None
            };
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            let (status, maybe_body) = handle_mcp_jsonrpc(
                &rpc_val,
                &base_url,
                &engine,
                &seed,
                backing_opt.as_ref(),
                &state.seed_client,
                state.mainnet_seed_client.as_ref(),
            );
            drop(engine);
            match maybe_body {
                Some(resp_json) => respond_json(req, status, &resp_json),
                None => respond_empty(req, status),
            }
        }
        (Method::Get, "/api/agent/402-challenge") => {
            let resource = query_param(&raw_url, "resource")
                .filter(|r| {
                    !r.is_empty()
                        && r.len() <= 128
                        && r.bytes()
                            .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
                })
                .unwrap_or_else(|| "/api/inference".to_string());
            let amount = query_param(&raw_url, "amount_love")
                .filter(|a| {
                    !a.is_empty()
                        && a.len() <= 20
                        && a.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                        && a.bytes().filter(|&b| b == b'.').count() <= 1
                })
                .unwrap_or_else(|| "0.01".to_string());
            let network = match query_param(&raw_url, "network")
                .as_deref()
                .map(str::trim)
            {
                Some("testnet") => "testnet".to_string(),
                _ => "mainnet".to_string(),
            };
            let recipient = query_param(&raw_url, "to")
                .filter(|a| a.len() <= 128 && a.bytes().all(|b| b.is_ascii_alphanumeric()))
                .unwrap_or_default();
            let unit_price = query_param(&raw_url, "unit_price")
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&v| v > 0)
                .unwrap_or(1_000);
            let challenge = build_http_402_challenge(&resource, &amount, &network, &recipient, unit_price);
            let body = serde_json::to_vec(&challenge).unwrap_or_else(|_| b"{}".to_vec());
            let mut resp = Response::from_data(body)
                .with_status_code(StatusCode(402))
                .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap());
            if let Ok(www_auth) = Header::from_bytes(
                "WWW-Authenticate",
                format!("Newsatoshi-402 realm=\"newsatoshi\", network=\"{network}\", amount=\"{amount}\""),
            ) {
                resp = resp.with_header(www_auth);
            }
            for h in cors_headers() {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        }
        (Method::Post, "/api/agent/402-verify") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: Value = match serde_json::from_str(&body) {
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
            if parsed.get("tip_hex").is_some() && parsed.get("token_hex").is_some() {
                let tip = parsed.get("tip_hex").and_then(|v| v.as_str()).unwrap_or("");
                let tok = parsed.get("token_hex").and_then(|v| v.as_str()).unwrap_or("");
                let ticks = parsed.get("ticks").and_then(|v| v.as_u64()).unwrap_or(0);
                let unit = parsed
                    .get("unit_price_base_units")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1_000);
                match verify_payword_token(tip, tok, ticks, unit) {
                    Ok(res) => respond_json(req, 200, &res),
                    Err(e) => respond_json(req, 400, &json!({ "ok": false, "valid": false, "error": e })),
                }
            } else if let Some(hash) = parsed.get("payment_hash_hex").and_then(|v| v.as_str()) {
                let net = parsed
                    .get("network")
                    .or_else(|| parsed.get("love_network"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("mainnet");
                let preimage = parsed.get("preimage_hex").and_then(|v| v.as_str());
                let Some(client) = state.client_for_network(net) else {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("no L1 client for network '{net}'") }),
                    );
                    return;
                };
                match crate::swap::verify_swap_on_chain(client, net, hash, None, preimage) {
                    Ok(report) => respond_json(req, 200, &report),
                    Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
                }
            } else {
                respond_json(
                    req,
                    400,
                    &json!({
                        "ok": false,
                        "error": "provide either {tip_hex, token_hex, ticks} for PayWord 402 or {payment_hash_hex, network?} for L1 lock verification"
                    }),
                );
            }
        }
        (Method::Get, "/api/health") => {
            let uptime_secs = state.started_at.elapsed().as_secs();
            let signing_keys_loaded = {
                let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
                let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
                engine.to_api_state()
            };
            respond_json(req, 200, &api_state);
        }
        (Method::Get, "/api/seed") => {
            let force = raw_url.contains("refresh=1") || raw_url.contains("refresh=true");
            let snapshot = state.get_seed_snapshot(force);
            respond_json(req, 200, &snapshot);
        }
        (Method::Get, "/api/network/metrics") => {
            let force = raw_url.contains("refresh=1") || raw_url.contains("refresh=true");
            let ths = query_param(&raw_url, "hashrate_ths")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1.2);
            let snapshot = state.get_seed_snapshot(force);
            respond_json(req, 200, &compute_network_metrics(&snapshot, ths));
        }
        (Method::Get, "/api/backing/summary") => {
            let force = raw_url.contains("refresh=1") || raw_url.contains("refresh=true");
            let summary = state.get_backing_summary(force);
            respond_json(req, 200, &summary);
        }
        (Method::Get, "/api/web4/charters") => {
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "canonical_backing_vault": CANONICAL_BACKING_VAULT,
                    "count": engine.web4_charters.len(),
                    "charters": engine.web4_charters,
                }),
            );
        }
        (Method::Post, "/api/web4/charters/compile") => {
            let base_url = infer_base_url(&req);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: SpawnCharterRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            let parent = parsed.parent_charter_id.as_deref().and_then(|pid| {
                engine.web4_charters.iter().find(|c| c.charter_id == pid.trim())
            });
            match compile_web4_charter(&parsed, parent, &base_url) {
                Ok(charter) => {
                    let erc8004 = render_erc8004_registration(&charter, &base_url);
                    respond_json(
                        req,
                        200,
                        &json!({
                            "ok": true,
                            "persisted": false,
                            "charter": charter,
                            "erc8004_registration": erc8004,
                        }),
                    );
                }
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/launchpad/compile") => {
            let base_url = infer_base_url(&req);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: crate::launchpad::LaunchMissionRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match crate::launchpad::compile_mission_launchpad(&parsed, &base_url) {
                Ok(mission) => respond_json(
                    req,
                    200,
                    &json!({
                        "ok": true,
                        "persisted": false,
                        "mission": mission
                    }),
                ),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/launchpad/quote") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            let side = parsed
                .get("side")
                .and_then(|v| v.as_str())
                .unwrap_or("buy")
                .trim()
                .to_ascii_lowercase();
            let curve_reserve = match parsed.get("curve_reserve").and_then(|v| v.as_u64()) {
                Some(v) => v,
                None => {
                    respond_json(req, 400, &json!({ "ok": false, "error": "missing required integer 'curve_reserve'" }));
                    return;
                }
            };
            let total_shares = match parsed.get("total_shares").and_then(|v| v.as_u64()) {
                Some(v) => v,
                None => {
                    respond_json(req, 400, &json!({ "ok": false, "error": "missing required integer 'total_shares'" }));
                    return;
                }
            };
            let curve_fee_bps = match parsed.get("curve_fee_bps").and_then(|v| v.as_u64()).unwrap_or(500) {
                v if v <= u16::MAX as u64 => v as u16,
                _ => {
                    respond_json(req, 400, &json!({ "ok": false, "error": "curve_fee_bps out of range" }));
                    return;
                }
            };
            let quoted = match side.as_str() {
                "buy" => match parsed.get("amount").and_then(|v| v.as_u64()) {
                    Some(amount) => crate::launchpad::quote_curve_buy(curve_reserve, total_shares, curve_fee_bps, amount),
                    None => Err("missing required integer 'amount' for buy quote".into()),
                },
                "redeem" => match parsed.get("shares").and_then(|v| v.as_u64()) {
                    Some(shares) => {
                        let miner_fee = parsed.get("miner_fee").and_then(|v| v.as_u64()).unwrap_or(0);
                        crate::launchpad::quote_curve_redeem(curve_reserve, total_shares, curve_fee_bps, shares, miner_fee)
                    }
                    None => Err("missing required integer 'shares' for redeem quote".into()),
                },
                other => Err(format!("invalid curve side '{other}' (expected 'buy' or 'redeem')")),
            };
            match quoted {
                Ok(q) => respond_json(req, 200, &q),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/zktls/verify") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: crate::zktls::ZkTlsProofPayload = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid zkTLS JSON: {e}") }));
                    return;
                }
            };
            match crate::zktls::verify_zktls_proof(&parsed) {
                Ok(report) => respond_json(req, 200, &report),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "valid": false, "error": e })),
            }
        }
        (Method::Get, "/api/v2/cutover-readiness") => {
            let force = raw_url.contains("refresh=1") || raw_url.contains("refresh=true");
            let snapshot = state.get_seed_snapshot(force);
            respond_json(req, 200, &crate::cutover::evaluate_v2_cutover_readiness(&snapshot));
        }
        (Method::Get, "/api/v2/live-verify") => {
            let url = state
                .mainnet_seed_client
                .as_ref()
                .map(|c| c.endpoint_url.as_str())
                .unwrap_or(crate::seed::DEFAULT_MAINNET_SEED_STATUS_URL);
            respond_json(req, 200, &crate::cutover::fetch_live_v2_verification(url));
        }
        (Method::Get, "/api/p2p/offers") => {
            // Honest zero initial state: never seed fake market makers or fake liquidity!
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "active_bonded_makers": 0,
                    "offers": [],
                    "supported_rails": crate::p2p::SUPPORTED_FIAT_RAILS,
                    "compile_endpoint": "/api/p2p/compile-offer",
                    "verify_settlement_endpoint": "/api/p2p/verify-settlement"
                }),
            );
        }
        (Method::Post, "/api/p2p/compile-offer") => {
            let base_url = infer_base_url(&req);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: crate::p2p::BondedP2pOfferRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match crate::p2p::compile_bonded_p2p_offer(&parsed, &base_url) {
                Ok(offer) => respond_json(req, 200, &json!({ "ok": true, "persisted": false, "offer": offer })),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/p2p/verify-settlement") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            let offer_id = parsed.get("offer_id_hex").and_then(|v| v.as_str()).unwrap_or("");
            let bond_id = parsed.get("maker_bond_id_hex").and_then(|v| v.as_str()).unwrap_or("");
            let settled_love = parsed
                .get("settled_love_base_units")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let proof_val = match parsed.get("zktls_proof") {
                Some(v) => v.clone(),
                None => {
                    respond_json(req, 400, &json!({ "ok": false, "error": "missing required 'zktls_proof' object" }));
                    return;
                }
            };
            let proof: crate::zktls::ZkTlsProofPayload = match serde_json::from_value(proof_val) {
                Ok(p) => p,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid zktls_proof: {e}") }));
                    return;
                }
            };
            match crate::p2p::verify_p2p_settlement_with_zktls(offer_id, bond_id, settled_love, &proof) {
                Ok(report) => respond_json(req, 200, &report),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "verified": false, "error": e })),
            }
        }
        (Method::Post, "/api/web4/charters") => {
            if !is_authorized_operator(&req, state) {
                respond_unauthorized(req);
                return;
            }
            let base_url = infer_base_url(&req);
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: SpawnCharterRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            let parent = parsed.parent_charter_id.as_deref().and_then(|pid| {
                engine.web4_charters.iter().find(|c| c.charter_id == pid.trim())
            });
            match compile_web4_charter(&parsed, parent, &base_url) {
                Ok(charter) => {
                    if !engine.web4_charters.iter().any(|c| c.charter_id == charter.charter_id) {
                        engine.web4_charters.insert(0, charter.clone());
                        if engine.web4_charters.len() > 200 {
                            engine.web4_charters.truncate(200);
                        }
                    }
                    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
                    let erc8004 = render_erc8004_registration(&charter, &base_url);
                    let api_state = engine.to_api_state();
                    respond_json(
                        req,
                        200,
                        &json!({
                            "ok": true,
                            "persisted": true,
                            "charter": charter,
                            "erc8004_registration": erc8004,
                            "state": api_state,
                        }),
                    );
                }
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/web4/services") => {
            let cap = query_param(&raw_url, "capability")
                .map(|c| c.trim().to_ascii_lowercase())
                .filter(|c| !c.is_empty());
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            let services: Vec<_> = engine
                .web4_services
                .iter()
                .filter(|s| match &cap {
                    Some(c) => s.capabilities.iter().any(|x| x == c),
                    None => true,
                })
                .cloned()
                .collect();
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "count": services.len(),
                    "services": services,
                }),
            );
        }
        (Method::Post, "/api/web4/services/validate") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: RegisterServiceRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match validate_web4_service(&parsed) {
                Ok(svc) => respond_json(req, 200, &json!({ "ok": true, "persisted": false, "service": svc })),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/web4/services") => {
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
            let parsed: RegisterServiceRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match validate_web4_service(&parsed) {
                Ok(svc) => {
                    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
                    if !engine.web4_services.iter().any(|s| s.service_id == svc.service_id) {
                        engine.web4_services.insert(0, svc.clone());
                        if engine.web4_services.len() > 200 {
                            engine.web4_services.truncate(200);
                        }
                    }
                    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
                    let api_state = engine.to_api_state();
                    respond_json(req, 200, &json!({ "ok": true, "persisted": true, "service": svc, "state": api_state }));
                }
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/web4/bounties") => {
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "count": engine.web4_bounties.len(),
                    "bounties": engine.web4_bounties,
                }),
            );
        }
        (Method::Post, "/api/web4/bounties/compile") => {
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: CreateBountyRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match compile_web4_bounty(&parsed) {
                Ok(bounty) => respond_json(req, 200, &json!({ "ok": true, "persisted": false, "bounty": bounty })),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/web4/bounties") => {
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
            let parsed: CreateBountyRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": format!("invalid JSON: {e}") }));
                    return;
                }
            };
            match compile_web4_bounty(&parsed) {
                Ok(bounty) => {
                    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
                    if !engine.web4_bounties.iter().any(|b| b.bounty_id == bounty.bounty_id) {
                        engine.web4_bounties.insert(0, bounty.clone());
                        if engine.web4_bounties.len() > 200 {
                            engine.web4_bounties.truncate(200);
                        }
                    }
                    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
                    let api_state = engine.to_api_state();
                    respond_json(req, 200, &json!({ "ok": true, "persisted": true, "bounty": bounty, "state": api_state }));
                }
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/web4/tools/verify-payword-batch") => {
            // Dogfooded HTTP 402 PayWord-gated utility endpoint:
            // If unpaid, emits dual-rail HTTP 402 challenge; if paid with valid X-Newsatoshi-PayWord
            // header or JSON payment object, verifies the SHA-256 PayWord token and returns verified status.
            let body = read_body_string(&mut req).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let pay_hdr = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("X-Newsatoshi-PayWord"))
                .map(|h| h.value.as_str().to_string());
            let mut tip_opt = parsed.get("tip_hex").and_then(|v| v.as_str()).map(ToString::to_string);
            let mut tok_opt = parsed.get("token_hex").and_then(|v| v.as_str()).map(ToString::to_string);
            let mut ticks_opt = parsed.get("ticks").and_then(|v| v.as_u64());
            if let Some(hdr) = pay_hdr {
                for kv in hdr.split(',') {
                    if let Some((k, v)) = kv.trim().split_once('=') {
                        match k.trim() {
                            "tip" => tip_opt = Some(v.trim().to_string()),
                            "token" => tok_opt = Some(v.trim().to_string()),
                            "ticks" => ticks_opt = v.trim().parse::<u64>().ok(),
                            _ => {}
                        }
                    }
                }
            }
            match (tip_opt, tok_opt, ticks_opt) {
                (Some(tip), Some(tok), Some(ticks)) => {
                    match verify_payword_token(&tip, &tok, ticks, 1_000) {
                        Ok(res) if res.valid => {
                            respond_json(
                                req,
                                200,
                                &json!({
                                    "ok": true,
                                    "paid": true,
                                    "settlement_rail": "NEWSATOSHI-PAYWORD-SHA256/1.0",
                                    "backing_vault": CANONICAL_BACKING_VAULT,
                                    "verification": res,
                                }),
                            );
                        }
                        Ok(res) => respond_json(req, 402, &json!({ "ok": false, "paid": false, "verification": res })),
                        Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
                    }
                }
                _ => {
                    let challenge = build_http_402_challenge(
                        "/api/web4/tools/verify-payword-batch",
                        "0.001",
                        "mainnet",
                        "",
                        1_000,
                    );
                    respond_json(req, 402, &challenge);
                }
            }
        }
        (Method::Get, p) if p.starts_with("/api/web4/charters/") && p.ends_with("/erc8004.json") => {
            let base_url = infer_base_url(&req);
            let cid = p
                .trim_start_matches("/api/web4/charters/")
                .trim_end_matches("/erc8004.json");
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ch) = engine.web4_charters.iter().find(|c| c.charter_id == cid) {
                respond_json(req, 200, &render_erc8004_registration(ch, &base_url));
            } else if cid == "charter-genesis-commons" {
                // Canonical ERC-8004 card for Genesis Charter #1 anchored on Base Mainnet (0x7a8183...eb47)
                respond_json(
                    req,
                    200,
                    &json!({
                        "type": "https://eips.ethereum.org/EIPS/eip-8004#registration-v1",
                        "name": "Newsatoshi Commons Genesis Utility (Charter #1)",
                        "description": "Verify-only public gateway, Lean 4.36 verified STARK verifier, Official MCP Registry server, and 4-chain SHA-256 HTLC & Proof-of-Backing coordinator.",
                        "active": true,
                        "charter_id": "charter-genesis-commons",
                        "charter_id_bytes32": crate::backing::GENESIS_BASE_CHARTER_ID_HEX,
                        "base_anchor_tx": crate::backing::GENESIS_BASE_ANCHOR_TX,
                        "base_genesis_deposit_tx": crate::backing::GENESIS_BASE_DEPOSIT_TX,
                        "backing_vault": CANONICAL_BACKING_VAULT,
                        "backing_bps": 2000,
                        "x402Support": true,
                        "paywordSupport": true,
                        "services": [
                            { "name": "MCP", "endpoint": format!("{base_url}/mcp"), "version": "2025-03-26" },
                            { "name": "BackingProof", "endpoint": format!("{base_url}/api/backing/summary"), "version": "1.0" },
                            { "name": "Lean4Kernel", "endpoint": format!("{base_url}/formal/NewsatoshiCovenants.lean"), "version": "4.36.0" }
                        ]
                    }),
                );
            } else {
                respond_problem_404(req, p, &base_url);
            }
        }
        (Method::Get, "/api/swaps") => {
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            respond_json(
                req,
                200,
                &json!({
                    "ok": true,
                    "protocol": "NEWSATOSHI-HTLC-SWAP/1.0",
                    "count": engine.swap_offers.len(),
                    "swaps": engine.swap_offers,
                }),
            );
        }
        (Method::Post, "/api/swaps/compile") => {
            // Public read-only BIP-199 P2WSH HTLC + BOLT-11 swap compiler (zero state mutation).
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: CompileSwapRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid swap compile JSON: {e}") }),
                    );
                    return;
                }
            };
            match compile_atomic_swap(&parsed) {
                Ok(contract) => respond_json(req, 200, &json!({ "ok": true, "contract": contract })),
                Err(e) => respond_json(req, 400, &json!({ "ok": false, "error": e })),
            }
        }
        (Method::Post, "/api/swaps") => {
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
            let parsed: CompileSwapRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid swap JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_publish_swap(state, parsed);
            respond_json(req, if resp.ok { 200 } else { 400 }, &resp);
        }
        (Method::Post, "/api/swaps/verify") => {
            // Public live dual-chain verifier (Newsatoshi L1 /api/lock + mempool.space BTC UTXOs).
            let body = match read_body_string(&mut req) {
                Ok(b) => b,
                Err(e) => {
                    respond_json(req, 400, &json!({ "ok": false, "error": e }));
                    return;
                }
            };
            let parsed: VerifySwapRequest = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(
                        req,
                        400,
                        &json!({ "ok": false, "error": format!("invalid verify swap JSON: {e}") }),
                    );
                    return;
                }
            };
            let resp = handle_verify_swap(state, parsed);
            respond_json(req, if resp.ok { 200 } else { 400 }, &resp);
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
            let base_url = infer_base_url(&req);
            if (path == "/" || path == "/index.html") && wants_markdown(&req) {
                let seed = state.get_seed_snapshot(false);
                let api_state = state.engine.lock().unwrap_or_else(|e| e.into_inner()).to_api_state();
                let body = render_llms_txt(&base_url, &api_state, &seed);
                let mut resp = Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(
                        Header::from_bytes("Content-Type", "text/markdown; charset=utf-8").unwrap(),
                    )
                    .with_header(Header::from_bytes("Vary", "Accept").unwrap())
                    .with_header(discovery_link_header());
                for h in cors_headers() {
                    resp = resp.with_header(h);
                }
                let _ = req.respond(resp);
                return;
            }
            if let Some(ref static_dir) = state.static_dir {
                serve_static_file(req, static_dir, &path, &base_url);
            } else {
                respond_problem_404(req, &path, &base_url);
            }
        }
        _ => {
            let base_url = infer_base_url(&req);
            respond_problem_404(req, &path, &base_url);
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
        engine
            .anchor_receipts
            .iter()
            .find(|r| r.anchor_hash.eq_ignore_ascii_case(&target))
            .map(|r| r.network.clone())
    };
    let Some(network) = receipt_network else {
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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

    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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

    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
                let msg =
                    CheckedWorkProof::deed_msg_with_nonce(parcel_id, deed_doc_hash, price_love, nonce);
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
                let msg = CheckedWorkProof::deed_msg(parcel_id, deed_doc_hash, price_love);
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let safe_payout = payout_love.min(rule_clone.max_bounty_payout).min(input_amount).max(1);
        let sandbox_payout = safe_payout.min(input_amount / 2).max(1);
        let err_msg = match scheme.as_str() {
            "slh_dsa" => {
                let commit = engine.signing_keys.as_ref().unwrap().hardware_slh.pk.commitment();
                let target_payout = if engine.ledger.next_slh_nonce_watermark(commit) > 0 {
                    safe_payout
                } else {
                    sandbox_payout
                };
                let proof = {
                    let keys = engine.signing_keys.as_ref().unwrap();
                    let replay_nonce = 0u64;
                    let msg = CheckedWorkProof::energy_msg_with_nonce(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        target_payout,
                        replay_nonce,
                    );
                    let sig = keys.hardware_slh.sign_hash(msg);
                    CheckedWorkProof::slh_dsa_energy_deployment(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        replay_nonce,
                        keys.hardware_slh.pk.clone(),
                        sig,
                    )
                };
                if engine.ledger.next_slh_nonce_watermark(commit) > 0 {
                    engine
                        .ledger
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            target_payout,
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
                            target_payout,
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
                            target_payout,
                            &device_id,
                            Some(rule_clone.rule_hash()),
                            proof,
                        )
                        .expect_err("replayed SLH-DSA nonce must fail")
                        .to_string()
                }
            }
            _ => {
                let root = engine.signing_keys.as_ref().unwrap().hardware_xmss.root();
                let target_payout = if engine.ledger.next_merkle_leaf_watermark(root) > 0 {
                    safe_payout
                } else {
                    sandbox_payout
                };
                let proof = {
                    let keys = engine.signing_keys.as_ref().unwrap();
                    let msg = CheckedWorkProof::energy_msg(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        target_payout,
                    );
                    let replay_sig = keys
                        .hardware_xmss
                        .sign_at_index_unchecked(0, msg)
                        .expect("leaf 0 exists");
                    CheckedWorkProof::merkle_energy_deployment(
                        parcel_id,
                        kwh_delivered.max(1),
                        soil_health_score.max(rule_clone.min_soil_health_score),
                        replay_sig,
                    )
                };
                if engine.ledger.next_merkle_leaf_watermark(root) > 0 {
                    engine
                        .ledger
                        .spend_covenant(
                            input_ref,
                            &rule_clone,
                            target_payout,
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
                            target_payout,
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
                            target_payout,
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

    let proposed_change_covenant = if attack == "strip_covenant" {
        // Ensure change > 0 so covenant-stripping check fires cleanly
        if payout_love >= input_amount && input_amount > 1 {
            payout_love = (input_amount / 2).min(rule_clone.max_bounty_payout);
        }
        None
    } else {
        Some(rule_clone.rule_hash())
    };

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
                    payout_love,
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
                let msg = CheckedWorkProof::energy_msg(
                    parcel_id,
                    kwh_delivered,
                    soil_health_score,
                    payout_love,
                );
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
            // Security invariant: NEVER persist unauthenticated / rejected external telemetry
            // spam into `telemetry_history` or `/data/commons-ledger.json` (prevents disk-fill DoS).
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
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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

    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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
struct PublishSwapResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<AtomicSwapContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_publish_swap(state: &AppState, req: CompileSwapRequest) -> PublishSwapResponse {
    let contract = match compile_atomic_swap(&req) {
        Ok(c) => c,
        Err(e) => {
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            return PublishSwapResponse {
                ok: false,
                contract: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };

    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
    if engine
        .swap_offers
        .iter()
        .any(|s| s.payment_hash_hex == contract.payment_hash_hex)
    {
        return PublishSwapResponse {
            ok: false,
            contract: None,
            error: Some(format!(
                "swap with payment_hash '{}' already published",
                contract.payment_hash_hex
            )),
            state: engine.to_api_state(),
        };
    }
    engine.swap_offers.push(contract.clone());
    let _ = save_state_atomic(&state.state_file, &engine.to_persisted());

    PublishSwapResponse {
        ok: true,
        contract: Some(contract),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Debug, Deserialize)]
struct VerifySwapRequest {
    payment_hash_hex: String,
    #[serde(default)]
    love_network: Option<String>,
    #[serde(default)]
    btc_address: Option<String>,
    #[serde(default)]
    preimage_hex: Option<String>,
}

#[derive(Serialize)]
struct VerifySwapResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification: Option<DualChainSwapVerification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    state: ApiStateView,
}

fn handle_verify_swap(state: &AppState, req: VerifySwapRequest) -> VerifySwapResponse {
    let (network, stored_btc_addr) = {
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
        let existing = engine
            .swap_offers
            .iter()
            .find(|s| s.payment_hash_hex.eq_ignore_ascii_case(req.payment_hash_hex.trim()));
        let net = req
            .love_network
            .as_deref()
            .map(str::trim)
            .filter(|s| *s == "mainnet" || *s == "testnet")
            .map(ToOwned::to_owned)
            .or_else(|| existing.map(|s| s.love_network.clone()))
            .unwrap_or_else(|| "mainnet".to_string());
        let btc = req
            .btc_address
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| existing.and_then(|s| s.btc_htlc.as_ref().map(|h| h.btc_p2wsh_address.clone())));
        (net, btc)
    };

    let Some(client) = state.client_for_network(&network) else {
        let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
        return VerifySwapResponse {
            ok: false,
            verification: None,
            error: Some(format!("No live Newsatoshi L1 seed client configured for '{network}'")),
            state: engine.to_api_state(),
        };
    };

    let report = match verify_swap_on_chain(
        client,
        &network,
        &req.payment_hash_hex,
        stored_btc_addr.as_deref(),
        req.preimage_hex.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            let engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
            return VerifySwapResponse {
                ok: false,
                verification: None,
                error: Some(e),
                state: engine.to_api_state(),
            };
        }
    };

    // Update persisted swap record if one matches this payment_hash.
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(offer) = engine
        .swap_offers
        .iter_mut()
        .find(|s| s.payment_hash_hex.eq_ignore_ascii_case(&report.payment_hash_hex))
    {
        offer.love_leg_verified = report.love_confirmed;
        if let Some(ref lock) = report.love_lock {
            offer.love_lock_id = Some(lock.id.clone());
            offer.love_lock_state = Some(lock.state.clone());
            offer.love_confirmed_height = if lock.at > 0 { Some(lock.at) } else { None };
        }
        offer.btc_leg_verified = report.btc_confirmed;
        if let Some(first_utxo) = report.btc_utxos.first() {
            offer.btc_funding_txid = Some(first_utxo.txid.clone());
            offer.btc_funded_sats = Some(report.btc_total_sats);
        }
        if report.preimage_valid == Some(true) {
            if let Some(ref p) = req.preimage_hex {
                offer.verified_preimage_hex = Some(p.trim().to_string());
            }
        }
        offer.last_verified_unix = Some(report.verified_at_unix);
        let _ = save_state_atomic(&state.state_file, &engine.to_persisted());
    }

    VerifySwapResponse {
        ok: true,
        verification: Some(report),
        error: None,
        state: engine.to_api_state(),
    }
}

#[derive(Serialize)]
struct ResetResponse {
    ok: bool,
    state: ApiStateView,
}

fn handle_reset(state: &AppState) -> ResetResponse {
    let mut engine = state.engine.lock().unwrap_or_else(|e| e.into_inner());
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

fn serve_static_file(req: Request, static_dir: &Path, req_path: &str, base_url: &str) {
    let Ok(root_canon) = fs::canonicalize(static_dir) else {
        respond_problem_404(req, req_path, base_url);
        return;
    };

    let rel = req_path.trim_start_matches('/');
    let is_root = rel.is_empty() || rel == "index.html";
    let candidate = if is_root {
        root_canon.join("index.html")
    } else {
        root_canon.join(rel)
    };

    let resolved = match fs::canonicalize(&candidate) {
        Ok(p) if p.starts_with(&root_canon) && p.is_file() => p,
        _ => {
            // Return genuine RFC 9457 404 (prevents SEO / agent soft-404s on nonexistent paths)
            respond_problem_404(req, req_path, base_url);
            return;
        }
    };

    match fs::read(&resolved) {
        Ok(bytes) => {
            let mime = content_type_for_path(&resolved);
            let len = bytes.len();
            let mut headers = vec![Header::from_bytes("Content-Type", mime).unwrap()];
            if is_root {
                headers.push(Header::from_bytes("Vary", "Accept").unwrap());
                headers.push(discovery_link_header());
            }
            let mut resp = Response::new(
                StatusCode(200),
                headers,
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
