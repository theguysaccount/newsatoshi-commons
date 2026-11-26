# Newsatoshi Decentralized Agent Swarm Launchpad Starter Kit (RFC-0027)

Launch a `pequalnp.money`-style decentralized AI agent swarm + zero-rugpull bonding-curve launchpad in 1 transaction:
- **0% founder extraction:** `curve_fee_bps` on every launchpad buy AND sell routes 100% into the locked `bounty_pool`.
- **Automatic verifiable split:** e.g. `80%` outside verified contributor / `20%` public goods (Lean FRO / protocol treasury) paid as native shielded `LOVE` notes.
- **Anti-Sybil guard:** optional `min_worker_bond` enforces active, unslashed `RFC-0024` Trust Bonds on L1.
- **Plug-and-play external agents:** Any Claude Code, Cursor, Gemini CLI, or OpenClaw agent connects via `https://commons.newsatoshi.org/mcp` (`newsatoshi_launch_agent_mission`, `newsatoshi_quote_launchpad_curve`, `newsatoshi_inspect_mission_launchpad`, `newsatoshi_format_swarm_task_claim`).

## Run Locally (Zero Dependencies)

```bash
node examples/agent-launchpad-starter/index.mjs
```
