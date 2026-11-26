import { useState } from 'react';
import {
  ArrowUpRight,
  Check,
  Coins,
  ExternalLink,
  Loader2,
  LogOut,
  Plus,
  RefreshCw,
  Wallet,
  X,
} from 'lucide-react';
import {
  EVM_CHAINS,
  chainById,
  formatCompactToken,
  formatUnits,
  type EvmChainKey,
} from '../wallet/evm';
import { useWallet } from '../wallet/WalletContext';
import { CopyButton, truncateHash } from './ui/CopyButton';

interface WalletModalProps {
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

export function WalletModal({ onNotice }: WalletModalProps) {
  const {
    discoveredWallets,
    evmAddress,
    evmChainId,
    evmWalletName,
    evmConnecting,
    balances,
    connectEvm,
    disconnectEvm,
    switchEvmChain,
    addWloveToWallet,
    refreshBalances,
    solanaAddress,
    solanaWalletName,
    solanaAvailable,
    connectSolana,
    disconnectSolana,
    btcAddress,
    btcWalletName,
    btcAvailable,
    connectBtc,
    disconnectBtc,
    loveAddress,
    setLoveAddress,
    modalOpen,
    closeModal,
    walletError,
  } = useWallet();

  const [loveInput, setLoveInput] = useState(loveAddress);
  const [addingToken, setAddingToken] = useState(false);

  if (!modalOpen) return null;

  const activeChain = chainById(evmChainId);

  const handleConnectEvm = async (id?: string) => {
    const ok = await connectEvm(id);
    if (ok) {
      onNotice('success', 'Wallet connected');
    }
  };

  const handleAddWlove = async () => {
    setAddingToken(true);
    try {
      const ok = await addWloveToWallet();
      if (ok) onNotice('success', 'Added wLOVE token to your wallet on Base');
    } finally {
      setAddingToken(false);
    }
  };

  const handleSaveLove = (e: React.FormEvent) => {
    e.preventDefault();
    setLoveAddress(loveInput.trim());
    onNotice('success', loveInput.trim() ? 'Saved your Newsatoshi L1 address' : 'Cleared L1 address');
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-2xl border border-white/[0.1] bg-[#0f0f12] p-6 shadow-2xl">
        {/* Header */}
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="text-[16px] font-semibold tracking-tight text-zinc-100">
              {evmAddress ? 'Connected Wallets' : 'Connect a Wallet'}
            </h2>
            <p className="mt-1 text-xs text-zinc-400">
              1-click swap on Base, back the 4-chain vault, or auto-fill bridges. Non-custodial — keys stay in your wallet.
            </p>
          </div>
          <button
            type="button"
            onClick={closeModal}
            aria-label="Close wallet dialog"
            className="rounded-lg border border-white/[0.08] p-1.5 text-zinc-400 transition hover:border-white/[0.2] hover:text-zinc-100 cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {walletError && (
          <div className="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-200">
            {walletError}
          </div>
        )}

        {/* Section 1: EVM Wallet (Primary for 1-Click Swap & 4-Chain Backing) */}
        <div className="mt-5 space-y-3">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-zinc-500">
              EVM Wallet · Base · Arbitrum · Optimism · Polygon
            </span>
            {evmAddress && (
              <button
                type="button"
                onClick={() => void refreshBalances()}
                className="inline-flex items-center gap-1 text-[11px] text-zinc-400 hover:text-zinc-100 cursor-pointer"
              >
                <RefreshCw className={`h-3 w-3 ${balances.loading ? 'animate-spin' : ''}`} />
                <span>Refresh</span>
              </button>
            )}
          </div>

          {evmAddress ? (
            <div className="space-y-3 rounded-xl border border-emerald-500/25 bg-[#0a0a0d] p-4">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2.5">
                  <span className="flex h-8 w-8 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-300">
                    <Wallet className="h-4 w-4" />
                  </span>
                  <div>
                    <div className="flex items-center gap-2 text-xs font-medium text-zinc-100">
                      <span>{evmWalletName || 'Connected Wallet'}</span>
                      <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] text-emerald-300">
                        {activeChain ? activeChain.shortName : `Chain #${evmChainId ?? '?'}`}
                      </span>
                    </div>
                    <div className="mt-0.5 flex items-center gap-1.5 font-mono text-[11px] text-zinc-400">
                      <span>{truncateHash(evmAddress, 8, 6)}</span>
                      <CopyButton value={evmAddress} />
                    </div>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={disconnectEvm}
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.1] px-2.5 py-1.5 text-[11px] font-medium text-zinc-400 transition hover:border-red-500/40 hover:text-red-300 cursor-pointer"
                >
                  <LogOut className="h-3 w-3" />
                  <span>Disconnect</span>
                </button>
              </div>

              {/* Live Base Balances */}
              <div className="grid grid-cols-3 gap-2 rounded-lg border border-white/[0.06] bg-white/[0.02] p-2.5 text-center">
                <div>
                  <div className="text-[10px] text-zinc-500">Base ETH</div>
                  <div className="mt-0.5 font-mono text-xs font-medium text-zinc-100 tabular-nums">
                    {formatUnits(balances.baseEthWei, 18, 5)}
                  </div>
                </div>
                <div className="border-x border-white/[0.06]">
                  <div className="text-[10px] text-zinc-500">Base USDC</div>
                  <div className="mt-0.5 font-mono text-xs font-medium text-zinc-100 tabular-nums">
                    {formatUnits(balances.baseUsdcRaw, 6, 2)}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-zinc-500">Base wLOVE</div>
                  <div className="mt-0.5 font-mono text-xs font-medium text-emerald-300 tabular-nums">
                    {formatCompactToken(balances.baseWloveWei, 18)}
                  </div>
                </div>
              </div>

              {/* 1-Click Chain Switcher */}
              <div className="flex flex-wrap items-center gap-1.5">
                {(Object.keys(EVM_CHAINS) as EvmChainKey[]).map((key) => {
                  const cfg = EVM_CHAINS[key];
                  const isCurrent = evmChainId === cfg.chainId;
                  return (
                    <button
                      key={key}
                      type="button"
                      onClick={() => void switchEvmChain(key)}
                      className={`rounded-lg border px-2.5 py-1 text-[11px] font-medium transition cursor-pointer ${
                        isCurrent
                          ? 'border-white/30 bg-white/[0.1] text-white'
                          : 'border-white/[0.08] bg-white/[0.02] text-zinc-400 hover:border-white/[0.18] hover:text-zinc-100'
                      }`}
                    >
                      {cfg.shortName}
                    </button>
                  );
                })}
                <button
                  type="button"
                  onClick={() => void handleAddWlove()}
                  disabled={addingToken}
                  className="ml-auto inline-flex items-center gap-1 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-1 text-[11px] font-medium text-emerald-300 transition hover:bg-emerald-500/20 cursor-pointer disabled:opacity-50"
                >
                  {addingToken ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <Plus className="h-3 w-3" />
                  )}
                  <span>Add wLOVE</span>
                </button>
              </div>
            </div>
          ) : discoveredWallets.length > 0 ? (
            <div className="grid grid-cols-1 gap-2">
              {discoveredWallets.map((w) => (
                <button
                  key={w.id}
                  type="button"
                  disabled={evmConnecting}
                  onClick={() => void handleConnectEvm(w.id)}
                  className="flex items-center justify-between rounded-xl border border-white/[0.09] bg-[#0a0a0d] px-4 py-3 text-left transition hover:border-white/[0.25] hover:bg-white/[0.04] cursor-pointer disabled:opacity-60"
                >
                  <div className="flex items-center gap-3">
                    {w.icon ? (
                      <img
                        src={w.icon}
                        alt={w.name}
                        className="h-7 w-7 rounded-lg object-contain"
                      />
                    ) : (
                      <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-white/[0.08] text-zinc-200">
                        <Wallet className="h-4 w-4" />
                      </span>
                    )}
                    <div>
                      <div className="text-sm font-medium text-zinc-100">{w.name}</div>
                      <div className="text-[11px] text-zinc-500">Detected in browser · 1-click Base swap</div>
                    </div>
                  </div>
                  {evmConnecting ? (
                    <Loader2 className="h-4 w-4 animate-spin text-zinc-400" />
                  ) : (
                    <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-0.5 text-[10px] font-medium text-emerald-300">
                      Connect
                    </span>
                  )}
                </button>
              ))}
            </div>
          ) : (
            <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-4">
              <p className="text-xs leading-relaxed text-zinc-300">
                No EVM wallet extension detected in this browser. Install a free wallet to swap in 1 click, or trade directly on Uniswap mobile:
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <a
                  href="https://www.coinbase.com/wallet/downloads"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.12] bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-zinc-100 hover:bg-white/[0.08]"
                >
                  <span>Coinbase Wallet</span>
                  <ExternalLink className="h-3 w-3 text-zinc-400" />
                </a>
                <a
                  href="https://rabby.io/"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.12] bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-zinc-100 hover:bg-white/[0.08]"
                >
                  <span>Rabby</span>
                  <ExternalLink className="h-3 w-3 text-zinc-400" />
                </a>
                <a
                  href="https://metamask.io/download/"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.12] bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-zinc-100 hover:bg-white/[0.08]"
                >
                  <span>MetaMask</span>
                  <ExternalLink className="h-3 w-3 text-zinc-400" />
                </a>
                <a
                  href="https://phantom.app/"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.12] bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-zinc-100 hover:bg-white/[0.08]"
                >
                  <span>Phantom</span>
                  <ExternalLink className="h-3 w-3 text-zinc-400" />
                </a>
              </div>
            </div>
          )}
        </div>

        {/* Section 2: Native Newsatoshi L1 Wallet (Zero Install Browser Wallet) */}
        <div className="mt-5 space-y-2.5 border-t border-white/[0.07] pt-5">
          <div className="flex items-center justify-between gap-2">
            <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-zinc-500">
              Native Newsatoshi L1 Wallet (LOVE · Shielded)
            </span>
            <a
              href="https://newsatoshi.org/wallet/"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-[11px] font-medium text-emerald-300 hover:text-emerald-200"
            >
              <span>Create in Browser</span>
              <ArrowUpRight className="h-3 w-3" />
            </a>
          </div>
          <form onSubmit={handleSaveLove} className="flex gap-2">
            <input
              type="text"
              value={loveInput}
              onChange={(e) => setLoveInput(e.target.value)}
              placeholder="Optional: paste your ns1… / nsd1… address to auto-fill"
              className="flex-1 rounded-xl border border-white/[0.1] bg-[#0a0a0d] px-3 py-2 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-white/30 focus:outline-none"
            />
            <button
              type="submit"
              className="inline-flex items-center gap-1 rounded-xl border border-white/[0.12] bg-white/[0.06] px-3.5 py-2 text-xs font-medium text-zinc-100 transition hover:bg-white/[0.12] cursor-pointer"
            >
              <Check className="h-3.5 w-3.5" />
              <span>Save</span>
            </button>
          </form>
        </div>

        {/* Section 3: Optional Solana & Bitcoin Bridge Connectors */}
        <div className="mt-5 space-y-2.5 border-t border-white/[0.07] pt-5">
          <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-zinc-500">
            Optional Bridge Connectors (Auto-fills L1 Bridge)
          </span>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {/* Solana */}
            <div className="flex items-center justify-between rounded-xl border border-white/[0.08] bg-[#0a0a0d] px-3.5 py-2.5">
              <div className="min-w-0">
                <div className="text-xs font-medium text-zinc-200">
                  {solanaWalletName || 'Solana'}
                </div>
                <div className="truncate font-mono text-[10px] text-zinc-500">
                  {solanaAddress ? truncateHash(solanaAddress, 5, 4) : 'Phantom / Solflare'}
                </div>
              </div>
              {solanaAddress ? (
                <button
                  type="button"
                  onClick={disconnectSolana}
                  className="text-[11px] text-zinc-400 hover:text-red-300 cursor-pointer"
                >
                  Disconnect
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => void connectSolana()}
                  className="rounded-lg border border-white/[0.12] px-2.5 py-1 text-[11px] font-medium text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                >
                  {solanaAvailable ? 'Connect' : 'Install'}
                </button>
              )}
            </div>

            {/* Bitcoin */}
            <div className="flex items-center justify-between rounded-xl border border-white/[0.08] bg-[#0a0a0d] px-3.5 py-2.5">
              <div className="min-w-0">
                <div className="text-xs font-medium text-zinc-200">
                  {btcWalletName || 'Bitcoin'}
                </div>
                <div className="truncate font-mono text-[10px] text-zinc-500">
                  {btcAddress ? truncateHash(btcAddress, 6, 4) : 'UniSat / Leather'}
                </div>
              </div>
              {btcAddress ? (
                <button
                  type="button"
                  onClick={disconnectBtc}
                  className="text-[11px] text-zinc-400 hover:text-red-300 cursor-pointer"
                >
                  Disconnect
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => void connectBtc()}
                  className="rounded-lg border border-white/[0.12] px-2.5 py-1 text-[11px] font-medium text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                >
                  {btcAvailable ? 'Connect' : 'Install'}
                </button>
              )}
            </div>
          </div>
        </div>

        {/* Bottom quick link */}
        <div className="mt-5 flex items-center justify-between border-t border-white/[0.07] pt-4 text-[11px] text-zinc-500">
          <span className="inline-flex items-center gap-1.5">
            <Coins className="h-3.5 w-3.5 text-zinc-400" />
            <span>Base wLOVE · 0% tax · LP burned</span>
          </span>
          <a
            href="https://app.uniswap.org/swap?chain=base&outputCurrency=0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-zinc-400 hover:text-zinc-100"
          >
            <span>Open in Uniswap</span>
            <ArrowUpRight className="h-3 w-3" />
          </a>
        </div>
      </div>
    </div>
  );
}
