#!/usr/bin/env bash
set -euo pipefail

# One-command immutable deployment of NewsatoshiHtlc.sol (zero owner, zero admin keys).
# Usage:
#   DEPLOYER_PRIVATE_KEY=0x... ./bridges/deploy-evm.sh base          # Base Mainnet (~$0.05 ETH)
#   DEPLOYER_PRIVATE_KEY=0x... ./bridges/deploy-evm.sh base-sepolia  # Base Sepolia Testnet (free faucet)
#   DEPLOYER_PRIVATE_KEY=0x... ./bridges/deploy-evm.sh arbitrum      # Arbitrum One

CHAIN="${1:-base}"
case "$CHAIN" in
  base)         RPC_URL="https://mainnet.base.org" ;;
  base-sepolia) RPC_URL="https://sepolia.base.org" ;;
  arbitrum)     RPC_URL="https://arb1.arbitrum.io/rpc" ;;
  ethereum)     RPC_URL="https://eth.llamarpc.com" ;;
  optimism)     RPC_URL="https://mainnet.optimism.io" ;;
  polygon)      RPC_URL="https://polygon-rpc.com" ;;
  *) echo "Unknown chain '$CHAIN'. Use: base | base-sepolia | arbitrum | ethereum | optimism | polygon" >&2; exit 1 ;;
esac

if [[ -z "${DEPLOYER_PRIVATE_KEY:-}" ]]; then
  echo "ERROR: set DEPLOYER_PRIVATE_KEY (or wrap with: agent-vault env-run --map DEPLOYER_PRIVATE_KEY=... -- ./bridges/deploy-evm.sh $CHAIN)" >&2
  exit 1
fi

export PATH="$HOME/.foundry/bin:$PATH"
cd "$(dirname "$0")/evm"

echo "==> Running Foundry test suite before broadcast..."
forge test -v

echo "==> Deploying immutable NewsatoshiHtlc to $CHAIN ($RPC_URL)..."
forge create NewsatoshiHtlc.sol:NewsatoshiHtlc \
  --rpc-url "$RPC_URL" \
  --private-key "$DEPLOYER_PRIVATE_KEY" \
  --broadcast
