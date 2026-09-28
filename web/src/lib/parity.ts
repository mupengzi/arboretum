import parity from "@/data/parity.json";

/**
 * The host side of every comparison on this page.
 *
 * These prices were produced by the same Rust code the contract was compiled from
 * (`scripts/gen_parity.sh` regenerates them), and each row was checked against the
 * deployed contract. The page contains no pricing code of its own, on purpose: a
 * JavaScript reimplementation would be a second answer in floating point to a question
 * whose only interesting property is that it has exactly one answer.
 */

export type Tenor = { raw: number; label: string };
export type Sigma = { raw: number; label: string };

export const SCALE = BigInt(parity.scale);
export const RATE = BigInt(parity.rate);
export const CARRY = BigInt(parity.carry);

export const spots: number[] = parity.spots;
export const strikes: number[] = parity.strikes;
export const tenors: Tenor[] = parity.tenors;
export const sigmas: Sigma[] = parity.sigmas;

type EuroRow = [number, number, number, number, number, string];

const key = (si: number, ki: number, ti: number, vi: number, pi: number) =>
  `${si}.${ki}.${ti}.${vi}.${pi}`;

const euroIndex = new Map<string, bigint>(
  (parity.european as unknown as EuroRow[]).map((row) => [
    key(row[0], row[1], row[2], row[3], row[4]),
    BigInt(row[5]),
  ]),
);

/** The host build's price for a grid point, or null if the engine declined that case. */
export function hostPrice(
  si: number,
  ki: number,
  ti: number,
  vi: number,
  pi: number,
): bigint | null {
  return euroIndex.get(key(si, ki, ti, vi, pi)) ?? null;
}

type LatRow = { steps: number; american: boolean; kind: number; price: string };

const latticeRows = parity.lattice as unknown as LatRow[];

export function hostLatticePrice(
  steps: number,
  american: boolean,
  kind: number,
): bigint | null {
  const hit = latticeRows.find(
    (r) => r.steps === steps && r.american === american && r.kind === kind,
  );
  return hit ? BigInt(hit.price) : null;
}

export const latticeCases = latticeRows;

/** How many grid points the engine actually priced, for the page to state honestly. */
export const europeanCaseCount = euroIndex.size;