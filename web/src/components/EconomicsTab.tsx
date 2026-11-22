import { useMemo, useState } from 'react';
import { Calculator, Eraser, Info } from 'lucide-react';
import { Panel } from './ui/Panel';

function parsePos(val: string): number | null {
  if (val.trim() === '') return null;
  const n = Number(val);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function parseNonNeg(val: string): number | null {
  if (val.trim() === '') return null;
  const n = Number(val);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

export function EconomicsTab() {
  // Strictly blank user-entered inputs — zero pre-baked synthetic quotes or made-up projections
  const [landPricePerAcre, setLandPricePerAcre] = useState<string>('');
  const [acres, setAcres] = useState<string>('');
  const [solarKw, setSolarKw] = useState<string>('');
  const [peakSunHours, setPeakSunHours] = useState<string>('');
  const [deratePct, setDeratePct] = useState<string>('');
  const [equipmentAndLegalCapex, setEquipmentAndLegalCapex] = useState<string>('');
  const [localUtilityRate, setLocalUtilityRate] = useState<string>('');
  const [commonsRate, setCommonsRate] = useState<string>('');
  const [reinvestBps, setReinvestBps] = useState<string>('');

  const clearAll = () => {
    setLandPricePerAcre('');
    setAcres('');
    setSolarKw('');
    setPeakSunHours('');
    setDeratePct('');
    setEquipmentAndLegalCapex('');
    setLocalUtilityRate('');
    setCommonsRate('');
    setReinvestBps('');
  };

  const calc = useMemo(() => {
    const landPerAcre = parseNonNeg(landPricePerAcre);
    const ac = parsePos(acres);
    const kw = parsePos(solarKw);
    const psh = parsePos(peakSunHours);
    const derate = parsePos(deratePct);
    const equip = parseNonNeg(equipmentAndLegalCapex);
    const gridRate = parsePos(localUtilityRate);
    const cRate = parsePos(commonsRate);
    const bps = parseNonNeg(reinvestBps);

    const landTotal = landPerAcre !== null && ac !== null ? landPerAcre * ac : null;
    const totalCapex = landTotal !== null && equip !== null ? landTotal + equip : null;
    const annualKwh =
      kw !== null && psh !== null && derate !== null ? kw * psh * 365 * (derate / 100) : null;
    const annualGross = annualKwh !== null && cRate !== null ? annualKwh * cRate : null;
    const annualSavings =
      annualKwh !== null && gridRate !== null && cRate !== null
        ? annualKwh * (gridRate - cRate)
        : null;
    const annualCovenantSurplus =
      annualGross !== null && bps !== null ? (annualGross * bps) / 10000 : null;
    const annualOperatorShare =
      annualGross !== null && annualCovenantSurplus !== null
        ? annualGross - annualCovenantSurplus
        : null;
    const monthsToNextLandPurchase =
      landTotal !== null &&
      landTotal > 0 &&
      annualCovenantSurplus !== null &&
      annualCovenantSurplus > 0
        ? (landTotal / annualCovenantSurplus) * 12
        : null;

    const hasAnyOutput =
      totalCapex !== null ||
      annualKwh !== null ||
      annualGross !== null ||
      annualCovenantSurplus !== null;

    return {
      landTotal,
      totalCapex,
      annualKwh,
      annualGross,
      annualSavings,
      annualCovenantSurplus,
      annualOperatorShare,
      monthsToNextLandPurchase,
      hasAnyOutput,
    };
  }, [
    landPricePerAcre,
    acres,
    solarKw,
    peakSunHours,
    deratePct,
    equipmentAndLegalCapex,
    localUtilityRate,
    commonsRate,
    reinvestBps,
  ]);

  return (
    <div className="space-y-6">
      <Panel
        title="Parcel Unit Economics Calculator (User-Supplied Quotes Only)"
        subtitle="Enter your own verified land listing, installer hardware bid, NREL PVWatts peak sun hours, and local utility tariff — zero hardcoded assumptions"
        icon={<Calculator className="h-4 w-4 text-emerald-400" />}
        action={
          <button
            type="button"
            onClick={clearAll}
            className="inline-flex items-center gap-1.5 rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1 text-xs text-slate-300 hover:border-slate-500 hover:text-white cursor-pointer"
          >
            <Eraser className="h-3.5 w-3.5" />
            Clear Inputs
          </button>
        }
      >
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Land Price per Acre (USD)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter actual quote ($/ac)"
              value={landPricePerAcre}
              onChange={(e) => setLandPricePerAcre(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">Parcel Acreage</span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter parcel acres"
              value={acres}
              onChange={(e) => setAcres(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Equipment + Legal Airlock CapEx (USD)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter solar + battery + trust bid ($)"
              value={equipmentAndLegalCapex}
              onChange={(e) => setEquipmentAndLegalCapex(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Solar Array Nameplate (kW DC)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter array kW"
              value={solarKw}
              onChange={(e) => setSolarKw(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Site Peak Sun Hours / Day (NREL PVWatts)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter site PSH/day"
              value={peakSunHours}
              onChange={(e) => setPeakSunHours(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              System Derate Factor (%)
            </span>
            <input
              type="number"
              min={1}
              max={100}
              step="any"
              placeholder="Enter derate % (e.g. 75–85)"
              value={deratePct}
              onChange={(e) => setDeratePct(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Local Utility Retail Tariff (USD / kWh)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="From local utility bill ($/kWh)"
              value={localUtilityRate}
              onChange={(e) => setLocalUtilityRate(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-sky-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Commons Offtake Rate (USD / kWh)
            </span>
            <input
              type="number"
              min={0}
              step="any"
              placeholder="Enter target Commons rate ($/kWh)"
              value={commonsRate}
              onChange={(e) => setCommonsRate(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-sky-500 focus:outline-none"
            />
          </label>

          <label className="space-y-1">
            <span className="block text-[11px] font-medium text-slate-300">
              Covenant Reinvestment Spread (bps, 10000 = 100%)
            </span>
            <input
              type="number"
              min={0}
              max={10000}
              step={1}
              placeholder="Enter reinvest_bps (e.g. 1500 = 15%)"
              value={reinvestBps}
              onChange={(e) => setReinvestBps(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>
        </div>
      </Panel>

      <Panel
        title="Computed Site Output (Pure Arithmetic — No Hidden Multipliers)"
        subtitle="Derived strictly from the inputs entered above"
        icon={<Info className="h-4 w-4 text-sky-400" />}
      >
        {!calc.hasAnyOutput ? (
          <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/40 px-5 py-10 text-center text-xs text-slate-400">
            Enter your actual land quote, hardware bid, site NREL peak sun hours, and local utility
            tariff above to calculate site unit economics.
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-4">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">
                Total Parcel CapEx
              </div>
              <div className="mt-1.5 font-mono text-xl font-bold text-slate-100 tabular-nums">
                {calc.totalCapex !== null
                  ? `$${calc.totalCapex.toLocaleString(undefined, { maximumFractionDigits: 2 })}`
                  : '—'}
              </div>
              <div className="mt-1 font-mono text-[10.5px] text-slate-500">
                Land:{' '}
                {calc.landTotal !== null
                  ? `$${calc.landTotal.toLocaleString(undefined, { maximumFractionDigits: 2 })}`
                  : '—'}
              </div>
            </div>

            <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-4">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">
                Estimated Annual Yield
              </div>
              <div className="mt-1.5 font-mono text-xl font-bold text-amber-300 tabular-nums">
                {calc.annualKwh !== null
                  ? `${Math.round(calc.annualKwh).toLocaleString()} kWh/yr`
                  : '—'}
              </div>
              <div className="mt-1 font-mono text-[10.5px] text-slate-500">
                kW × PSH × 365 × derate
              </div>
            </div>

            <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-4">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">
                Gross Energy Revenue / Savings
              </div>
              <div className="mt-1.5 font-mono text-xl font-bold text-emerald-300 tabular-nums">
                {calc.annualGross !== null
                  ? `$${calc.annualGross.toLocaleString(undefined, { maximumFractionDigits: 2 })}/yr`
                  : '—'}
              </div>
              <div className="mt-1 font-mono text-[10.5px] text-slate-500">
                vs utility:{' '}
                {calc.annualSavings !== null
                  ? `$${calc.annualSavings.toLocaleString(undefined, { maximumFractionDigits: 2 })}/yr`
                  : '—'}
              </div>
            </div>

            <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-4">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">
                Covenant Sweep / Land Payback
              </div>
              <div className="mt-1.5 font-mono text-xl font-bold text-sky-300 tabular-nums">
                {calc.annualCovenantSurplus !== null
                  ? `$${calc.annualCovenantSurplus.toLocaleString(undefined, { maximumFractionDigits: 2 })}/yr`
                  : '—'}
              </div>
              <div className="mt-1 font-mono text-[10.5px] text-slate-500">
                Adjacent land:{' '}
                {calc.monthsToNextLandPurchase !== null
                  ? `${calc.monthsToNextLandPurchase.toFixed(1)} months (energy-only)`
                  : '—'}
              </div>
            </div>
          </div>
        )}
      </Panel>
    </div>
  );
}
