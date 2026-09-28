import { createPublicClient, fallback, http, type Address } from "viem";

/** The live deployment. Arbitrum Sepolia, activated, verified bit-identical to a local
 *  build of the same source by scripts/verify_onchain.sh. */
export const CONTRACT: Address = "0x374f469725d735115b8b15dee3f8749ff929d94a";

export const CHAIN_ID = 421614;
export const CHAIN_NAME = "Arbitrum Sepolia";
export const RPC_URL = "https://sepolia-rollup.arbitrum.io/rpc";
export const EXPLORER = `https://sepolia.arbiscan.io/address/${CONTRACT}`;
export const DEPLOY_TX = `https://sepolia.arbiscan.io/tx/0xeae8c5dd5bc09d8b3d866ad7eaedd86fd827d418a63c894b63245a39dd5ee037`;
export const ACTIVATION_TX = `https://sepolia.arbiscan.io/tx/0xa562c36deb7fd9eebc64e60cfc0ed2e29c1b22c5e46a4c38d851c8080e97d7be`;

export const REPO = "https://github.com/";

/** The browser talks to the chain directly, which is the point: the reviewer's own machine
 *  makes the call, no backend of ours is in the path, and no wallet is involved because
 *  every method on this contract is a view.
 *
 *  The fallbacks exist because the public endpoint sits behind a load balancer whose
 *  backends disagree about CORS headers: some responses carry `access-control-allow-origin`
 *  twice, which a browser rejects even though the value is permissive. The same-origin
 *  route takes over only when the direct call fails, and a second public endpoint sits
 *  behind that. Which path answered does not change any number: every result is compared
 *  against the dataset in src/data/parity.json either way. */
export const publicClient = createPublicClient({
  transport: fallback([
    http(RPC_URL, { timeout: 20_000, retryCount: 1 }),
    http("/api/rpc", { timeout: 20_000, retryCount: 1 }),
    http("https://arbitrum-sepolia-rpc.publicnode.com", { timeout: 20_000, retryCount: 1 }),
  ]),
});

const i128 = { type: "int128" } as const;

export const ABI = [
  {
    type: "function",
    name: "scale",
    stateMutability: "view",
    inputs: [],
    outputs: [i128],
  },
  {
    type: "function",
    name: "priceEuropean",
    stateMutability: "view",
    inputs: [
      { name: "spot", ...i128 },
      { name: "strike", ...i128 },
      { name: "t", ...i128 },
      { name: "sigma", ...i128 },
      { name: "rate", ...i128 },
      { name: "carry", ...i128 },
      { name: "isPut", type: "bool" },
    ],
    outputs: [i128],
  },
  {
    type: "function",
    name: "priceLattice",
    stateMutability: "view",
    inputs: [
      { name: "spot", ...i128 },
      { name: "strike", ...i128 },
      { name: "t", ...i128 },
      { name: "sigma", ...i128 },
      { name: "rate", ...i128 },
      { name: "carry", ...i128 },
      { name: "isPut", type: "bool" },
      { name: "american", type: "bool" },
      { name: "steps", type: "uint32" },
    ],
    outputs: [i128],
  },
  {
    type: "function",
    name: "noiseBand",
    stateMutability: "view",
    inputs: [
      { name: "spot", ...i128 },
      { name: "strike", ...i128 },
      { name: "t", ...i128 },
      { name: "sigma", ...i128 },
      { name: "rate", ...i128 },
      { name: "carry", ...i128 },
    ],
    outputs: [i128],
  },
] as const;

export const SCALE = 1_000_000_000n;

/** Grouped digits. The raw integer is what the chain actually returned, so it is shown as
 *  it is rather than rounded into a nicer shape. */
export function rawInt(v: bigint): string {
  return v.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Nine decimal places is the representation's own resolution; anything shorter would be
 *  hiding digits the chain returns. */
export function decimal(v: bigint): string {
  const neg = v < 0n;
  const abs = neg ? -v : v;
  const whole = abs / SCALE;
  const frac = (abs % SCALE).toString().padStart(9, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${whole}${frac ? "." + frac : ""}`;
}

export function shortened(addr: string): string {
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}