import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowUpRight,
  CheckCircle2,
  KeyRound,
  Loader2,
  Lock,
  RefreshCw,
  RotateCcw,
  Terminal,
  Unlock,
  WifiOff,
} from 'lucide-react';
import {
  actionAcquireParcel,
  actionAnchorL1,
  actionConfirmAnchor,
  actionRecordFunding,
  actionResetLedger,
  actionSettleStream,
  actionSubmitTelemetry,
  actionVerifyAnchor,
  actionVerifyStark,
  getBackingSummary,
  getCommonsState,
  getOperatorToken,
  getSeedStatus,
  setOperatorToken,
} from './api/client';
import { ChainExplorerTab } from './components/ChainExplorerTab';
import { CockpitTab } from './components/CockpitTab';
import { EconomicsTab } from './components/EconomicsTab';
import { LegalAirlockTab } from './components/LegalAirlockTab';
import { OverviewPage } from './components/OverviewPage';
import { SwapsMiningTab } from './components/SwapsMiningTab';
import { Web4ForgeTab } from './components/Web4ForgeTab';
import type {
  AcquireParcelPayload,
  BackingProofSummary,
  CommonsState,
  ConfirmAnchorPayload,
  HealthResponse,
  PrimaryView,
  ProtocolSubTab,
  RecordFundingPayload,
  SeedStatusResponse,
  StreamSettlePayload,
  TelemetryPayload,
  VerifyStarkResponse,
} from './types';

interface ToastNotice {
  id: number;
  kind: 'success' | 'error';
  message: string;
}

const PRIMARY_NAV: { id: PrimaryView; label: string }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'build', label: 'Back & Build' },
  { id: 'bridge', label: 'Bridge & Mine' },
  { id: 'protocol', label: 'Protocol' },
];

const PROTOCOL_TABS: { id: ProtocolSubTab; label: string }[] = [
  { id: 'explorer', label: 'L1 Explorer' },
  { id: 'cockpit', label: 'Commons Ledger' },
  { id: 'economics', label: 'Unit Economics' },
  { id: 'legal', label: 'Legal Airlock' },
];

function readInitialView(): PrimaryView {
  try {
    const p = new URLSearchParams(window.location.search).get('view');
    if (p === 'build' || p === 'bridge' || p === 'protocol' || p === 'overview') return p;
  } catch {
    // ignore
  }
  return 'overview';
}

export default function App() {
  const [view, setView] = useState<PrimaryView>(readInitialView);
  const [protocolTab, setProtocolTab] = useState<ProtocolSubTab>('explorer');
  const [loading, setLoading] = useState<boolean>(true);
  const [connected, setConnected] = useState<boolean>(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [state, setState] = useState<CommonsState | null>(null);
  const [seed, setSeed] = useState<SeedStatusResponse | null>(null);
  const [backing, setBacking] = useState<BackingProofSummary | null>(null);
  const [seedRefreshing, setSeedRefreshing] = useState<boolean>(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [toast, setToast] = useState<ToastNotice | null>(null);
  const [authModalOpen, setAuthModalOpen] = useState<boolean>(false);
  const [operatorTokenInput, setOperatorTokenInput] = useState<string>(() => getOperatorToken());
  const [hasOperatorToken, setHasOperatorToken] = useState<boolean>(() => Boolean(getOperatorToken()));
  const toastTimer = useRef<number | null>(null);

  const navigate = useCallback((nextView: PrimaryView, subTab?: ProtocolSubTab) => {
    setView(nextView);
    if (subTab) setProtocolTab(subTab);
    try {
      const url = new URL(window.location.href);
      if (nextView === 'overview') url.searchParams.delete('view');
      else url.searchParams.set('view', nextView);
      window.history.replaceState({}, '', url.toString());
    } catch {
      // ignore
    }
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }, []);

  const showToast = useCallback((kind: 'success' | 'error', message: string) => {
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    setToast({ id: Date.now(), kind, message });
    toastTimer.current = window.setTimeout(() => setToast(null), 4500);
  }, []);

  const refreshState = useCallback(async () => {
    const res = await getCommonsState();
    setConnected(res.connected);
    setHealth(res.health);
    setState(res.state);
    setConnectionError(res.error);
    setLoading(false);
  }, []);

  const refreshSeed = useCallback(async (force = false) => {
    setSeedRefreshing(true);
    try {
      const res = await getSeedStatus(force);
      setSeed(res);
    } finally {
      setSeedRefreshing(false);
    }
  }, []);

  const refreshBacking = useCallback(async (force = false) => {
    const res = await getBackingSummary(force);
    if (res) setBacking(res);
  }, []);

  useEffect(() => {
    void refreshState();
    void refreshSeed();
    void refreshBacking();
  }, [refreshState, refreshSeed, refreshBacking]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshSeed();
      if (!connected) {
        void refreshState();
      }
    }, 15000);
    return () => window.clearInterval(interval);
  }, [connected, refreshSeed, refreshState]);

  const handleRecordFunding = async (payload: RecordFundingPayload) => {
    setBusyAction('fund');
    try {
      const res = await actionRecordFunding(payload);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast('success', `Recorded +${payload.amount_love.toLocaleString()} LOVE L1 funding`);
      } else {
        showToast('error', res.error || 'Failed to record L1 funding');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Funding error');
    } finally {
      setBusyAction(null);
    }
  };

  const handleAcquireParcel = async (payload: AcquireParcelPayload) => {
    setBusyAction('acquire');
    try {
      const res = await actionAcquireParcel(payload);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast('success', `Recorded Parcel #${payload.parcel_id} deed + verified STARK`);
      } else {
        showToast('error', res.error || 'Parcel acquisition rejected');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Parcel acquisition failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleSubmitTelemetry = async (payload: TelemetryPayload) => {
    setBusyAction('telemetry');
    try {
      const res = await actionSubmitTelemetry(payload);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast(
          'success',
          `Verified +${payload.kwh_delivered.toLocaleString()} kWh (Soil ${payload.soil_health_score}/100)`
        );
      } else {
        showToast('error', res.error || 'Telemetry rejected by covenant');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Telemetry submission failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleSettleStream = async (payload: StreamSettlePayload) => {
    setBusyAction('stream');
    try {
      const res = await actionSettleStream(payload);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast(
          'success',
          `PayWord settled (+${(res.reinvested_love ?? 0).toLocaleString()} LOVE reinvested)`
        );
      } else {
        showToast('error', res.error || 'PayWord stream settlement failed');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'PayWord stream settlement failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleAnchorL1 = async (network: 'testnet' | 'mainnet') => {
    setBusyAction('anchor');
    try {
      const res = await actionAnchorL1(network);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast('success', `Prepared unbroadcast ${network.toUpperCase()} anchor commitment`);
      } else {
        showToast('error', res.error || 'Anchor preparation failed');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Anchor preparation failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleConfirmAnchor = async (payload: ConfirmAnchorPayload) => {
    setBusyAction('confirm-anchor');
    try {
      const res = await actionConfirmAnchor(payload);
      if (res.state) setState(res.state);
      if (res.ok) {
        showToast('success', 'Confirmed real L1 broadcast txid on anchor receipt');
      } else {
        showToast('error', res.error || 'Failed to confirm L1 txid');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Confirm anchor failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleVerifyAnchor = async (anchorHash: string) => {
    setBusyAction('verify-anchor');
    try {
      const res = await actionVerifyAnchor(anchorHash);
      if (res.state) setState(res.state);
      if (res.verified_on_l1 && res.receipt) {
        showToast(
          'success',
          `Verified on Newsatoshi L1 · Block #${res.receipt.l1_confirmed_height ?? '?'} (${res.receipt.l1_confirmations ?? 0} confs)`
        );
      } else {
        showToast('error', res.error || 'Anchor hash not yet confirmed on L1');
      }
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'On-chain L1 verification failed');
    } finally {
      setBusyAction(null);
    }
  };

  const handleVerifyStark = async (proofId: string): Promise<VerifyStarkResponse | null> => {
    try {
      const res = await actionVerifyStark(proofId);
      if (res.valid) {
        showToast('success', `Winterfell STARK ${proofId} verified in ${res.verify_ms}ms`);
      } else {
        showToast('error', res.error || `STARK verification failed for ${proofId}`);
      }
      return res;
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'STARK verification error');
      return null;
    }
  };

  const handleReset = async () => {
    setBusyAction('reset');
    try {
      const res = await actionResetLedger();
      setState(res.state);
      showToast('success', 'Ledger cleared to 0 LOVE empty state');
    } catch (err) {
      showToast('error', err instanceof Error ? err.message : 'Reset failed');
    } finally {
      setBusyAction(null);
    }
  };

  const mainnetHeight = seed?.mainnet_live ? seed.mainnet_status?.height : null;

  return (
    <div className="bg-studio-canvas flex min-h-screen flex-col text-zinc-100">
      {/* Single Minimal 60px Navigation Bar */}
      <header className="sticky top-0 z-30 border-b border-white/[0.07] bg-[#08080a]/85 backdrop-blur-xl">
        <div className="mx-auto flex h-15 max-w-[1200px] items-center justify-between gap-4 px-4 sm:px-6">
          {/* Bespoke Geometric Monogram + Wordmark */}
          <button
            type="button"
            onClick={() => navigate('overview')}
            className="group flex items-center gap-2.5 text-left cursor-pointer"
          >
            <svg
              viewBox="0 0 28 28"
              fill="none"
              className="h-6 w-6 shrink-0 text-zinc-100 transition-transform duration-200 group-hover:scale-105"
              aria-hidden="true"
            >
              <circle cx="14" cy="14" r="11.5" stroke="currentColor" strokeWidth="1.5" />
              <circle cx="14" cy="14" r="3.5" fill="currentColor" />
            </svg>
            <span className="text-[15px] font-semibold tracking-tight text-zinc-100">
              Newsatoshi
            </span>
          </button>

          {/* 4 Primary Views */}
          <nav className="flex items-center gap-1 overflow-x-auto" aria-label="Primary">
            {PRIMARY_NAV.map((item) => {
              const active = view === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => navigate(item.id)}
                  className={`shrink-0 rounded-full px-3.5 py-1.5 text-[13px] font-medium transition cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-white/30 ${
                    active
                      ? 'bg-white/[0.09] text-zinc-50'
                      : 'text-zinc-400 hover:text-zinc-100'
                  }`}
                >
                  {item.label}
                </button>
              );
            })}
          </nav>

          {/* Right: Quiet Live Mainnet Pill + Whitepaper Link */}
          <div className="hidden items-center gap-3 md:flex">
            <button
              type="button"
              onClick={() => navigate('protocol', 'explorer')}
              title="Inspect live Mainnet & Testnet blocks"
              className="inline-flex items-center gap-2 rounded-full border border-white/[0.08] bg-white/[0.02] px-3 py-1 font-mono text-[11px] text-zinc-300 transition hover:border-white/[0.18] hover:text-white cursor-pointer"
            >
              <span
                className={`h-1.5 w-1.5 rounded-full ${
                  mainnetHeight ? 'bg-emerald-400' : connected ? 'bg-amber-400' : 'bg-red-400'
                }`}
              />
              <span className="tabular-nums">
                {mainnetHeight ? `Mainnet #${mainnetHeight.toLocaleString()}` : 'Mainnet'}
              </span>
            </button>

            <a
              href="/whitepaper"
              className="inline-flex items-center gap-1 text-[13px] font-medium text-zinc-400 transition hover:text-zinc-100"
            >
              <span>Whitepaper</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
        </div>
      </header>

      {/* Subtle Toast Notification */}
      {toast && (
        <div className="fixed bottom-5 right-5 z-50 max-w-md">
          <div
            className={`flex items-center gap-2.5 rounded-xl border px-4 py-3 text-xs font-medium shadow-2xl backdrop-blur-xl ${
              toast.kind === 'success'
                ? 'border-emerald-500/35 bg-[#0f0f12]/95 text-emerald-200'
                : 'border-red-500/35 bg-[#0f0f12]/95 text-red-200'
            }`}
          >
            <CheckCircle2 className="h-4 w-4 shrink-0" />
            <span>{toast.message}</span>
          </div>
        </div>
      )}

      {/* Main Content */}
      <main className="mx-auto w-full max-w-[1200px] flex-1 px-4 sm:px-6">
        {view === 'overview' ? (
          <OverviewPage seed={seed} backing={backing} onNavigate={navigate} />
        ) : view === 'build' ? (
          <div className="animate-fade-in py-10">
            <div className="mb-8 flex flex-wrap items-end justify-between gap-4 border-b border-white/[0.07] pb-6">
              <div>
                <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                  Back &amp; Build
                </p>
                <h1 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-50 sm:text-5xl">
                  Proof of Backing &amp; Web4 Forge
                </h1>
                <p className="mt-2 max-w-2xl text-sm text-zinc-400">
                  Live on-chain reserves across 4 EVM mainnets, plus a deterministic charter
                  compiler enforcing Lean 4 monotonic spending bounds and ERC-8004 identity.
                </p>
              </div>
              {connected && health?.auth_required && (
                <button
                  type="button"
                  onClick={() => setAuthModalOpen((v) => !v)}
                  className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.1] bg-white/[0.03] px-3 py-1.5 text-xs font-medium text-zinc-300 transition hover:border-white/[0.2] hover:text-white cursor-pointer"
                >
                  {hasOperatorToken ? <Unlock className="h-3.5 w-3.5 text-emerald-400" /> : <Lock className="h-3.5 w-3.5 text-zinc-500" />}
                  <span>{hasOperatorToken ? 'Operator Key Active' : 'Operator Publish Key'}</span>
                </button>
              )}
            </div>

            {authModalOpen && health?.auth_required && (
              <div className="mb-6 rounded-2xl border border-white/[0.09] bg-[#0f0f12] p-4">
                <form
                  className="flex flex-wrap items-center gap-2.5 text-xs"
                  onSubmit={(e) => {
                    e.preventDefault();
                    setOperatorToken(operatorTokenInput);
                    const active = Boolean(operatorTokenInput.trim());
                    setHasOperatorToken(active);
                    setAuthModalOpen(false);
                    showToast(
                      'success',
                      active ? 'Operator token saved for this session' : 'Operator token cleared'
                    );
                  }}
                >
                  <KeyRound className="h-4 w-4 shrink-0 text-zinc-400" />
                  <input
                    type="password"
                    placeholder="Optional COMMONS_ADMIN_TOKEN (stored in sessionStorage only)"
                    value={operatorTokenInput}
                    onChange={(e) => setOperatorTokenInput(e.target.value)}
                    className="min-w-[240px] flex-1 rounded-lg border border-white/[0.1] bg-[#0a0a0d] px-3 py-2 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-white/30 focus:outline-none"
                  />
                  <button
                    type="submit"
                    className="rounded-lg bg-zinc-100 px-3.5 py-2 font-semibold text-zinc-950 hover:bg-white cursor-pointer"
                  >
                    Save
                  </button>
                  {hasOperatorToken && (
                    <button
                      type="button"
                      onClick={() => {
                        setOperatorToken('');
                        setOperatorTokenInput('');
                        setHasOperatorToken(false);
                        setAuthModalOpen(false);
                        showToast('success', 'Returned to read-only mode');
                      }}
                      className="rounded-lg border border-white/[0.1] px-3 py-2 text-zinc-400 hover:text-zinc-100 cursor-pointer"
                    >
                      Clear
                    </button>
                  )}
                </form>
              </div>
            )}

            <Web4ForgeTab
              hasOperatorToken={hasOperatorToken}
              onStateUpdated={setState}
              onNotice={showToast}
            />
          </div>
        ) : view === 'bridge' ? (
          <div className="animate-fade-in py-10">
            <div className="mb-8 border-b border-white/[0.07] pb-6">
              <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                Bridge &amp; Mine
              </p>
              <h1 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-50 sm:text-5xl">
                Zero-Custody Buy Roads &amp; AuxPoW Mining
              </h1>
              <p className="mt-2 max-w-2xl text-sm text-zinc-400">
                Compile SHA-256 atomic HTLCs from Bitcoin, Lightning, Solana, Base, Arbitrum,
                Optimism, or Polygon, verify both legs live, or calculate Bitcoin merge-mining
                yield.
              </p>
            </div>
            <SwapsMiningTab
              state={state}
              seed={seed}
              hasOperatorToken={hasOperatorToken}
              onStateUpdated={setState}
              onNotice={showToast}
            />
          </div>
        ) : (
          <div className="animate-fade-in py-10">
            <div className="mb-6 flex flex-wrap items-end justify-between gap-4 border-b border-white/[0.07] pb-6">
              <div>
                <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
                  Protocol Verification
                </p>
                <h1 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-50 sm:text-5xl">
                  Chain, Covenants &amp; Legal Airlock
                </h1>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                {PROTOCOL_TABS.map((tab) => {
                  const active = protocolTab === tab.id;
                  return (
                    <button
                      key={tab.id}
                      type="button"
                      onClick={() => setProtocolTab(tab.id)}
                      className={`rounded-full px-3.5 py-1.5 text-xs font-medium transition cursor-pointer ${
                        active
                          ? 'bg-zinc-100 text-zinc-950'
                          : 'border border-white/[0.08] text-zinc-400 hover:border-white/[0.18] hover:text-zinc-100'
                      }`}
                    >
                      {tab.label}
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Discreet Operator Controls ONLY inside Commons Ledger */}
            {protocolTab === 'cockpit' && connected && (
              <div className="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-white/[0.07] bg-[#0f0f12] px-4 py-3 text-xs text-zinc-400">
                <div className="flex flex-wrap items-center gap-2.5">
                  <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                  <span className="font-mono text-[11px] text-zinc-300">
                    Verify-Only Gateway · 0 Server Signing Keys · v{health?.version ?? '0.11.0'}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  {health?.auth_required && (
                    <button
                      type="button"
                      onClick={() => setAuthModalOpen((v) => !v)}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.09] px-2.5 py-1 text-[11px] font-medium text-zinc-300 hover:border-white/[0.2] hover:text-white cursor-pointer"
                    >
                      {hasOperatorToken ? (
                        <Unlock className="h-3 w-3 text-emerald-400" />
                      ) : (
                        <Lock className="h-3 w-3 text-zinc-500" />
                      )}
                      <span>{hasOperatorToken ? 'Operator Unlocked' : 'Operator Unlock'}</span>
                    </button>
                  )}
                  {(!health?.auth_required || hasOperatorToken) && (
                    <button
                      type="button"
                      onClick={() => void handleReset()}
                      disabled={busyAction !== null}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.09] px-2.5 py-1 text-[11px] font-medium text-zinc-400 hover:text-zinc-100 cursor-pointer disabled:opacity-50"
                    >
                      <RotateCcw className="h-3 w-3" />
                      <span>Clear Ledger</span>
                    </button>
                  )}
                </div>
              </div>
            )}

            {protocolTab === 'cockpit' && authModalOpen && health?.auth_required && (
              <div className="mb-6 rounded-2xl border border-white/[0.09] bg-[#0f0f12] p-4">
                <form
                  className="flex flex-wrap items-center gap-2.5 text-xs"
                  onSubmit={(e) => {
                    e.preventDefault();
                    setOperatorToken(operatorTokenInput);
                    const active = Boolean(operatorTokenInput.trim());
                    setHasOperatorToken(active);
                    setAuthModalOpen(false);
                    showToast(
                      'success',
                      active ? 'Operator token saved for this session' : 'Operator token cleared'
                    );
                  }}
                >
                  <KeyRound className="h-4 w-4 shrink-0 text-zinc-400" />
                  <input
                    type="password"
                    placeholder="Paste COMMONS_ADMIN_TOKEN (sessionStorage only)"
                    value={operatorTokenInput}
                    onChange={(e) => setOperatorTokenInput(e.target.value)}
                    className="min-w-[240px] flex-1 rounded-lg border border-white/[0.1] bg-[#0a0a0d] px-3 py-2 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-white/30 focus:outline-none"
                  />
                  <button
                    type="submit"
                    className="rounded-lg bg-zinc-100 px-3.5 py-2 font-semibold text-zinc-950 hover:bg-white cursor-pointer"
                  >
                    Save
                  </button>
                </form>
              </div>
            )}

            {loading ? (
              <div className="flex items-center justify-center gap-2 py-20 text-xs text-zinc-500">
                <Loader2 className="h-4 w-4 animate-spin text-zinc-300" />
                <span>Connecting to Newsatoshi Commons node…</span>
              </div>
            ) : protocolTab === 'explorer' ? (
              <ChainExplorerTab
                seed={seed}
                anchorReceipts={state?.anchor_receipts ?? []}
                refreshing={seedRefreshing}
                onRefresh={() => void refreshSeed(true)}
              />
            ) : protocolTab === 'economics' ? (
              <EconomicsTab />
            ) : !connected || !state ? (
              <div className="surface-card mx-auto max-w-2xl rounded-2xl p-6">
                <div className="flex items-center gap-2.5 text-red-300">
                  <WifiOff className="h-5 w-5" />
                  <h2 className="text-base font-medium">Commons Engine Disconnected</h2>
                </div>
                <p className="mt-2 text-xs leading-relaxed text-zinc-400">
                  Cannot reach <code className="font-mono text-zinc-200">/api/state</code>
                  {connectionError ? ` (${connectionError})` : ''}. No simulated fallback data is
                  displayed while disconnected.
                </p>
                <div className="mt-4 rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3">
                  <div className="mb-1 flex items-center gap-1.5 font-mono text-[10px] uppercase text-zinc-400">
                    <Terminal className="h-3.5 w-3.5" />
                    <span>Start Local Node</span>
                  </div>
                  <code className="block break-all font-mono text-[11px] text-zinc-300">
                    newsatoshi-commons serve --port 8791 --state-file ./data/commons-ledger.json
                  </code>
                </div>
                <button
                  type="button"
                  onClick={() => void refreshState()}
                  className="mt-4 inline-flex items-center gap-2 rounded-full bg-zinc-100 px-4 py-2 text-xs font-semibold text-zinc-950 hover:bg-white cursor-pointer"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  <span>Retry</span>
                </button>
              </div>
            ) : protocolTab === 'cockpit' ? (
              <CockpitTab
                state={state}
                seed={seed}
                busyAction={busyAction}
                onRecordFunding={handleRecordFunding}
                onAcquireParcel={handleAcquireParcel}
                onSubmitTelemetry={handleSubmitTelemetry}
                onSettleStream={handleSettleStream}
                onAnchorL1={handleAnchorL1}
                onConfirmAnchor={handleConfirmAnchor}
                onVerifyAnchor={handleVerifyAnchor}
                onVerifyStark={handleVerifyStark}
              />
            ) : (
              <LegalAirlockTab state={state} seed={seed} />
            )}
          </div>
        )}
      </main>

      {/* Minimalist Architectural Footer */}
      <footer className="border-t border-white/[0.07] bg-[#08080a]">
        <div className="mx-auto flex max-w-[1200px] flex-wrap items-center justify-between gap-4 px-4 py-7 text-xs text-zinc-500 sm:px-6">
          <div className="flex flex-wrap items-center gap-3">
            <span className="font-medium text-zinc-300">Newsatoshi Commons</span>
            <span className="text-zinc-700">·</span>
            <span className="font-mono text-[11px]">
              v{health?.version ?? '0.11.0'} · Verify-Only · 0 Server Keys
            </span>
          </div>

          <div className="flex flex-wrap items-center gap-5">
            <a href="https://newsatoshi.org/" className="transition hover:text-zinc-200">
              newsatoshi.org
            </a>
            <a href="/whitepaper" className="transition hover:text-zinc-200">
              Whitepaper
            </a>
            <a href="/formal/NewsatoshiCovenants.lean" className="transition hover:text-zinc-200">
              Lean 4 Proof
            </a>
            <a href="/developers" className="transition hover:text-zinc-200">
              Developers
            </a>
            <a href="/openapi.json" className="transition hover:text-zinc-200">
              OpenAPI
            </a>
            <a href="/.well-known/erc8004.json" className="transition hover:text-zinc-200">
              ERC-8004
            </a>
            <a
              href="https://github.com/theguysaccount/newsatoshi-commons"
              target="_blank"
              rel="noreferrer"
              className="transition hover:text-zinc-200"
            >
              GitHub
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
}
