import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Static export: the page is entirely client-side apart from rendering, and the host
  // serves frozen files without running Functions. Every chain read happens in the
  // visitor's browser, so there is nothing for a server to do.
  output: "export",
  reactStrictMode: true,
};

export default nextConfig;