import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // The page talks to a public Arbitrum Sepolia RPC straight from the browser, so a
  // reviewer's own machine is what queries the chain. No backend sits in between, and
  // nothing here needs a wallet.
  reactStrictMode: true,
};

export default nextConfig;