import { useMemo, useState } from 'react';
import { ArrowUpRight, CheckCircle2, Loader2, ShieldCheck, Wallet } from 'lucide-react';
import {
  CANONICAL_BACKING_VAULT,
  EVM_CHAINS,
  GLOBAL_COMMONS_CHARTER_ID,
  ZERO_BYTES32,
  encodeApprove,
  encodeDepositErc20,
  encodeDepositEth,
  parseUnits,
  readErc20Allowance,
  sha256HexUtf8,
  type EvmChainKey,
} from '../wallet/evm';
import { useWallet } from '../wallet/WalletContext';

interface QuickBackCardProps {
  onBacked?: () => void;
  onNotice?: (kind: 'success' | 'error', message: string) => void;
}

export function QuickBackCard({ onBacked, onNotice }: QuickBackCardProps) {
  const {
    evmAddress,
    evmChainId,
    openModal,
    switchEvmChain,
    sendEvmTx,
    waitForEvmReceipt,
  } = useWallet();

  const [chainKey, setChainKey] = useState<EvmChainKey>('base');
  const [asset, setAsset] = useState<'NATIVE' | 'USDC'>('NATIVE');
  const [amountInput, setAmountInput] = useState<string>('0.001');
  const [memo, setMemo] = useState<string>('');
  const [busy, setBusy] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [txConfirmed, setTxConfirmed] = useState<boolean>(false);

  const cfg = EVM_CHAINS[chainKey];
  const symbol = asset === 'NATIVE' ? cfg.nativeSymbol : 'USDC';
  const decimals = asset === 'NATIVE' ? cfg.nativeDecimals : cfg.usdcDecimals;
  const amountRaw = useMemo(() => parseUnits(amountInput, decimals), [amountInput, decimals]);
  const isRightChain = evmChainId === cfg.chainId;

  const presets =
    asset === 'USDC'
      ? ['1', '5', '10']
      : chainKey === 'polygon'
        ? ['5', '10', '25']
        : ['0.001', '0.005', '0.01'];

  const handleBack = async () => {
    if (!evmAddress) {
      openModal();
      return;
    }
    if (!isRightChain) {
      setBusy('Switching network…');
      try {
        await switchEvmChain(chainKey);
      } finally {
        setBusy(null);
      }
      return;
    }
    if (!amountRaw || amountRaw <= 0n) return;

    setTxHash(null);
    setTxConfirmed(false);
    try {
      const memoHash = memo.trim() ? await sha256HexUtf8(memo.trim()) : ZERO_BYTES32;

      if (asset === 'NATIVE') {
        setBusy(`Backing vault with ${amountInput} ${symbol}…`);
        const hash = await sendEvmTx({
          to: CANONICAL_BACKING_VAULT,
          data: encodeDepositEth(GLOBAL_COMMONS_CHARTER_ID, memoHash),
          value: `0x${amountRaw.toString(16)}`,
        });
        setTxHash(hash);
        onNotice?.('success', `Deposit submitted on ${cfg.name}`);
        const st = await waitForEvmReceipt(chainKey, hash);
        if (st === 'success') {
          setTxConfirmed(true);
          onNotice?.('success', `Backed Newsatoshi Vault with ${amountInput} ${symbol} on ${cfg.name}!`);
          onBacked?.();
        }
      } else {
        setBusy('Checking USDC allowance…');
        const currentAllowance = await readErc20Allowance(
          chainKey,
          cfg.usdcAddress,
          evmAddress,
          CANONICAL_BACKING_VAULT
        );
        if (currentAllowance < amountRaw) {
          setBusy('Approving USDC…');
          const approveHash = await sendEvmTx({
            to: cfg.usdcAddress,
            data: encodeApprove(CANONICAL_BACKING_VAULT, amountRaw),
            value: '0x0',
          });
          await waitForEvmReceipt(chainKey, approveHash);
        }
        setBusy(`Depositing ${amountInput} USDC…`);
        const hash = await sendEvmTx({
          to: CANONICAL_BACKING_VAULT,
          data: encodeDepositErc20(cfg.usdcAddress, amountRaw, GLOBAL_COMMONS_CHARTER_ID, memoHash),
          value: '0x0',
        });
        setTxHash(hash);
        onNotice?.('success', `USDC deposit submitted on ${cfg.name}`);
        const st = await waitForEvmReceipt(chainKey, hash);
        if (st === 'success') {
          setTxConfirmed(true);
          onNotice?.('success', `Backed Newsatoshi Vault with ${amountInput} USDC on ${cfg.name}!`);
          onBacked?.();
        }
      }
    } catch (err) {
      onNotice?.('error', err instanceof Error ? err.message : 'Vault deposit cancelled');
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="surface-card rounded-2xl p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <ShieldCheck className="h-4 w-4 text-emerald-400" />
          <h3 className="text-[14px] font-semibold text-zinc-100">
            1-Click Back Newsatoshi Reserve Vault
          </h3>
        </div>
        <span className="rounded-full border border-white/[0.08] bg-white/[0.03] px-2.5 py-0.5 font-mono text-[10px] text-zinc-400">
          0x37Ed…9265 · No Withdraw Function
        </span>
      </div>
      <p className="mt-1.5 text-xs text-zinc-400">
        Permanently add ETH, POL, or USDC to the immutable, ownerless backing reserve on any of the 4 supported mainnets in one wallet signature.
      </p>

      {/* Chain Selector */}
      <div className="mt-4 grid grid-cols-4 gap-1.5">
        {(Object.keys(EVM_CHAINS) as EvmChainKey[]).map((key) => {
          const item = EVM_CHAINS[key];
          const active = chainKey === key;
          return (
            <button
              key={key}
              type="button"
              onClick={() => {
                setChainKey(key);
                if (asset === 'NATIVE') {
                  setAmountInput(key === 'polygon' ? '5' : '0.001');
                }
              }}
              className={`rounded-xl border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                active
                  ? 'border-white/30 bg-white/[0.1] text-white'
                  : 'border-white/[0.08] bg-[#0a0a0d] text-zinc-400 hover:border-white/[0.18] hover:text-zinc-100'
              }`}
            >
              {item.shortName}
            </button>
          );
        })}
      </div>

      {/* Asset + Amount */}
      <div className="mt-3 rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3.5">
        <div className="flex items-center justify-between gap-2">
          <div className="inline-flex rounded-lg border border-white/[0.08] bg-white/[0.02] p-0.5">
            <button
              type="button"
              onClick={() => {
                setAsset('NATIVE');
                setAmountInput(chainKey === 'polygon' ? '5' : '0.001');
              }}
              className={`rounded-md px-2.5 py-1 font-mono text-[11px] font-medium transition cursor-pointer ${
                asset === 'NATIVE' ? 'bg-white/[0.12] text-white' : 'text-zinc-500 hover:text-zinc-200'
              }`}
            >
              {cfg.nativeSymbol}
            </button>
            <button
              type="button"
              onClick={() => {
                setAsset('USDC');
                setAmountInput('1');
              }}
              className={`rounded-md px-2.5 py-1 font-mono text-[11px] font-medium transition cursor-pointer ${
                asset === 'USDC' ? 'bg-white/[0.12] text-white' : 'text-zinc-500 hover:text-zinc-200'
              }`}
            >
              USDC
            </button>
          </div>

          <div className="flex items-center gap-1.5">
            {presets.map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setAmountInput(p)}
                className={`rounded-md border px-2 py-0.5 font-mono text-[10px] transition cursor-pointer ${
                  amountInput === p
                    ? 'border-white/30 bg-white/[0.1] text-zinc-100'
                    : 'border-white/[0.08] text-zinc-400 hover:text-zinc-200'
                }`}
              >
                {p}
              </button>
            ))}
          </div>
        </div>

        <div className="mt-3 flex items-center justify-between gap-3">
          <input
            type="text"
            inputMode="decimal"
            value={amountInput}
            onChange={(e) => setAmountInput(e.target.value)}
            placeholder="0.001"
            className="w-full bg-transparent font-mono text-2xl font-medium text-zinc-100 placeholder:text-zinc-700 focus:outline-none tabular-nums"
          />
          <span className="font-mono text-xs font-semibold text-zinc-300">{symbol}</span>
        </div>
      </div>

      {/* Optional Note */}
      <input
        type="text"
        value={memo}
        onChange={(e) => setMemo(e.target.value)}
        placeholder="Optional public note (hashed on-chain as memoHash)"
        className="mt-2.5 w-full rounded-xl border border-white/[0.08] bg-[#0a0a0d] px-3 py-2 text-xs text-zinc-200 placeholder:text-zinc-600 focus:border-white/25 focus:outline-none"
      />

      <button
        type="button"
        disabled={busy !== null || (Boolean(evmAddress) && isRightChain && (!amountRaw || amountRaw <= 0n))}
        onClick={() => void handleBack()}
        className="mt-3 flex w-full items-center justify-center gap-2 rounded-xl bg-zinc-50 px-4 py-3 text-[13px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer disabled:opacity-45"
      >
        {busy ? (
          <>
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>{busy}</span>
          </>
        ) : !evmAddress ? (
          <>
            <Wallet className="h-4 w-4" />
            <span>Connect Wallet to Back Vault</span>
          </>
        ) : !isRightChain ? (
          <span>Switch Wallet to {cfg.name}</span>
        ) : (
          <span>
            Back Reserve with {amountInput || '0'} {symbol} on {cfg.shortName}
          </span>
        )}
      </button>

      {txHash && (
        <div className="mt-3 flex items-center justify-between rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-3.5 py-2 text-xs text-emerald-200">
          <span className="inline-flex items-center gap-1.5">
            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
            <span>{txConfirmed ? 'Confirmed in Reserve Vault!' : 'Confirming on-chain…'}</span>
          </span>
          <a
            href={`${cfg.explorerUrl}/tx/${txHash}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 font-mono text-[11px] underline"
          >
            <span>View TX</span>
            <ArrowUpRight className="h-3 w-3" />
          </a>
        </div>
      )}
    </div>
  );
}
