# Turnkey Paid AI API / GPU Seller Proxy (Native `LOVE` PayWords + `x402 v2`)

Monetize any local GPU (`Ollama`, `vLLM`, `llama.cpp`) or upstream OpenAI-compatible API in native shielded `LOVE` with zero Stripe/KYC middlemen and `< 1 ms` per-call O(1) SHA-256 PayWord verification.

## 1-Command Launch

```bash
export UPSTREAM_AI_URL="http://127.0.0.1:11434"
export NS_SELLER_ADDRESS="nsd1<YOUR_SHIELDED_DELIVERY_ADDRESS>"
export NS_SELLER_BOND_ID="<YOUR_64HEX_RFC0024_TRUST_BOND_ID>"
export NS_PRICE_BASE_UNITS="10000"   # 0.0001 LOVE per call
export NS_REQUIRE_FUNDING="1"        # Enforce live RFC-0022 L1 channel funding
export NS_SELLER_ADMIN_TOKEN="$(openssl rand -hex 24)"

node examples/paid-ai-seller/server.mjs
```

## Run Automated E2E Test

```bash
node --test examples/paid-ai-seller/test_seller.mjs
```
