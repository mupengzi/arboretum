import type { NextRequest } from "next/server";

/**
 * A same-origin fallback for the chain reads.
 *
 * The page's first choice is to talk to a public Arbitrum Sepolia RPC straight from the
 * browser, because that is the honest version of "your own machine checked this". The
 * public endpoint is behind a load balancer whose backends do not agree on their CORS
 * headers: some responses carry `access-control-allow-origin: *` twice, which browsers
 * reject outright even though the header is permissive.
 *
 * So this route exists as a second path. viem's fallback transport prefers the direct call
 * and only reaches for this one when the direct call fails, which keeps the common case
 * transparent and the demo reliable. Nothing here is trusted: the numbers are compared
 * against a locally generated dataset either way.
 */
const UPSTREAM = "https://sepolia-rollup.arbitrum.io/rpc";

export async function POST(request: NextRequest) {
  const body = await request.text();
  try {
    const upstream = await fetch(UPSTREAM, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body,
      cache: "no-store",
    });
    return new Response(await upstream.text(), {
      status: upstream.status,
      headers: { "content-type": "application/json" },
    });
  } catch (err) {
    return new Response(
      JSON.stringify({
        jsonrpc: "2.0",
        id: null,
        error: { code: -32603, message: err instanceof Error ? err.message : "upstream failed" },
      }),
      { status: 502, headers: { "content-type": "application/json" } },
    );
  }
}