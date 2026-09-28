"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { ABI, CONTRACT, rawInt, publicClient } from "@/lib/engine";
import { CARRY, RATE, hostLatticePrice } from "@/lib/parity";

type Cell = {
  keys: [boolean, number]; // american, kind (0 call, 1 put)
  label: string;
  price: bigint | null;
  gas: bigint | null;
  host: bigint | null;
  failed: boolean;
};

const SPOT = 250_000_000_000n;
const STRIKE = 240_000_000_000n;
const TENOR = 250_000_000n;
const SIGMA = 350_000_000n;

const shape: Array<{ keys: [boolean, number]; label: string }> = [
  { keys: [false, 0], label: "European call" },
  { keys: [false, 1], label: "European put" },
  { keys: [true, 0], label: "American call" },
  { keys: [true, 1], label: "American put" },
];

export function LatticePanel() {
  const [steps, setSteps] = useState(512);
  const [cells, setCells] = useState<Cell[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const cache = useRef(new Map<string, Cell[]>());

  const load = useCallback(async (n: number) => {
    const cached = cache.current.get(String(n));
    if (cached) {
      setCells(cached);
      setError(null);
      return;
    }
    setCells(null);
    setError(null);
    try {
      const results = await Promise.all(
        shape.map(async ({ keys: [american, kind] }) => {
          const isPut = kind === 1;
          const args = [SPOT, STRIKE, TENOR, SIGMA, RATE, CARRY, isPut, american, n] as const;
          const [price, gas] = await Promise.all([
            publicClient.readContract({
              address: CONTRACT,
              abi: ABI,
              functionName: "priceLattice",
              args,
            }),
            publicClient
              .estimateContractGas({
                address: CONTRACT,
                abi: ABI,
                functionName: "priceLattice",
                args,
              })
              .catch(() => null),
          ]);
          return { price, gas };
        }),
      );
      const next: Cell[] = shape.map((s, i) => ({
        ...s,
        price: results[i].price,
        gas: results[i].gas,
        host: hostLatticePrice(n, s.keys[0], s.keys[1]),
        failed: false,
      }));
      cache.current.set(String(n), next);
      setCells(next);
    } catch (err) {
      setError(err instanceof Error ? err.message.split("\n")[0] : "the call failed");
    }
  }, []);

  useEffect(() => {
    void load(steps);
  }, [load, steps]);

  const europeanGas = cells?.find((c) => c.label === "European call")?.gas ?? null;

  return (
    <div className="rounded-ui border border-line bg-surface">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-5 py-3">
        <span className="font-mono text-[13px] text-muted">
          priceLattice(250, 240, 3m, 35%, call or put)
        </span>
        <div className="grid grid-cols-2 gap-1">
          {[256, 512].map((n) => (
            <button
              key={n}
              type="button"
              aria-pressed={n === steps}
              onClick={() => setSteps(n)}
              className={`rounded-ui border px-3 py-1 font-mono text-[13px] tabular-nums transition-colors duration-150 active:translate-y-px ${
                n === steps
                  ? "border-accent/60 bg-accent/10 text-accent"
                  : "border-line bg-surface text-muted hover:bg-elevated hover:text-ink"
              }`}
            >
              {n} steps
            </button>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4">
        {shape.map((s, i) => {
          const cell = cells?.[i];
          const match = cell && cell.host !== null && cell.price !== null ? cell.price === cell.host : null;
          return (
            <div key={s.label} className={`px-5 py-4 ${i > 0 ? "border-t border-line sm:border-l sm:border-t-0" : ""}`}>
              <div className="text-[13px] text-muted">{s.label}</div>
              <div className="mt-2 h-6">
                {cell ? (
                  <div className="font-mono text-lg tabular-nums">{rawInt(cell.price!)}</div>
                ) : (
                  <div className="skeleton h-6 w-32" />
                )}
              </div>
              <div className="mt-2 flex items-center justify-between gap-2">
                <span className="font-mono text-[12px] text-muted tabular-nums">
                  {cell?.gas ? `${rawInt(cell.gas)} gas` : "gas -"}
                </span>
                {match !== null && (
                  <span className={`font-mono text-[12px] ${match ? "text-accent" : "text-danger"}`}>
                    {match ? "MATCH" : "DIFFERS"}
                  </span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <p className="border-t border-line px-5 py-4 text-[13px] leading-relaxed text-muted">
        {error
          ? `The lattice reads did not complete: ${error}`
          : cells
            ? `An early-exercise tree at ${steps} steps costs ${rawInt(cells.find((c) => c.label === "American put")!.gas ?? 0n)} gas. The same tree without the exercise check costs ${rawInt(europeanGas ?? 0n)}, and the closed form in the panel above costs 72,442. Arbitrum's gas documentation gives loop-heavy computation a 50 to 100 times advantage under Stylus over the EVM equivalent.`
            : "Reading the deployed contract."}
      </p>
    </div>
  );
}