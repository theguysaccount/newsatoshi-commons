"""FastAPI metered endpoint using zero-dependency Newsatoshi O(1) SHA-256 PayWords."""
import os
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from newsatoshi_402 import BasePayWordGate, EVM_PAYWORD_CHAINS

app = FastAPI(title="Newsatoshi PayWords Metered FastAPI Starter")
CHAIN = os.environ.get("PAYWORDS_CHAIN", "base")
RECIPIENT = os.environ.get("PAYWORDS_RECIPIENT", "0x1744535521EB420be9700475400455a4Ad0EBEa2")
chain_cfg = EVM_PAYWORD_CHAINS.get(CHAIN, EVM_PAYWORD_CHAINS["base"])

gate = BasePayWordGate(
    recipient_address=RECIPIENT,
    unit_amount_raw=1000,  # $0.001 USDC
    token_address=chain_cfg["usdc"],
    rpc_url=chain_cfg["rpc_url"],
    channel_contract=chain_cfg["payword_channel"],
    verify_on_chain=True,
)


@app.post("/api/inference")
async def metered_inference(request: Request):
    verdict = gate.verify_headers(dict(request.headers))
    if not verdict["allowed"]:
        return JSONResponse(
            content=verdict["body"],
            status_code=verdict["status"],
            headers=verdict.get("headers", {}),
        )
    payload = await request.json() if await request.body() else {}
    return {
        "ok": True,
        "chain": chain_cfg["name"],
        "channel_id": verdict.get("channel_id"),
        "step": verdict.get("index"),
        "result": f"Processed metered agent request: {payload}",
    }
