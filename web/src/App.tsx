import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Activity,
  ArrowLeftRight,
  Calculator,
  CheckCircle2,
  Flame,
  Globe,
  KeyRound,
  Loader2,
  Lock,
  RefreshCw,
  RotateCcw,
  Scale,
  Server,
  ShieldCheck,
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
  getCommonsState,
  getOperatorToken,
  getSeedStatus,
  setOperatorToken,
} from './api/client';
import { ChainExplorerTab } from './components/ChainExplorerTab';
import { CockpitTab } from './components/CockpitTab';
import { EconomicsTab } from './components/EconomicsTab';
import { InnovationHero } from './components/InnovationHero';
import { LegalAirlockTab } from './components/LegalAirlockTab';
import { SwapsMiningTab } from './components/SwapsMiningTab';
import { TopL1Rail } from './components/TopL1Rail';
import type {
  AcquireParcelPayload,
  CommonsState,
  ConfirmAnchorPayload,
  HealthResponse,
  RecordFundingPayload,
  SeedStatusResponse,
  StreamSettlePayload,
  TelemetryPayload,
  VerifyStarkResponse,
  WorkspaceTab,
} from './types';

interface ToastNotice {
  id: number;
  kind: 'success' | 'error';
  message: string;
}

export default function App() {
  const [activeTab, setActiveTab] = useState<WorkspaceTab>('cockpit');
  const [loading, setLoading] = useState<boolean>(true);
  const [connected, setConnected] = useState<boolean>(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [state, setState] = useState<CommonsState | null>(null);
  const [seed, setSeed] = useState<SeedStatusResponse | null>(null);
  const [seedRefreshing, setSeedRefreshing] = useState<boolean>(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [toast, setToast] = useState<ToastNotice | null>(null);
  const [authModalOpen, setAuthModalOpen] = useState<boolean>(false);
  const [operatorTokenInput, setOperatorTokenInput] = useState<string>(() => getOperatorToken());
  const [hasOperatorToken, setHasOperatorToken] = useState<boolean>(() => Boolean(getOperatorToken()));
  const toastTimer = useRef<number | null>(null);

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

  useEffect(() => {
    void refreshState();
    void refreshSeed();
  }, [refreshState, refreshSeed]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshSeed();
      if (!connected) {
        void refreshState();
      }
    }, 10000);
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
          `Verified on-chain on Newsatoshi L1! Block #${res.receipt.l1_confirmed_height ?? '?'} (${res.receipt.l1_confirmations ?? 0} confirmations)`
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

  const tabs: { id: WorkspaceTab; label: string; icon: typeof Activity }[] = [
    { id: 'cockpit', label: 'Commons Ledger', icon: Activity },
    { id: 'explorer', label: 'Live L1 Explorer', icon: Globe },
    { id: 'swaps', label: 'Swaps & Mining', icon: ArrowLeftRight },
    { id: 'economics', label: 'Unit Economics Calculator', icon: Calculator },
    { id: 'legal', label: 'Legal Airlock', icon: Scale },
  ];

  return (
    <div className="bg-obsidian-grid flex min-h-screen flex-col text-slate-100">
      <TopL1Rail seed={seed} refreshing={seedRefreshing} onRefresh={() => void refreshSeed(true)} />

      <header className="sticky top-0 z-30 border-b border-slate-800/80 bg-[#06090f]/85 backdrop-blur-md">
        <div className="mx-auto flex max-w-[1480px] flex-wrap items-center justify-between gap-4 px-4 py-3.5 sm:px-6">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-emerald-500/40 bg-emerald-500/10">
              <Flame className="h-5 w-5 text-emerald-400" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-base font-bold tracking-tight text-slate-50 sm:text-lg">
                  Newsatoshi Commons
                </h1>
                <span className="rounded border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-emerald-300">
                  {state?.protocol_version || 'NEWSATOSHI/2.1'}
                </span>
              </div>
              <p className="text-[11px] text-slate-400">
                Quantum-Proof Sound Money · Free Bitcoin Security · AI Agent Payments
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2.5">
            {connected ? (
              <div className="inline-flex items-center gap-2 rounded-full border border-emerald-500/45 bg-emerald-500/15 px-3 py-1.5 text-xs font-medium text-emerald-200">
                <span className="relative flex h-2 w-2">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400" />
                </span>
                <Server className="h-3.5 w-3.5 text-emerald-300" />
                <span className="font-mono text-[11px] font-semibold">Live Rust Node</span>
                {health && (
                  <span className="font-mono text-[10px] text-emerald-300/80">v{health.version}</span>
                )}
              </div>
            ) : (
              <button
                type="button"
                onClick={() => void refreshState()}
                className="inline-flex items-center gap-2 rounded-full border border-red-500/45 bg-red-500/15 px-3 py-1.5 text-xs font-medium text-red-200 hover:bg-red-500/25 cursor-pointer"
              >
                <WifiOff className="h-3.5 w-3.5 text-red-400" />
                <span className="font-mono text-[11px] font-semibold">Node Disconnected</span>
                <RefreshCw className="h-3 w-3" />
              </button>
            )}

            {connected && (health?.verify_only || state?.verify_only) && (
              <div
                className="inline-flex items-center gap-1.5 rounded-full border border-sky-500/40 bg-sky-500/10 px-3 py-1.5 font-mono text-[11px] font-semibold text-sky-200"
                title="Mainnet key-isolation mode: zero private signing keys exist on this server. Only offline-signed XMSS/SLH-DSA hardware & deed packets are accepted."
              >
                <ShieldCheck className="h-3.5 w-3.5 text-sky-300" />
                Verify-Only · 0 Server Keys
              </div>
            )}

            {connected && health?.auth_required && (
              <button
                type="button"
                onClick={() => setAuthModalOpen((v) => !v)}
                className={`inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 font-mono text-[11px] font-medium transition cursor-pointer ${
                  hasOperatorToken
                    ? 'border-emerald-500/45 bg-emerald-500/15 text-emerald-200 hover:bg-emerald-500/25'
                    : 'border-amber-500/40 bg-amber-500/10 text-amber-200 hover:bg-amber-500/20'
                }`}
              >
                {hasOperatorToken ? (
                  <>
                    <Unlock className="h-3.5 w-3.5" />
                    Operator Unlocked
                  </>
                ) : (
                  <>
                    <Lock className="h-3.5 w-3.5" />
                    Public Read-Only · Unlock
                  </>
                )}
              </button>
            )}

            {connected && (!health?.auth_required || hasOperatorToken) && (
              <button
                type="button"
                onClick={() => void handleReset()}
                disabled={busyAction !== null}
                className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/90 bg-slate-900/90 px-3 py-1.5 font-mono text-[11px] font-medium text-slate-300 hover:border-slate-500 hover:text-white cursor-pointer disabled:opacity-50"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                Clear Ledger
              </button>
            )}
          </div>
        </div>

        {authModalOpen && health?.auth_required && (
          <div className="border-t border-slate-800/80 bg-slate-950/95">
            <form
              className="mx-auto flex max-w-[1480px] flex-wrap items-center gap-2.5 px-4 py-2.5 text-xs sm:px-6"
              onSubmit={(e) => {
                e.preventDefault();
                setOperatorToken(operatorTokenInput);
                const active = Boolean(operatorTokenInput.trim());
                setHasOperatorToken(active);
                setAuthModalOpen(false);
                showToast(
                  'success',
                  active ? 'Operator token saved for this browser session' : 'Operator token cleared'
                );
              }}
            >
              <KeyRound className="h-4 w-4 text-amber-400 shrink-0" />
              <span className="text-slate-300">Operator Bearer Token:</span>
              <input
                type="password"
                placeholder="Paste COMMONS_ADMIN_TOKEN (stored in sessionStorage only)"
                value={operatorTokenInput}
                onChange={(e) => setOperatorTokenInput(e.target.value)}
                className="min-w-[240px] flex-1 rounded-md border border-slate-700 bg-slate-900 px-3 py-1.5 font-mono text-xs text-slate-100 placeholder:text-slate-500 focus:border-emerald-500 focus:outline-none"
              />
              <button
                type="submit"
                className="rounded-md bg-emerald-500 px-3 py-1.5 font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer"
              >
                Save Token
              </button>
              {hasOperatorToken && (
                <button
                  type="button"
                  onClick={() => {
                    setOperatorToken('');
                    setOperatorTokenInput('');
                    setHasOperatorToken(false);
                    setAuthModalOpen(false);
                    showToast('success', 'Returned to public read-only mode');
                  }}
                  className="rounded-md border border-slate-700 px-3 py-1.5 text-slate-300 hover:border-slate-500 cursor-pointer"
                >
                  Lock
                </button>
              )}
            </form>
          </div>
        )}

        <nav className="mx-auto flex max-w-[1480px] gap-1 overflow-x-auto px-4 sm:px-6">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const active = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`inline-flex shrink-0 items-center gap-2 border-b-2 px-3.5 py-2.5 text-xs font-medium transition cursor-pointer ${
                  active
                    ? 'border-emerald-400 text-emerald-300'
                    : 'border-transparent text-slate-400 hover:text-slate-200'
                }`}
              >
                <Icon className="h-3.5 w-3.5" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </nav>
      </header>

      {toast && (
        <div className="fixed bottom-5 right-5 z-50 max-w-md">
          <div
            className={`flex items-center gap-2.5 rounded-xl border px-4 py-3 text-xs font-medium shadow-2xl backdrop-blur-md ${
              toast.kind === 'success'
                ? 'border-emerald-500/50 bg-slate-950/95 text-emerald-200'
                : 'border-red-500/50 bg-slate-950/95 text-red-200'
            }`}
          >
            <CheckCircle2 className="h-4 w-4 shrink-0" />
            <span>{toast.message}</span>
          </div>
        </div>
      )}

      <main className="mx-auto w-full max-w-[1480px] flex-1 px-4 py-6 sm:px-6">
        <InnovationHero seed={seed} onNavigateTab={setActiveTab} />
        {loading ? (
          <div className="flex items-center justify-center gap-2 py-20 text-xs text-slate-400">
            <Loader2 className="h-4 w-4 animate-spin text-emerald-400" />
            Connecting to local Newsatoshi Commons Rust node…
          </div>
        ) : activeTab === 'explorer' ? (
          <ChainExplorerTab
            seed={seed}
            anchorReceipts={state?.anchor_receipts ?? []}
            refreshing={seedRefreshing}
            onRefresh={() => void refreshSeed(true)}
          />
        ) : activeTab === 'swaps' ? (
          <SwapsMiningTab
            state={state}
            seed={seed}
            hasOperatorToken={hasOperatorToken}
            onStateUpdated={setState}
            onNotice={showToast}
          />
        ) : activeTab === 'economics' ? (
          <EconomicsTab />
        ) : !connected || !state ? (
          <div className="mx-auto max-w-2xl rounded-xl border border-red-500/35 bg-slate-950/85 p-6">
            <div className="flex items-center gap-2.5 text-red-300">
              <WifiOff className="h-5 w-5" />
              <h2 className="text-base font-semibold">Rust Commons Engine Disconnected</h2>
            </div>
            <p className="mt-2 text-xs leading-relaxed text-slate-300">
              Cannot reach <code className="font-mono text-slate-100">/api/state</code>
              {connectionError ? ` (${connectionError})` : ''}. No fallback or simulated ledger data
              is rendered while disconnected.
            </p>
            <div className="mt-4 rounded-lg border border-slate-800 bg-[#04070d] p-3">
              <div className="mb-1 flex items-center gap-1.5 font-mono text-[10px] uppercase text-emerald-400">
                <Terminal className="h-3.5 w-3.5" />
                Start Local Rust Node
              </div>
              <code className="block break-all font-mono text-[11px] text-slate-200">
                newsatoshi-commons serve --port 8791 --state-file ./data/commons-ledger.json
                --seed-url https://newsatoshi-seed.fly.dev/status --static-dir ./web/dist
              </code>
            </div>
            <button
              type="button"
              onClick={() => void refreshState()}
              className="mt-4 inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              Retry Connection
            </button>
          </div>
        ) : activeTab === 'cockpit' ? (
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
      </main>
    </div>
  );
}
