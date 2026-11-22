import type {
  AcquireParcelPayload,
  AcquireParcelResponse,
  AnchorResponse,
  CommonsState,
  ConfirmAnchorPayload,
  ConfirmAnchorResponse,
  HealthResponse,
  RecordFundingPayload,
  RecordFundingResponse,
  SeedStatusResponse,
  StreamSettlePayload,
  StreamSettleResponse,
  TelemetryPayload,
  TelemetryResponse,
  VerifyAnchorResponse,
  VerifyStarkResponse,
} from '../types';
import {
  normalizeAnchorReceipt,
  normalizeCommonsState,
  normalizeFunding,
  normalizeParcel,
  normalizeSeedResponse,
  normalizeStarkProof,
  normalizeTelemetry,
} from './normalize';

const API_BASE = ((import.meta.env.VITE_API_BASE as string | undefined) || '').replace(/\/$/, '');
const OPERATOR_TOKEN_STORAGE_KEY = 'newsatoshi_commons_operator_token';

export function getOperatorToken(): string {
  try {
    return window.sessionStorage.getItem(OPERATOR_TOKEN_STORAGE_KEY) || '';
  } catch {
    return '';
  }
}

export function setOperatorToken(token: string): void {
  try {
    const trimmed = token.trim();
    if (trimmed) {
      window.sessionStorage.setItem(OPERATOR_TOKEN_STORAGE_KEY, trimmed);
    } else {
      window.sessionStorage.removeItem(OPERATOR_TOKEN_STORAGE_KEY);
    }
  } catch {
    // ignore storage errors
  }
}

function apiUrl(path: string): string {
  return `${API_BASE}${path}`;
}

async function fetchJson<T>(
  path: string,
  init?: RequestInit,
  timeoutMs = 15000
): Promise<{ status: number; data: T }> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const token = getOperatorToken();
  try {
    const res = await fetch(apiUrl(path), {
      ...init,
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(init?.headers ?? {}),
      },
      signal: controller.signal,
    });

    const text = await res.text();
    let data: unknown = {};
    try {
      data = text ? JSON.parse(text) : {};
    } catch {
      throw new Error(`Non-JSON response (${res.status}) from ${path}`);
    }

    if (res.status === 401 || res.status === 403 || res.status === 404 || res.status >= 500) {
      const msg =
        data && typeof data === 'object' && 'error' in data && typeof (data as Record<string, unknown>).error === 'string'
          ? String((data as Record<string, unknown>).error)
          : `Commons API returned HTTP ${res.status}`;
      throw new Error(msg);
    }

    return { status: res.status, data: data as T };
  } finally {
    clearTimeout(timer);
  }
}

export async function getHealth(): Promise<HealthResponse | null> {
  try {
    const { status, data } = await fetchJson<HealthResponse>('/api/health', { method: 'GET' }, 4000);
    if (
      status !== 200 ||
      !data ||
      typeof data.ok !== 'boolean' ||
      typeof data.protocol_version !== 'string' ||
      !data.protocol_version.toUpperCase().startsWith('NEWSATOSHI')
    ) {
      return null;
    }
    return data;
  } catch {
    return null;
  }
}

export interface StateFetchResult {
  connected: boolean;
  state: CommonsState | null;
  health: HealthResponse | null;
  error: string | null;
}

export async function getCommonsState(): Promise<StateFetchResult> {
  try {
    const [{ status, data: rawState }, health] = await Promise.all([
      fetchJson<unknown>('/api/state', { method: 'GET' }, 5000),
      getHealth(),
    ]);
    if (
      status !== 200 ||
      !rawState ||
      typeof rawState !== 'object' ||
      !('covenant_rule_hash' in rawState) ||
      !('ledger_summary' in rawState)
    ) {
      return {
        connected: false,
        state: null,
        health: null,
        error: 'Endpoint did not return a valid NEWSATOSHI/2.1 /api/state payload.',
      };
    }
    return {
      connected: true,
      state: normalizeCommonsState(rawState),
      health,
      error: null,
    };
  } catch (err) {
    return {
      connected: false,
      state: null,
      health: null,
      error: err instanceof Error ? err.message : 'Unable to reach Newsatoshi Commons Rust server.',
    };
  }
}

export async function getSeedStatus(forceRefresh = false): Promise<SeedStatusResponse> {
  try {
    const query = forceRefresh ? '?refresh=1' : '';
    const { status, data: raw } = await fetchJson<unknown>(`/api/seed${query}`, { method: 'GET' }, 7000);
    if (status !== 200 || !raw || typeof raw !== 'object') {
      throw new Error('Invalid /api/seed response');
    }
    return normalizeSeedResponse(raw);
  } catch (err) {
    return {
      live: false,
      endpoint: '/api/seed',
      fetched_at_unix: Math.floor(Date.now() / 1000),
      status: null,
      mainnet_info: null,
      mainnet_live: false,
      mainnet_endpoint: null,
      mainnet_status: null,
      error: err instanceof Error ? err.message : 'Seed status endpoint unreachable.',
    };
  }
}

export async function actionRecordFunding(
  payload: RecordFundingPayload
): Promise<RecordFundingResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>('/api/actions/record-funding', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
  return {
    ok: Boolean(raw.ok),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    funding: raw.funding ? normalizeFunding(raw.funding, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionConfirmAnchor(
  payload: ConfirmAnchorPayload
): Promise<ConfirmAnchorResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>('/api/actions/confirm-anchor', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
  return {
    ok: Boolean(raw.ok),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    receipt: raw.receipt ? normalizeAnchorReceipt(raw.receipt, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionAcquireParcel(
  payload: AcquireParcelPayload
): Promise<AcquireParcelResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/acquire-parcel',
    { method: 'POST', body: JSON.stringify(payload) },
    20000
  );
  return {
    ok: Boolean(raw.ok),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    txid: typeof raw.txid === 'string' ? raw.txid : undefined,
    parcel: raw.parcel ? normalizeParcel(raw.parcel) : undefined,
    stark_proof: raw.stark_proof ? normalizeStarkProof(raw.stark_proof, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionSubmitTelemetry(payload: TelemetryPayload): Promise<TelemetryResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/telemetry',
    { method: 'POST', body: JSON.stringify({ ...payload, simulate_attack: 'none' }) },
    20000
  );
  return {
    ok: Boolean(raw.ok),
    blocked: Boolean(raw.blocked),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    packet: raw.packet ? normalizeTelemetry(raw.packet, 0) : undefined,
    stark_proof: raw.stark_proof ? normalizeStarkProof(raw.stark_proof, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionSettleStream(
  payload: StreamSettlePayload
): Promise<StreamSettleResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/stream-settle',
    { method: 'POST', body: JSON.stringify(payload) },
    20000
  );
  return {
    ok: Boolean(raw.ok),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    gross_love: typeof raw.gross_love === 'number' ? raw.gross_love : undefined,
    reinvested_love: typeof raw.reinvested_love === 'number' ? raw.reinvested_love : undefined,
    operator_paid_love: typeof raw.operator_paid_love === 'number' ? raw.operator_paid_love : undefined,
    stark_proof: raw.stark_proof ? normalizeStarkProof(raw.stark_proof, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionAnchorL1(network: 'testnet' | 'mainnet'): Promise<AnchorResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/anchor',
    { method: 'POST', body: JSON.stringify({ network }) },
    15000
  );
  return {
    ok: Boolean(raw.ok),
    error: typeof raw.error === 'string' ? raw.error : undefined,
    receipt: raw.receipt ? normalizeAnchorReceipt(raw.receipt, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
  };
}

export async function actionVerifyAnchor(anchorHash?: string): Promise<VerifyAnchorResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/verify-anchor',
    {
      method: 'POST',
      body: JSON.stringify(anchorHash ? { anchor_hash: anchorHash } : {}),
    },
    12000
  );
  return {
    ok: Boolean(raw.ok),
    verified_on_l1: Boolean(raw.verified_on_l1),
    receipt: raw.receipt ? normalizeAnchorReceipt(raw.receipt, 0) : undefined,
    state: raw.state ? normalizeCommonsState(raw.state) : undefined,
    error: typeof raw.error === 'string' ? raw.error : undefined,
  };
}

export async function actionVerifyStark(proofId: string): Promise<VerifyStarkResponse> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/verify-stark',
    { method: 'POST', body: JSON.stringify({ proof_id: proofId }) },
    12000
  );
  return {
    ok: Boolean(raw.ok),
    valid: Boolean(raw.valid ?? raw.ok),
    proof_id: typeof raw.proof_id === 'string' ? raw.proof_id : proofId,
    verify_ms: typeof raw.verify_ms === 'number' ? raw.verify_ms : 0,
    proof_bytes_len: typeof raw.proof_bytes_len === 'number' ? raw.proof_bytes_len : 0,
    security_bits: typeof raw.security_bits === 'number' ? raw.security_bits : 0,
    error: typeof raw.error === 'string' ? raw.error : undefined,
  };
}

export async function actionResetLedger(): Promise<{ ok: boolean; state: CommonsState }> {
  const { data: raw } = await fetchJson<Record<string, unknown>>(
    '/api/actions/reset',
    { method: 'POST', body: '{}' },
    10000
  );
  return {
    ok: Boolean(raw.ok),
    state: normalizeCommonsState(raw.state),
  };
}
