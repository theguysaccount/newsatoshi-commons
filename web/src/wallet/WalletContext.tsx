import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  BASE_USDC_ADDRESS,
  BASE_WLOVE_ADDRESS,
  EVM_CHAINS,
  chainById,
  readErc20Balance,
  readNativeBalance,
  rpcCall,
  type EvmChainKey,
} from './evm';

export interface Eip1193Provider {
  request: (args: { method: string; params?: unknown[] | Record<string, unknown> }) => Promise<unknown>;
  on?: (event: string, listener: (...args: unknown[]) => void) => void;
  removeListener?: (event: string, listener: (...args: unknown[]) => void) => void;
  isMetaMask?: boolean;
  isCoinbaseWallet?: boolean;
  isRabby?: boolean;
  isPhantom?: boolean;
  isBraveWallet?: boolean;
}

export interface DiscoveredEvmWallet {
  id: string;
  name: string;
  icon?: string;
  rdns?: string;
  provider: Eip1193Provider;
}

interface Eip6963ProviderDetail {
  info: {
    uuid: string;
    name: string;
    icon: string;
    rdns: string;
  };
  provider: Eip1193Provider;
}

interface SolanaProvider {
  isPhantom?: boolean;
  isSolflare?: boolean;
  isBackpack?: boolean;
  publicKey?: { toBase58?: () => string; toString: () => string } | null;
  connect: (opts?: { onlyIfTrusted?: boolean }) => Promise<{ publicKey: { toBase58?: () => string; toString: () => string } }>;
  disconnect?: () => Promise<void>;
}

interface UnisatProvider {
  requestAccounts: () => Promise<string[]>;
  getPublicKey?: () => Promise<string>;
}

declare global {
  interface Window {
    ethereum?: Eip1193Provider & { providers?: Eip1193Provider[] };
    phantom?: { solana?: SolanaProvider; ethereum?: Eip1193Provider };
    solflare?: SolanaProvider;
    backpack?: SolanaProvider;
    solana?: SolanaProvider;
    unisat?: UnisatProvider;
    LeatherProvider?: {
      request: (method: string) => Promise<{
        result?: { addresses?: { address: string; publicKey?: string; symbol?: string; type?: string }[] };
      }>;
    };
  }
}

export interface EvmBalances {
  baseEthWei: bigint;
  baseUsdcRaw: bigint;
  baseWloveWei: bigint;
  activeNativeWei: bigint;
  activeUsdcRaw: bigint;
  loading: boolean;
}

interface WalletContextValue {
  // EVM
  discoveredWallets: DiscoveredEvmWallet[];
  evmAddress: string | null;
  evmChainId: number | null;
  evmWalletName: string | null;
  evmProvider: Eip1193Provider | null;
  evmConnecting: boolean;
  balances: EvmBalances;
  connectEvm: (walletId?: string) => Promise<boolean>;
  disconnectEvm: () => void;
  switchEvmChain: (chainKey: EvmChainKey) => Promise<boolean>;
  sendEvmTx: (tx: { to: string; data?: string; value?: string }) => Promise<string>;
  waitForEvmReceipt: (chainKey: EvmChainKey, txHash: string) => Promise<'success' | 'reverted' | 'timeout'>;
  addWloveToWallet: () => Promise<boolean>;
  refreshBalances: () => Promise<void>;

  // Solana
  solanaAddress: string | null;
  solanaWalletName: string | null;
  solanaAvailable: boolean;
  connectSolana: () => Promise<boolean>;
  disconnectSolana: () => void;

  // Bitcoin
  btcAddress: string | null;
  btcPubkeyHex: string | null;
  btcWalletName: string | null;
  btcAvailable: boolean;
  connectBtc: () => Promise<boolean>;
  disconnectBtc: () => void;

  // Native Newsatoshi L1 address (remembered locally for zero re-typing)
  loveAddress: string;
  setLoveAddress: (addr: string) => void;

  // Modal & status
  modalOpen: boolean;
  openModal: () => void;
  closeModal: () => void;
  walletError: string | null;
  clearWalletError: () => void;
}

const LOVE_ADDR_STORAGE_KEY = 'newsatoshi_love_address_v1';
const LAST_WALLET_STORAGE_KEY = 'newsatoshi_last_evm_wallet_v1';

const WalletContext = createContext<WalletContextValue | null>(null);

function detectFallbackWallets(): DiscoveredEvmWallet[] {
  if (typeof window === 'undefined') return [];
  const out: DiscoveredEvmWallet[] = [];
  const seen = new Set<Eip1193Provider>();
  const add = (id: string, name: string, provider?: Eip1193Provider) => {
    if (!provider || seen.has(provider)) return;
    seen.add(provider);
    out.push({ id, name, provider });
  };
  const eth = window.ethereum;
  if (eth?.providers && Array.isArray(eth.providers)) {
    for (const p of eth.providers) {
      if (p.isRabby) add('fallback-rabby', 'Rabby Wallet', p);
      else if (p.isCoinbaseWallet) add('fallback-coinbase', 'Coinbase Wallet', p);
      else if (p.isPhantom) add('fallback-phantom-evm', 'Phantom', p);
      else if (p.isBraveWallet) add('fallback-brave', 'Brave Wallet', p);
      else if (p.isMetaMask) add('fallback-metamask', 'MetaMask', p);
      else add(`fallback-eth-${out.length}`, 'Browser Wallet', p);
    }
  } else if (eth) {
    const label = eth.isRabby
      ? 'Rabby Wallet'
      : eth.isCoinbaseWallet
        ? 'Coinbase Wallet'
        : eth.isPhantom
          ? 'Phantom'
          : eth.isBraveWallet
            ? 'Brave Wallet'
            : eth.isMetaMask
              ? 'MetaMask'
              : 'Injected EVM Wallet';
    add('fallback-injected', label, eth);
  }
  if (window.phantom?.ethereum) {
    add('fallback-phantom-ext', 'Phantom', window.phantom.ethereum);
  }
  return out;
}

function getSolanaProvider(): { name: string; provider: SolanaProvider } | null {
  if (typeof window === 'undefined') return null;
  if (window.phantom?.solana) return { name: 'Phantom', provider: window.phantom.solana };
  if (window.solflare) return { name: 'Solflare', provider: window.solflare };
  if (window.backpack) return { name: 'Backpack', provider: window.backpack };
  if (window.solana) return { name: window.solana.isPhantom ? 'Phantom' : 'Solana Wallet', provider: window.solana };
  return null;
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [eip6963Wallets, setEip6963Wallets] = useState<DiscoveredEvmWallet[]>([]);
  const [evmAddress, setEvmAddress] = useState<string | null>(null);
  const [evmChainId, setEvmChainId] = useState<number | null>(null);
  const [evmWalletName, setEvmWalletName] = useState<string | null>(null);
  const [evmProvider, setEvmProvider] = useState<Eip1193Provider | null>(null);
  const [evmConnecting, setEvmConnecting] = useState(false);

  const [solanaAddress, setSolanaAddress] = useState<string | null>(null);
  const [solanaWalletName, setSolanaWalletName] = useState<string | null>(null);

  const [btcAddress, setBtcAddress] = useState<string | null>(null);
  const [btcPubkeyHex, setBtcPubkeyHex] = useState<string | null>(null);
  const [btcWalletName, setBtcWalletName] = useState<string | null>(null);

  const [loveAddress, setLoveAddressState] = useState<string>(() => {
    try {
      return window.localStorage.getItem(LOVE_ADDR_STORAGE_KEY) || '';
    } catch {
      return '';
    }
  });

  const [modalOpen, setModalOpen] = useState(false);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [balances, setBalances] = useState<EvmBalances>({
    baseEthWei: 0n,
    baseUsdcRaw: 0n,
    baseWloveWei: 0n,
    activeNativeWei: 0n,
    activeUsdcRaw: 0n,
    loading: false,
  });

  const autoReconnectTried = useRef(false);

  // EIP-6963 Multi-Wallet Discovery
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onAnnounce = (event: Event) => {
      const detail = (event as CustomEvent<Eip6963ProviderDetail>).detail;
      if (!detail?.provider || !detail?.info?.uuid) return;
      setEip6963Wallets((prev) => {
        if (prev.some((w) => w.id === detail.info.uuid || w.rdns === detail.info.rdns)) {
          return prev;
        }
        return [
          ...prev,
          {
            id: detail.info.uuid,
            name: detail.info.name,
            icon: detail.info.icon,
            rdns: detail.info.rdns,
            provider: detail.provider,
          },
        ];
      });
    };
    window.addEventListener('eip6963:announceProvider', onAnnounce);
    window.dispatchEvent(new Event('eip6963:requestProvider'));
    return () => window.removeEventListener('eip6963:announceProvider', onAnnounce);
  }, []);

  const discoveredWallets = useMemo(() => {
    if (eip6963Wallets.length > 0) return eip6963Wallets;
    return detectFallbackWallets();
  }, [eip6963Wallets]);

  const setLoveAddress = useCallback((addr: string) => {
    setLoveAddressState(addr);
    try {
      if (addr.trim()) window.localStorage.setItem(LOVE_ADDR_STORAGE_KEY, addr.trim());
      else window.localStorage.removeItem(LOVE_ADDR_STORAGE_KEY);
    } catch {
      // ignore
    }
  }, []);

  const refreshBalances = useCallback(async () => {
    if (!evmAddress) {
      setBalances({
        baseEthWei: 0n,
        baseUsdcRaw: 0n,
        baseWloveWei: 0n,
        activeNativeWei: 0n,
        activeUsdcRaw: 0n,
        loading: false,
      });
      return;
    }
    setBalances((prev) => ({ ...prev, loading: true }));
    try {
      const activeCfg = chainById(evmChainId);
      const activeKey: EvmChainKey = activeCfg ? activeCfg.key : 'base';

      const [baseEthWei, baseUsdcRaw, baseWloveWei] = await Promise.all([
        readNativeBalance('base', evmAddress).catch(() => 0n),
        readErc20Balance('base', BASE_USDC_ADDRESS, evmAddress).catch(() => 0n),
        readErc20Balance('base', BASE_WLOVE_ADDRESS, evmAddress).catch(() => 0n),
      ]);

      let activeNativeWei = baseEthWei;
      let activeUsdcRaw = baseUsdcRaw;
      if (activeKey !== 'base') {
        const [nBal, uBal] = await Promise.all([
          readNativeBalance(activeKey, evmAddress).catch(() => 0n),
          readErc20Balance(activeKey, EVM_CHAINS[activeKey].usdcAddress, evmAddress).catch(() => 0n),
        ]);
        activeNativeWei = nBal;
        activeUsdcRaw = uBal;
      }

      setBalances({
        baseEthWei,
        baseUsdcRaw,
        baseWloveWei,
        activeNativeWei,
        activeUsdcRaw,
        loading: false,
      });
    } catch {
      setBalances((prev) => ({ ...prev, loading: false }));
    }
  }, [evmAddress, evmChainId]);

  useEffect(() => {
    void refreshBalances();
  }, [refreshBalances]);

  // Listen to accountsChanged / chainChanged on connected EVM provider
  useEffect(() => {
    if (!evmProvider?.on) return;
    const handleAccounts = (accounts: unknown) => {
      const list = Array.isArray(accounts) ? (accounts as string[]) : [];
      if (list.length === 0) {
        setEvmAddress(null);
        setEvmWalletName(null);
        setEvmProvider(null);
      } else {
        setEvmAddress(list[0]);
      }
    };
    const handleChain = (chainHex: unknown) => {
      if (typeof chainHex === 'string') {
        setEvmChainId(parseInt(chainHex, 16));
      }
    };
    evmProvider.on('accountsChanged', handleAccounts);
    evmProvider.on('chainChanged', handleChain);
    return () => {
      evmProvider.removeListener?.('accountsChanged', handleAccounts);
      evmProvider.removeListener?.('chainChanged', handleChain);
    };
  }, [evmProvider]);

  // Silent reconnect if user previously authorized an account (zero popup)
  useEffect(() => {
    if (autoReconnectTried.current || discoveredWallets.length === 0 || evmAddress) return;
    autoReconnectTried.current = true;
    let preferredId = '';
    try {
      preferredId = window.localStorage.getItem(LAST_WALLET_STORAGE_KEY) || '';
    } catch {
      // ignore
    }
    if (!preferredId) return;
    const match =
      discoveredWallets.find((w) => w.rdns === preferredId || w.name === preferredId || w.id === preferredId) ||
      discoveredWallets[0];
    if (!match) return;
    void (async () => {
      try {
        const accounts = (await match.provider.request({ method: 'eth_accounts' })) as string[];
        if (accounts && accounts.length > 0) {
          const chainHex = (await match.provider.request({ method: 'eth_chainId' })) as string;
          setEvmProvider(match.provider);
          setEvmWalletName(match.name);
          setEvmAddress(accounts[0]);
          setEvmChainId(parseInt(chainHex, 16));
        }
      } catch {
        // silent
      }
    })();
  }, [discoveredWallets, evmAddress]);

  const connectEvm = useCallback(
    async (walletId?: string): Promise<boolean> => {
      setWalletError(null);
      const target = walletId
        ? discoveredWallets.find((w) => w.id === walletId)
        : discoveredWallets[0];
      if (!target) {
        setWalletError('No browser wallet found. Install MetaMask, Coinbase Wallet, Rabby, or Phantom.');
        return false;
      }
      setEvmConnecting(true);
      try {
        const accounts = (await target.provider.request({
          method: 'eth_requestAccounts',
        })) as string[];
        if (!accounts || accounts.length === 0) {
          setWalletError('Wallet returned no accounts');
          return false;
        }
        const chainHex = (await target.provider.request({ method: 'eth_chainId' })) as string;
        setEvmProvider(target.provider);
        setEvmWalletName(target.name);
        setEvmAddress(accounts[0]);
        setEvmChainId(parseInt(chainHex, 16));
        try {
          window.localStorage.setItem(LAST_WALLET_STORAGE_KEY, target.rdns || target.name);
        } catch {
          // ignore
        }
        return true;
      } catch (err) {
        const msg = err instanceof Error ? err.message : 'Connection rejected';
        setWalletError(msg);
        return false;
      } finally {
        setEvmConnecting(false);
      }
    },
    [discoveredWallets]
  );

  const disconnectEvm = useCallback(() => {
    setEvmAddress(null);
    setEvmChainId(null);
    setEvmWalletName(null);
    setEvmProvider(null);
    try {
      window.localStorage.removeItem(LAST_WALLET_STORAGE_KEY);
    } catch {
      // ignore
    }
  }, []);

  const switchEvmChain = useCallback(
    async (chainKey: EvmChainKey): Promise<boolean> => {
      if (!evmProvider) {
        setWalletError('Connect an EVM wallet first');
        return false;
      }
      const cfg = EVM_CHAINS[chainKey];
      setWalletError(null);
      try {
        await evmProvider.request({
          method: 'wallet_switchEthereumChain',
          params: [{ chainId: cfg.chainIdHex }],
        });
        setEvmChainId(cfg.chainId);
        return true;
      } catch (err) {
        const code = (err as { code?: number })?.code;
        if (code === 4902 || /unrecognized chain|not been added/i.test(String(err))) {
          try {
            await evmProvider.request({
              method: 'wallet_addEthereumChain',
              params: [
                {
                  chainId: cfg.chainIdHex,
                  chainName: cfg.name,
                  nativeCurrency: {
                    name: cfg.nativeSymbol,
                    symbol: cfg.nativeSymbol,
                    decimals: cfg.nativeDecimals,
                  },
                  rpcUrls: cfg.rpcUrls,
                  blockExplorerUrls: [cfg.explorerUrl],
                },
              ],
            });
            setEvmChainId(cfg.chainId);
            return true;
          } catch (addErr) {
            setWalletError(addErr instanceof Error ? addErr.message : `Could not add ${cfg.name}`);
            return false;
          }
        }
        setWalletError(err instanceof Error ? err.message : `Could not switch to ${cfg.name}`);
        return false;
      }
    },
    [evmProvider]
  );

  const sendEvmTx = useCallback(
    async (tx: { to: string; data?: string; value?: string }): Promise<string> => {
      if (!evmProvider || !evmAddress) {
        throw new Error('Connect your wallet first');
      }
      const hash = (await evmProvider.request({
        method: 'eth_sendTransaction',
        params: [
          {
            from: evmAddress,
            to: tx.to,
            data: tx.data ?? '0x',
            value: tx.value ?? '0x0',
          },
        ],
      })) as string;
      return hash;
    },
    [evmProvider, evmAddress]
  );

  const waitForEvmReceipt = useCallback(
    async (chainKey: EvmChainKey, txHash: string): Promise<'success' | 'reverted' | 'timeout'> => {
      for (let i = 0; i < 45; i += 1) {
        await new Promise((r) => window.setTimeout(r, 2000));
        try {
          const receipt = (await rpcCall(chainKey, 'eth_getTransactionReceipt', [txHash])) as unknown as {
            status?: string;
          } | null;
          if (receipt && typeof receipt === 'object' && 'status' in receipt) {
            void refreshBalances();
            return receipt.status === '0x1' ? 'success' : 'reverted';
          }
        } catch {
          // keep polling
        }
      }
      void refreshBalances();
      return 'timeout';
    },
    [refreshBalances]
  );

  const addWloveToWallet = useCallback(async (): Promise<boolean> => {
    if (!evmProvider) {
      setWalletError('Connect an EVM wallet first');
      return false;
    }
    try {
      if (evmChainId !== EVM_CHAINS.base.chainId) {
        const switched = await switchEvmChain('base');
        if (!switched) return false;
      }
      await evmProvider.request({
        method: 'wallet_watchAsset',
        params: {
          type: 'ERC20',
          options: {
            address: BASE_WLOVE_ADDRESS,
            symbol: 'wLOVE',
            decimals: 18,
          },
        },
      });
      return true;
    } catch (err) {
      setWalletError(err instanceof Error ? err.message : 'Could not add wLOVE token');
      return false;
    }
  }, [evmProvider, evmChainId, switchEvmChain]);

  // Solana Connector
  const solanaAvailable = Boolean(getSolanaProvider());
  const connectSolana = useCallback(async (): Promise<boolean> => {
    setWalletError(null);
    const found = getSolanaProvider();
    if (!found) {
      setWalletError('No Solana wallet detected. Install Phantom, Solflare, or Backpack.');
      return false;
    }
    try {
      const resp = await found.provider.connect();
      const pk = resp.publicKey.toBase58 ? resp.publicKey.toBase58() : resp.publicKey.toString();
      setSolanaAddress(pk);
      setSolanaWalletName(found.name);
      return true;
    } catch (err) {
      setWalletError(err instanceof Error ? err.message : 'Solana connection rejected');
      return false;
    }
  }, []);

  const disconnectSolana = useCallback(() => {
    const found = getSolanaProvider();
    void found?.provider.disconnect?.();
    setSolanaAddress(null);
    setSolanaWalletName(null);
  }, []);

  // Bitcoin Connector (Unisat / Leather)
  const btcAvailable =
    typeof window !== 'undefined' && Boolean(window.unisat || window.LeatherProvider);

  const connectBtc = useCallback(async (): Promise<boolean> => {
    setWalletError(null);
    if (typeof window === 'undefined') return false;
    if (window.unisat) {
      try {
        const accounts = await window.unisat.requestAccounts();
        const pk = window.unisat.getPublicKey ? await window.unisat.getPublicKey() : null;
        if (accounts[0]) {
          setBtcAddress(accounts[0]);
          setBtcPubkeyHex(pk);
          setBtcWalletName('UniSat');
          return true;
        }
      } catch (err) {
        setWalletError(err instanceof Error ? err.message : 'UniSat connection rejected');
        return false;
      }
    }
    if (window.LeatherProvider) {
      try {
        const resp = await window.LeatherProvider.request('getAddresses');
        const segwit = resp.result?.addresses?.find(
          (a) => a.symbol === 'BTC' && (a.type === 'p2wpkh' || a.address.startsWith('bc1q'))
        ) || resp.result?.addresses?.find((a) => a.symbol === 'BTC');
        if (segwit) {
          setBtcAddress(segwit.address);
          setBtcPubkeyHex(segwit.publicKey ?? null);
          setBtcWalletName('Leather');
          return true;
        }
      } catch (err) {
        setWalletError(err instanceof Error ? err.message : 'Leather connection rejected');
        return false;
      }
    }
    setWalletError('No Bitcoin browser wallet detected. Install UniSat or Leather.');
    return false;
  }, []);

  const disconnectBtc = useCallback(() => {
    setBtcAddress(null);
    setBtcPubkeyHex(null);
    setBtcWalletName(null);
  }, []);

  const value = useMemo<WalletContextValue>(
    () => ({
      discoveredWallets,
      evmAddress,
      evmChainId,
      evmWalletName,
      evmProvider,
      evmConnecting,
      balances,
      connectEvm,
      disconnectEvm,
      switchEvmChain,
      sendEvmTx,
      waitForEvmReceipt,
      addWloveToWallet,
      refreshBalances,
      solanaAddress,
      solanaWalletName,
      solanaAvailable,
      connectSolana,
      disconnectSolana,
      btcAddress,
      btcPubkeyHex,
      btcWalletName,
      btcAvailable,
      connectBtc,
      disconnectBtc,
      loveAddress,
      setLoveAddress,
      modalOpen,
      openModal: () => {
        setWalletError(null);
        setModalOpen(true);
      },
      closeModal: () => setModalOpen(false),
      walletError,
      clearWalletError: () => setWalletError(null),
    }),
    [
      discoveredWallets,
      evmAddress,
      evmChainId,
      evmWalletName,
      evmProvider,
      evmConnecting,
      balances,
      connectEvm,
      disconnectEvm,
      switchEvmChain,
      sendEvmTx,
      waitForEvmReceipt,
      addWloveToWallet,
      refreshBalances,
      solanaAddress,
      solanaWalletName,
      solanaAvailable,
      connectSolana,
      disconnectSolana,
      btcAddress,
      btcPubkeyHex,
      btcWalletName,
      btcAvailable,
      connectBtc,
      disconnectBtc,
      loveAddress,
      setLoveAddress,
      modalOpen,
      walletError,
    ]
  );

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

export function useWallet(): WalletContextValue {
  const ctx = useContext(WalletContext);
  if (!ctx) throw new Error('useWallet must be used inside WalletProvider');
  return ctx;
}
