"""Autonomous Python AI Agent (LangChain / CrewAI / OpenAI / Anthropic) with 1-line PayWords Auto-Pay."""
import os
from newsatoshi_402 import AutoPaySession, generate_payword_chain

URL = os.environ.get("PAID_API_URL", "https://commons.newsatoshi.org/api/v2/paywords/try-402")
CHAIN = os.environ.get("PAYWORDS_CHAIN", "base")
CHANNEL_ID = os.environ.get("PAYWORDS_CHANNEL_ID", "0x" + "00" * 32)


def main():
    wallet = generate_payword_chain(max_ticks=1000, unit_price_base_units=1000)
    session = AutoPaySession(wallet=wallet, chain=CHAIN, channel_id=CHANNEL_ID)
    for idx in range(1, 4):
        status, body = session.request(URL, method="POST")
        print(f"Call #{idx} -> HTTP {status}: {body}")


if __name__ == "__main__":
    main()
