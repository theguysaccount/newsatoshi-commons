# Parcel #1 Sub-$50,000 Planning & Sizing Worksheet (Illustrative Reference Model)

> **Notice — Planning Worksheet Only (Not Operating Site Data):** No land has been acquired and no hardware is currently deployed. Every dollar, yield, and timeline figure below is an illustrative engineering & financial planning target for scoping a future Parcel #1 deployment; replace with verified county quotes, installer bids, and NREL PVWatts site data before committing capital.

**Protocol Binding:** `NEWSATOSHI/2.1` (`LOVE`) · `reinvest_bps = 1500` (15.00% Covenant Floor) · `min_soil_health_score >= 80`
**Machine-Readable Twin:** [`parcel-1-model.json`](./parcel-1-model.json) *(every figure in this report is exact-`Decimal` verified to the cent against `parcel-1-model.json`)*
**L1 Genesis Anchors:** Mainnet `000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7` (`auxpow_chainid = 20051`) · Testnet `00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394`

---

## 1. Executive Thesis: Out-Competing Extractive Capital on Pure Unit Economics

A Perpetual Purpose Trust + Wyoming DUNA bound to a `NEWSATOSHI/2.1` recursive covenant does not rely on philanthropy, government subsidies, or regulatory capture to beat investor-owned utilities (IOUs), cloud hyperscalers, and speculative real-estate investment trusts (REITs). It wins on **structural cost of capital** and **thermodynamic exergy cascading**:

1. **Zero WACC, Zero Shareholder Dividends, Zero Corporate Lobbying:** An investor-owned operator financing an identical `$49,500` parcel requires **`$23,030.00/yr`** just to break even on hard O&M (`$11,640`), a 12% WACC return hurdle (`$5,940`), corporate/executive/lobbying overhead (`$3,600`), and corporate income tax (`$1,850`). The Commons Perpetual Purpose Trust breaks even at **`$11,640.00/yr`** (**49.46% lower**).
2. **"Bill-Killer" Pricing Beneath Extractive Break-Even:** Charging local communities **`$0.09/kWh`** (50% below the `$0.18/kWh` regional utility rate), **`$0.55/kWh-eq`** for solar-matched edge compute (50% below commercial cloud), and **`$15.00/sq ft/yr`** for greenhouse CSA produce (34.78% below retail organic) generates **`$22,144.40/yr`**—still **`$885.60/yr` BELOW the extractive REIT's break-even**, delivering **`$16,544.40/yr`** in immediate local household savings while STILL compounding **`$10,504.40/yr`** into acquiring adjacent debt-free acreage.
3. **Autonomous Physical Compounding (`Parcel #1 → Parcel #N`):** With **zero debt** and **zero external capital** after the initial `$49,500.00` Parcel #1 deployment, organic compounding autonomously acquires **Parcel #2 in Year 5** (Month 52.32 exact; commissioned start of Year 6) and **Parcel #3 in Year 7**—tripling physical land and solar capacity to **18 acres / 75 kW** while returning **`$148,899.60`** in cumulative direct bill savings to local residents (**3.01× the original seed endowment**).

---

## 2. Sub-$50,000 Parcel #1 Capital Allocation (`$49,500.00` Exact)

Total initial Parcel #1 deployment fits cleanly beneath the `$50,000` threshold (`$500.00` reserve headroom), while subsequent **Parcel #N** expansions require only **`$45,800.00`** because the Perpetual Purpose Trust and Wyoming DUNA entities already exist (saving `$3,700.00` per parcel in one-time legal formation costs).

| Category | Itemized Bill of Materials (BOM) | Subtotal (USD) | Share (%) |
| :--- | :--- | ---: | ---: |
| **1. Raw Rural High-Irradiance Land & Recording** | • 6.0 acres rural high-irradiance land @ `$2,250/acre` (Cochise County, AZ benchmark): `$13,500.00`<br>• Title search, boundary verification, escrow & county deed recording: `$1,500.00` | **`$15,000.00`** | `30.30%` |
| **2. Off-Grid 25 kW Solar + 50 kWh LiFePO4 + Smart Inverter/Meter** | • 25 kW bifacial N-type monocrystalline PV array (`$0.28/W` wholesale): `$7,000.00`<br>• Ground-mount galvanized racking, concrete piers, DC cabling & combiners: `$3,300.00`<br>• 50 kWh modular 48V LiFePO4 battery bank (`$130/kWh` installed pack): `$6,500.00`<br>• 25–30 kW hybrid smart inverter/charger + revenue-grade Modbus/RS-485 meter: `$3,500.00`<br>• Electrical BOS, surge arrestors, disconnects, grounding & commissioning: `$1,200.00` | **`$21,500.00`** | `43.43%` |
| **3. Greenhouse + Waste-Heat Edge Compute + Hardware TEE Kit** | • 20'×40' (`800 sq ft`) Gothic-arch high-tunnel greenhouse, thermal mass & drip irrigation: `$3,800.00`<br>• Insulated waste-heat compute micro-shed + 4-node solar-matched edge AI cluster: `$3,200.00`<br>• Hardware TEE signer (RPi 5 + TPM 2.0 / ESP32-S3), CT meters, backhaul & 8-probe soil kit: `$1,500.00` | **`$8,500.00`** | `17.17%` |
| **4. Perpetual Purpose Trust + Wyoming DUNA Airlock** | • Wyoming DUNA formation, Year-1 registered agent, IRS EIN & operating bank bridge: `$650.00`<br>• Perpetual Purpose Trust counsel review, Trust Enforcer & Standby Fiduciary onboarding: `$2,750.00`<br>• 99-yr CLT ground lease, anti-enclosure deed restriction recording & Day-0 soil lab assay: `$1,100.00` | **`$4,500.00`** | `9.09%` |
| **TOTAL PARCEL #1 CAPEX** | **Full Turnkey Autonomous Commons Node (Land + Power + Food/Compute + Legal Airlock)** | **`$49,500.00`** | **`100.00%`** |
| **MARGINAL PARCEL #N CAPEX** | Identical Land (`$15,000`) + Solar/Battery (`$21,500`) + Greenhouse/Compute/TEE (`$8,500`) + Supplemental Deed Recording & Day-0 Soil Assay (`$800`) | **`$45,800.00`** | *(saves `$3,700`)* |

### Candidate Rural High-Irradiance Site Matrix

| Candidate County | Typical Land Price / Acre | Acres for `$13,500` Land Budget | Actual Avg Peak Sun Hours (PSH/day) | Modeled Conservative PSH/day | Zoning & Thermodynamic Fit |
| :--- | :--- | :---: | :---: | :---: | :--- |
| **Cochise County, AZ *(Benchmark)*** | `$2,000–$2,500` | `6.0 ac` | `5.9 hrs` | `5.5 hrs` | RU-4 rural zoning with owner-builder opt-out pathway; exceptional irradiance and rainwater-harvesting terrain |
| **Hudspeth County, TX** | `$1,500–$2,200` | `7.5 ac` | `5.8 hrs` | `5.5 hrs` | Unincorporated rural county with minimal land-use restriction and low tax basis; requires catchment/well storage |
| **Costilla County, CO** | `$1,800–$2,700` | `5.5 ac` | `5.6 hrs` | `5.5 hrs` | High-altitude San Luis Valley irradiance; cold desert nights maximize winter greenhouse heating value from edge-compute waste heat |

---

## 3. Thermodynamic Yield & Exergy-Cascade Energy Budget

### 3.1 Annual Solar Generation (Zero Optimism Bias)
$$\text{Annual Yield} = 25\text{ kW} \times 5.5\text{ PSH/day} \times 365\text{ days/yr} \times 0.82\text{ derate} = \mathbf{41,153.75\text{ kWh/yr}} \approx \mathbf{41,154\text{ kWh/yr}}$$

The `0.82` derate factor conservatively accounts for inverter conversion efficiency, LiFePO4 round-trip storage loss, summer cell temperature derating, wiring losses, and desert dust soiling—leaving bifacial ground-albedo gain (+5–10%) as an unmodeled upside buffer.

### 3.2 Complete 100% Energy Budget (Zero Double-Counting)
Every single kilowatt-hour of the `41,153.75 kWh/yr` solar yield is allocated exclusively to one electrical end-use, while **thermodynamic waste heat** rejected by the compute cluster is captured a second time as low-grade thermal energy (`kWh_th`) to heat the greenhouse through cold desert nights:

| Allocation Stream | Share of Electrical Yield | Electrical Energy (`kWh/yr`) | Thermal Cascade (`kWh_th/yr`) | Bill-Killer Pricing | Extractive Benchmark | Annual Gross Revenue (USD) | Direct Local Savings (USD) |
| :--- | :---: | ---: | ---: | :--- | :--- | ---: | ---: |
| **Stream A: Local Bill-Killer Electricity** *(Microgrid / EV / Neighbor Feeders)* | `60.0%` | `24,692.25` | `—` | `$0.0900 / kWh` | `$0.1800 / kWh` (IOU) | `$2,222.30` | `$2,222.30` |
| **Stream B: Solar-Matched Edge Compute** *(Opportunistic Midday AI Inference)* | `35.0%` | `14,403.81` | `→ rejects ~14,404 kWh_th` | `$0.5500 / kWh-eq` | `$1.1000 / kWh-eq` (Cloud) | `$7,922.10` | `$7,922.10` |
| **Stream C: Waste-Heat Greenhouse Produce** *(800 sq ft High-Tunnel CSA)* | `5.0%` *(pumps/fans)* | `2,057.69` | `+12,963.43 kWh_th` *(90% capture)* | `$15.00 / sq ft/yr` | `$23.00 / sq ft/yr` (Retail) | `$12,000.00` | `$6,400.00` |
| **TOTAL PARCEL #1 METERED OUTPUT** | **`100.0%`** | **`41,153.75`** | **`12,963.43 kWh_th`** | **35%–50% Below Market** | **IOU / Cloud / Grocery** | **`$22,144.40`** | **`$16,544.40`** |

> **Why Exergy Cascading Matters (Electricity-Only Transparency Check):**
> If Parcel #1 deployed solar panels *alone* and sold 100% of `41,153.75 kWh/yr` strictly as raw electricity at `$0.09/kWh`, annual gross revenue would be **`$3,703.84/yr`** (delivering `$3,703.84/yr` in household bill savings, with a 15% protocol covenant floor of **`$555.58/yr`**). Pairing excess midday electrons (which otherwise face battery saturation or curtailment) with a `$3,200` edge-compute cluster and routing `12,963 kWh_th/yr` of free rejected waste heat into an `$3,800` greenhouse multiplies gross parcel output by **5.98× (`$22,144.40/yr`)** for only `$8,500` of incremental hardware capex—compressing full-parcel compounding time from decades down to **4.36 years**.

---

## 4. Annual Parcel #1 P&L & Exact `reinvest_bps = 1500` Split

Under `NEWSATOSHI/2.1`, every PayWord machine streaming settlement automatically splits gross metered revenue with zero human discretion:

| Cashflow Line Item | Parcel #1 (USD / yr) | Parcel #2+ Marginal (USD / yr) | Mechanism / Legal Enforcement |
| :--- | ---: | ---: | :--- |
| **Gross Metered PayWord Revenue** | **`$22,144.40`** | **`$22,144.40`** | Stream A (`$2,222.30`) + Stream B (`$7,922.10`) + Stream C (`$12,000.00`) |
| ↳ **Protocol Land Covenant Sweep (`reinvest_bps = 1500`, `15.00%`)** | **`$3,321.66`** | **`$3,321.66`** | Cryptographically locked into `CovenantRule` UTXO (`OP_VERIFY_COVENANT_STARK`) |
| ↳ **Operator / Marginal O&M Channel (`8500` bps, `85.00%`)** | **`$18,822.74`** | **`$18,822.74`** | Routed to Wyoming DUNA Operating Channel |
| **Less Hard O&M, Reserves & Resident Steward Stipend:** | | | |
| • Rural agricultural county property tax | `-$300.00` | `-$300.00` | County assessor pass-through (CLT Ground Lease Sec. 3.3) |
| • Wyoming DUNA & Trust annual filing, registered agent & insurance | `-$750.00` | `-$200.00` | Entity admin paid once on Parcel #1; `$200/yr` marginal parcel insurance |
| • Solar/inverter maintenance & 10-yr LiFePO4 battery sinking fund | `-$1,150.00` | `-$1,150.00` | Funds full `$6,500` battery + inverter refresh on schedule without debt |
| • Edge-compute 5-yr hardware refresh reserve (`$3,200 / 5 yrs`) | `-$640.00` | `-$640.00` | Keeps edge AI efficiency competitive indefinitely |
| • Greenhouse seeds, compost, poly-film reserve & annual soil lab assay | `-$1,600.00` | `-$1,600.00` | Maintains verified `soil_health_score >= 80` (`min_soil_health_score`) |
| • Local CLT Resident Steward living-wage stipend | `-$7,200.00` | `-$7,200.00` | Part-time (~12–15 hrs/wk) + `$1/yr` ground lease & marginal utilities |
| **Total Operating Costs & Replacement Reserves** | **`-$11,640.00`** | **`-$11,090.00`** | **100% paid from `85%` channel with full hardware depreciation funded** |
| **Wyoming DUNA Non-Dividend Net Operating Surplus** | **`$7,182.74`** | **`$7,732.74`** | Statutory prohibition on private dividends (`W.S. § 17-32-109`) → swept to Land Covenant (or member price rebates) |
| **TOTAL DEBT-FREE ANNUAL EXPANSION INFLOW** | **`$10,504.40`** | **`$11,054.40`** | **`$3,321.66` immutable floor + DUNA surplus (`21.22%` / `24.14%` unlevered physical yield!)** |

- **Exact Time from Parcel #1 to Parcel #2 (`$45,800.00 / $10,504.40`):** **`4.36 years` (`52.32 months`)** with zero debt and zero external funding.

---

## 5. Side-by-Side Cost-of-Capital Comparison: Commons Trust vs. Extractive IOU / REIT

### 5.1 Pure Solar Electricity LCOE (`25 kW + 50 kWh`, `$25,000` Allocated CapEx, `41,153.75 kWh/yr`, 20-Year Life)

| Cost Component | Newsatoshi Commons Trust (`0%` WACC) | Extractive Investor-Owned Utility (`12%` WACC) | Structural Difference |
| :--- | ---: | ---: | :--- |
| Annualized Capital Charge (20-yr life) | `$1,250.00` *(0% straight-line replacement sinking fund)* | `$3,346.97` *(12% WACC CRF `0.1339`)* | **`-$2,096.97 / yr`** |
| Hard Solar/Battery O&M & Sinking Reserve | `$1,150.00` | `$1,150.00` | Identical physical hardware |
| Grid T&D, Executive Compensation, Billing & Lobbying | `$0.00` *(local off-grid + PayWord automated)* | `$2,600.00` | **`-$2,600.00 / yr`** |
| Federal / State Corporate Income Tax Gross-Up | `$0.00` *(non-dividend purpose trust / DUNA)* | `$320.00` | **`-$320.00 / yr`** |
| **Total Annual Solar Power Cost** | **`$2,400.00 / yr`** | **`$7,416.97 / yr`** | **`-67.64%` Total Cost** |
| **Delivered Cost per kWh (Amortized LCOE)** | **`$0.0583 / kWh`** | **`$0.1802 / kWh`** | **Commons LCOE is 67.64% lower** |
| **Pure Cash O&M Floor (Post-Amortization)** | **`$0.0279 / kWh`** | `~$0.14+ / kWh` *(perpetual rate-base rent)* | Drives local cost toward thermodynamic floor |
| **Retail Tariff Charged to Community** | **`$0.0900 / kWh`** *(Bill-Killer)* | **`$0.1800 / kWh`** *(Regional IOU)* | **50.0% household bill cut + `$0.0317/kWh` Commons surplus!** |

### 5.2 Full Integrated `$49,500` Parcel: Annual Break-Even Revenue Comparison

| Annual Financial Requirement (Identical `$49,500` Physical Asset) | Newsatoshi Commons Trust | Extractive REIT / Investor-Owned Operator | Commons Advantage |
| :--- | ---: | ---: | ---: |
| Hard O&M, Hardware Refresh Reserves & Steward Labor | `$11,640.00` | `$11,640.00` | `$0.00` |
| WACC / Debt Service & Shareholder Dividend Hurdle (`12.0%` on `$49,500`) | `$0.00` | `$5,940.00` | **`-$5,940.00`** |
| Corporate Executive, Legal/Lobbying & REIT Management Overhead | `$0.00` | `$3,600.00` | **`-$3,600.00`** |
| Corporate Income Tax on Private Equity Return | `$0.00` | `$1,850.00` | **`-$1,850.00`** |
| **MINIMUM BREAK-EVEN REVENUE REQUIRED** | **`$11,640.00 / yr`** | **`$23,030.00 / yr`** | **`-$11,390.00 / yr (-49.46%)`** |
| **Commons Actual Bill-Killer Revenue** | **`$22,144.40 / yr`** | *(Extractive REIT loses `-$885.60/yr` at Commons prices!)* | **Underprices extractive break-even by `$885.60/yr`** |
| **Debt-Free Surplus Compounded into Perpetual Land Commons** | **`+$10,504.40 / yr`** | **`$0.00`** *(all surplus extracted as private dividends)* | **Self-compounding physical abundance loop** |

> **The Kill Shot:** An extractive REIT or investor-owned utility literally **cannot match** the Commons' Bill-Killer prices (`$22,144.40/yr`) without operating below its `$23,030.00/yr` investor hurdle and suffering capital flight. Meanwhile, at those exact same below-market prices, the Commons funds 100% of its hardware depreciation reserves, pays a local steward stipend, saves local households `$16,544.40/yr`, and compounds `$10,504.40/yr` into buying the neighboring parcel debt-free.

---

## 6. Verified Year 1–7 Autonomous Compounding Schedule (`Parcel #1 → Parcel #N`)

**Conservative Commissioning Convention:** Whenever the Land Covenant Treasury crosses the **`$45,800.00`** marginal expansion threshold during Year $t$, the Trust autonomously acquires and builds the next parcel during Year $t$, and the new parcel conservatively enters full metered revenue service at the **start of Year $t+1$** (providing a built-in construction and commissioning buffer). Zero debt, zero external capital after Day-0 `$49,500.00` seed.

### 6.1 Primary Model — Single Parcel #1 Seed (`$49,500.00`), Full Commons Reinvestment (Years 1–7)

| Year | Active Parcels | Verified Solar (`kWh`) | Gross Revenue (`$`) | 15% Covenant Floor (`$`) | 85% Operator Channel (`$`) | O&M + Reserves + Steward (`$`) | DUNA Net Surplus (`$`) | Total Treasury Inflow (`$`) | Parcels Acquired (`$45.8k`) | End Parcels (Acres / kW) | Ending Treasury (`$`) | Annual Local Savings (`$`) |
| :---: | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: | ---: | ---: |
| **Yr 1** | `1` | `41,153.75` | `$22,144.40` | `$3,321.66` | `$18,822.74` | `$11,640.00` | `$7,182.74` | `$10,504.40` | `0` | `1` (`6 ac / 25 kW`) | `$10,504.40` | `$16,544.40` |
| **Yr 2** | `1` | `41,153.75` | `$22,144.40` | `$3,321.66` | `$18,822.74` | `$11,640.00` | `$7,182.74` | `$10,504.40` | `0` | `1` (`6 ac / 25 kW`) | `$21,008.80` | `$16,544.40` |
| **Yr 3** | `1` | `41,153.75` | `$22,144.40` | `$3,321.66` | `$18,822.74` | `$11,640.00` | `$7,182.74` | `$10,504.40` | `0` | `1` (`6 ac / 25 kW`) | `$31,513.20` | `$16,544.40` |
| **Yr 4** | `1` | `41,153.75` | `$22,144.40` | `$3,321.66` | `$18,822.74` | `$11,640.00` | `$7,182.74` | `$10,504.40` | `0` | `1` (`6 ac / 25 kW`) | `$42,017.60` | `$16,544.40` |
| **Yr 5** | `1` | `41,153.75` | `$22,144.40` | `$3,321.66` | `$18,822.74` | `$11,640.00` | `$7,182.74` | `$10,504.40` | **`+1`** | **`2` (`12 ac / 50 kW`)** | `$6,722.00` | `$16,544.40` |
| **Yr 6** | `2` | `82,307.50` | `$44,288.80` | `$6,643.32` | `$37,645.48` | `$22,730.00` | `$14,915.48` | `$21,558.80` | `0` | `2` (`12 ac / 50 kW`) | `$28,280.80` | `$33,088.80` |
| **Yr 7** | `2` | `82,307.50` | `$44,288.80` | `$6,643.32` | `$37,645.48` | `$22,730.00` | `$14,915.48` | `$21,558.80` | **`+1`** | **`3` (`18 ac / 75 kW`)** | `$4,039.60` | `$33,088.80` |
| **7-Yr Total** | **`1 → 3`** | **`370,383.75`** | **`$199,299.60`** | **`$29,894.94`** | **`$169,404.66`** | **`$103,660.00`** | **`$65,744.66`** | **`$95,639.60`** | **`+2` (`$91,600`)** | **`3` (`18 ac / 75 kW`)** | **`$4,039.60`** | **`$148,899.60`** |

- **Conservation Check:** `$29,894.94 (15% covenant) + $65,744.66 (DUNA surplus) = $95,639.60` total inflow; `$95,639.60 - $91,600.00 (2 new parcels @ $45,800) = $4,039.60` exact ending treasury.
- **Year-7 Real Asset + Reserve Position:** `$49,500.00 (Parcel #1) + $91,600.00 (Parcels #2 & #3) + $4,039.60 (liquid treasury) = $145,139.60` (**`2.93×`** seed capital) **PLUS** **`$148,899.60`** (`3.01×` seed) already delivered in direct household bill savings!

### 6.2 Exponential Inflection Continuation (Years 8–10)
Once 3 parcels enter service in Year 8, annual expansion inflow jumps to `$32,613.20/yr` (buying a new parcel every **1.40 years**), then `$43,667.60/yr` in Year 10 (**~1.05 years/parcel**):

| Year | Active Parcels | Verified Solar (`kWh`) | Gross Revenue (`$`) | Total Treasury Inflow (`$`) | Parcels Acquired | End Parcels (Acres / kW) | Ending Treasury (`$`) | Annual Local Savings (`$`) |
| :---: | :---: | ---: | ---: | ---: | :---: | :---: | ---: | ---: |
| **Yr 8** | `3` | `123,461.25` | `$66,433.20` | `$32,613.20` | `0` | `3` (`18 ac / 75 kW`) | `$36,652.80` | `$49,633.20` |
| **Yr 9** | `3` | `123,461.25` | `$66,433.20` | `$32,613.20` | **`+1`** | **`4` (`24 ac / 100 kW`)** | `$23,466.00` | `$49,633.20` |
| **Yr 10** | `4` | `164,615.00` | `$88,577.60` | `$43,667.60` | **`+1`** | **`5` (`30 ac / 125 kW`)** | `$21,333.60` | `$66,177.60` |

*(By end of Year 10: **5 full-stack parcels**, **30 acres**, **125 kW solar**, **250 kWh LiFePO4**, **4,000 sq ft greenhouse**, **`$254,033.60`** locked in perpetual commons assets + cash, and **`$314,343.60`** in cumulative local household savings!)*

---

## 7. Boundary & Sensitivity Scenarios

### 7.1 Sensitivity A — Maximum Immediate Price-Cut Mode (Pure `15%` Protocol Floor Only, Years 1–7)
If Wyoming DUNA members vote to return **100%** of the `$7,182.74/yr` DUNA operating surplus directly to local subscribers as immediate deeper tariff rebates (driving effective community electricity all the way toward `$0.0583/kWh` marginal LCOE), ONLY the immutable `reinvest_bps = 1500` protocol sweep (`$3,321.66/yr`) accumulates in the Land Covenant Treasury:
- **7-Year Land Covenant Treasury Accumulation:** **`$23,251.62`** (sufficient to acquire an adjacent **6-acre raw land bank parcel at `$15,000` in Year 5 — exact `4.52 years`** — with `$8,251.62` remaining toward its solar microgrid!).
- **7-Year Cumulative Local Community Savings:** Increases to **`$166,089.98`** (`3.36×` the `$49,500` seed!).

### 7.2 Sensitivity B — 3-Parcel Community Seed Cluster (`$141,100.00` Seed, Zero Debt, Years 1–7)
When a local community launches with **3 starter parcels** (`$49,500 + 2 × $45,800 = $141,100.00`), compounding crosses the 1-parcel-per-year inflection immediately in Year 2:

| Year | Active Parcels | Gross Revenue (`$`) | Total Treasury Inflow (`$`) | Parcels Acquired | End Parcels (Acres / kW) | Ending Treasury (`$`) | Cumulative Local Savings (`$`) |
| :---: | :---: | ---: | ---: | :---: | :---: | ---: | ---: |
| **Yr 1** | `3` | `$66,433.20` | `$32,613.20` | `0` | `3` (`18 ac / 75 kW`) | `$32,613.20` | `$49,633.20` |
| **Yr 2** | `3` | `$66,433.20` | `$32,613.20` | `+1` | `4` (`24 ac / 100 kW`) | `$19,426.40` | `$99,266.40` |
| **Yr 3** | `4` | `$88,577.60` | `$43,667.60` | `+1` | `5` (`30 ac / 125 kW`) | `$17,294.00` | `$165,444.00` |
| **Yr 4** | `5` | `$110,722.00` | `$54,722.00` | `+1` | `6` (`36 ac / 150 kW`) | `$26,216.00` | `$248,166.00` |
| **Yr 5** | `6` | `$132,866.40` | `$65,776.40` | `+2` | `8` (`48 ac / 200 kW`) | `$392.40` | `$347,432.40` |
| **Yr 6** | `8` | `$177,155.20` | `$87,885.20` | `+1` | `9` (`54 ac / 225 kW`) | `$42,477.60` | `$479,787.60` |
| **Yr 7** | `9` | `$199,299.60` | `$98,939.60` | `+3` | **`12` (`72 ac / 300 kW`)** | `$4,017.20` | **`$628,687.20`** |

*(End of Year 7 on a 3-Parcel Cluster: **12 full-stack parcels**, **72 non-enclosable acres**, **300 kW solar**, **600 kWh LiFePO4**, **`$557,317.20`** in physical assets + cash (`3.95×` seed), and **`$628,687.20`** (`4.46×` seed) delivered in local community savings!)*

---

## 8. Key Risk Factors & Protocol Mitigations

| Risk Vector | Quantified Sensitivity | Structural / Cryptographic Mitigation |
| :--- | :--- | :--- |
| **Battery / Inverter Degradation** | 10-year LiFePO4 cycle life (`~3,500–6,000` cycles) | `$1,150/yr` sinking fund (`$11,500` over 10 yrs) fully pre-funds battery (`$6,500`) + inverter (`$3,500`) replacement with `$1,500` spare. |
| **Edge Compute Pricing Compression** | Compute price drops 30% (`$0.55 → $0.385/kWh-eq`) | Gross revenue drops `$2,376.63/yr` to `$19,767.77/yr`; Parcel #1 still nets `$8,127.77/yr` surplus (Parcel #2 shifts from Yr 5 to Yr 6; zero bankruptcy risk due to 0% debt). |
| **Greenhouse Crop Loss / Weather Shock** | 25% temporary produce drop (`-$3,000/yr`) | `$7,504.40/yr` positive expansion surplus remains; zero debt service means a bad agricultural season merely slows expansion rather than triggering foreclosure. |
| **Soil Degradation / Ecological Extraction** | `soil_health_score < 80` | Hardware TEE + annual accredited soil lab assay freezes deployment bounties automatically at the protocol layer (Trust Charter Art. V). |
| **Hardware TEE / Inverter Failure** | Key lockout risk | Zero-discretion Track A / Track B Hardware Replacement Ceremony (Trust Charter Sec. 6.2) with 14/30-day public L1 challenge windows and live heartbeat veto. |
