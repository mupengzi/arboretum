"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { encodeFunctionData } from "viem";
import { ABI, CHAIN_NAME, CONTRACT, decimal, publicClient, rawInt } from "@/lib/engine";
import { CARRY, RATE, hostPrice, sigmas, spots, strikes, tenors, europeanCaseCount } from "@/lib/parity";

type Outcome =
  | { state: "loading" }
  | {
      state: "ok";
      chain: bigint;
      host: bigint | null;
      band: bigint | null;
      gas: bigint | null;
      ms: number;
      calldata: `0x${string}`;
    }
  | { state: "error"; message: string; host: bigint | null };

const money = (raw: number) => (raw / 1e9).toLocaleString("en-US", { maximumFractionDigits: 2 });

function Choice({
  label,
  note,
  options,
  value,
  onChange,
  cols = "grid-cols-4",
}: {
  label: string;
  note: string;
  options: string[];
  value: number;
  onChange: (i: number) => void;
  cols?: string;
}) {
  return (
    <div className="grid gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[13px] text-muted">{label}</span>
        <span className="font-mono text-[13px] text-muted tabular-nums">{note}</span>
      </div>
      <div className={`grid ${cols} gap-1`}>
        {options.map((o, i) => (
          <button
            key={o}
            type="button"
            aria-pressed={i === value}
            onClick={() => onChange(i)}
            className={`rounded-ui border px-2 py-1.5 font-mono text-[13px] tabular-nums transition-colors duration-150 active:translate-y-px ${
              i === value
                ? "border-accent/60 bg-accent/10 text-accent"
                : "border-line bg-surface text-muted hover:border-line hover:bg-elevated hover:text-ink"
            }`}
          >
            {o}
          </button>
        ))}
      </div>
    </div>
  );
}

export function QuoteTool() {
  const [si, setSi] = useState(2); // 250
  const [ki, setKi] = useState(3); // 240
  const [ti, setTi] = useState(2); // 3 months
  const [vi, setVi] = useState(3); // 35%
  const [isPut, setIsPut] = useState(false);
  const [outcome, setOutcome] = useState<Outcome>({ state: "loading" });

  const cache = useRef(new Map<string, Outcome>());
  const request = useRef(0);

  const pi = isPut ? 1 : 0;
  const args = useMemo(
    () => [BigInt(spots[si]), BigInt(strikes[ki]), BigInt(tenors[ti].raw), BigInt(sigmas[vi].raw), RATE, CARRY, isPut] as const,
    [si, ki, ti, vi, isPut],
  );

  const load = useCallback(async () => {
    const id = ++request.current;
    const cacheKey = `${si}.${ki}.${ti}.${vi}.${pi}`;
    const host = hostPrice(si, ki, ti, vi, pi);

    const cached = cache.current.get(cacheKey);
    if (cached) {
      setOutcome(cached);
      return;
    }
    setOutcome({ state: "loading" });

    const started = performance.now();
    try {
      const [price, band] = await Promise.all([
        publicClient.readContract({
          address: CONTRACT,
          abi: ABI,
          functionName: "priceEuropean",
          args,
        }),
        publicClient.readContract({
          address: CONTRACT,
          abi: ABI,
          functionName: "noiseBand",
          args: [BigInt(spots[si]), BigInt(strikes[ki]), BigInt(tenors[ti].raw), BigInt(sigmas[vi].raw), RATE, CARRY],
        }),
      ]);
      const ms = performance.now() - started;

      // Gas is a separate question and a separate call; if it fails the price still stands,
      // so it is allowed to fail quietly.
      const gas = await publicClient
        .estimateContractGas({ address: CONTRACT, abi: ABI, functionName: "priceEuropean", args })
        .catch(() => null);

      if (request.current !== id) return;
      const next: Outcome = { state: "ok", chain: price, host, band, gas, ms, calldata: encodeFunctionData({ abi: ABI, functionName: "priceEuropean", args }) };
      cache.current.set(cacheKey, next);
      setOutcome(next);
    } catch (err) {
      if (request.current !== id) return;
      const message = err instanceof Error ? err.message.split("\n")[0] : "the call failed";
      setOutcome({ state: "error", message, host });
    }
  }, [si, ki, ti, vi, pi, args]);

  useEffect(() => {
    void load();
  }, [load]);

  const match =
    outcome.state === "ok" && outcome.host !== null ? outcome.chain === outcome.host : null;

  return (
    <div className="grid gap-8 lg:grid-cols-12 lg:gap-10">
      {/* Inputs */}
      <div className="grid content-start gap-6 lg:col-span-4">
        <Choice
          label="Underlying"
          note={money(spots[si])}
          options={spots.map((s) => money(s))}
          value={si}
          onChange={setSi}
        />
        <Choice
          label="Strike"
          note={money(strikes[ki])}
          options={strikes.map((s) => money(s))}
          value={ki}
          onChange={setKi}
        />
        <Choice
          label="Tenor"
          note={tenors[ti].label}
          options={tenors.map((t) => t.label)}
          value={ti}
          onChange={setTi}
          cols="grid-cols-5"
        />
        <Choice
          label="Volatility"
          note={sigmas[vi].label}
          options={sigmas.map((s) => s.label)}
          value={vi}
          onChange={setVi}
          cols="grid-cols-3"
        />
        <Choice
          label="Type"
          note={isPut ? "put" : "call"}
          options={["Call", "Put"]}
          value={pi}
          onChange={(i) => setIsPut(i === 1)}
          cols="grid-cols-2"
        />
        <p className="border-t border-line pt-4 text-[13px] leading-relaxed text-muted">
          Rate and carry are fixed at 5% and 1% so the grid stays finite. Volatility is an
          input here, as it is everywhere else: there is no live implied-volatility surface
          for tokenised equities to read yet.
        </p>
      </div>

      {/* Result */}
      <div className="min-w-0 lg:col-span-8">
        <div className="rounded-ui border border-line bg-surface">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-5 py-3">
            <span className="font-mono text-[13px] text-muted">
              priceEuropean({" "}
              {[money(spots[si]), money(strikes[ki]), tenors[ti].label, sigmas[vi].label, isPut ? "put" : "call"].join(", ")}
              {" "})
            </span>
            <span className="font-mono text-[13px] text-muted">{CHAIN_NAME}</span>
          </div>

          <div className="grid gap-6 px-5 py-6 sm:grid-cols-2">
            <div className="min-w-0">
              <div className="text-[13px] text-muted">Returned by the contract</div>
              <div className="mt-2 h-9">
                {outcome.state === "loading" ? (
                  <div className="skeleton h-9 w-56" />
                ) : outcome.state === "ok" ? (
                  <div className="break-all font-mono text-3xl tabular-nums">{rawInt(outcome.chain)}</div>
                ) : (
                  <div className="font-mono text-3xl text-muted">-</div>
                )}
              </div>
              <div className="mt-1 font-mono text-[13px] text-muted">
                {outcome.state === "ok" ? decimal(outcome.chain) : "the raw integer, at 1e9 scale"}
              </div>
            </div>

            <div className="min-w-0">
              <div className="text-[13px] text-muted">Same inputs, local build</div>
              <div className="mt-2 flex h-9 items-center justify-between gap-3">
                {outcome.state === "loading" ? (
                  <div className="skeleton h-9 w-56" />
                ) : outcome.host !== null ? (
                  <div className="break-all font-mono text-3xl tabular-nums text-muted">
                    {rawInt(outcome.host)}
                  </div>
                ) : (
                  <div className="text-[13px] text-muted">not in the shipped grid</div>
                )}
                {match !== null && (
                  <span
                    className={`rounded-ui border px-2 py-1 font-mono text-[13px] ${
                      match
                        ? "border-accent/50 bg-accent/10 text-accent"
                        : "border-danger/50 bg-danger/10 text-danger"
                    }`}
                  >
                    {match ? "MATCH" : "DIFFERS"}
                  </span>
                )}
              </div>
              <div className="mt-1 font-mono text-[13px] text-muted">
                {match ? "bit for bit, no tolerance applied" : outcome.state === "error" ? "shown without the chain" : "\u00a0"}
              </div>
            </div>
          </div>

          <dl className="grid grid-cols-2 border-t border-line sm:grid-cols-4">
            {[
              ["Noise band", outcome.state === "ok" && outcome.band !== null ? rawInt(outcome.band) : "-"],
              ["Estimated gas", outcome.state === "ok" && outcome.gas !== null ? rawInt(outcome.gas) : "-"],
              ["Round trip", outcome.state === "ok" ? `${outcome.ms.toFixed(0)} ms` : "-"],
              [
                "Grid points",
                `${europeanCaseCount.toLocaleString("en-US")}`,
              ],
            ].map(([k, v], i) => (
              <div key={k} className={`min-w-0 px-4 py-3 sm:px-5 ${i > 0 ? "border-l border-line" : ""}`}>
                <dt className="text-[13px] text-muted">{k}</dt>
                <dd className="mt-1 break-all font-mono text-[13px] tabular-nums">{v}</dd>
              </div>
            ))}
          </dl>

          {outcome.state === "error" && (
            <div className="border-t border-line px-5 py-4">
              <p className="text-[13px] leading-relaxed text-danger">
                The read did not complete: {outcome.message}
              </p>
              <button
                type="button"
                onClick={() => void load()}
                className="mt-3 rounded-ui border border-line bg-elevated px-3 py-1.5 text-[13px] transition-colors hover:border-accent/50 hover:text-accent active:translate-y-px"
              >
                Try again
              </button>
            </div>
          )}

          {outcome.state === "ok" && (
            <div className="border-t border-line px-5 py-4">
              <div className="text-[13px] text-muted">The call this page made</div>
              <code className="mt-2 block break-all font-mono text-[12px] leading-relaxed text-muted">
                {CONTRACT} · {outcome.calldata}
              </code>
              <p className="mt-2 text-[13px] leading-relaxed text-muted">
                Replay it against any Arbitrum Sepolia RPC with eth_call, or read the same
                value from a node of your own. Nothing about this number is private to us.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}