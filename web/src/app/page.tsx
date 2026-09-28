import { ArrowUpRight, CheckCircle } from "@phosphor-icons/react/dist/ssr";
import { LatticePanel } from "@/components/LatticePanel";
import { QuoteTool } from "@/components/QuoteTool";
import { ACTIVATION_TX, CHAIN_NAME, CONTRACT, DEPLOY_TX, EXPLORER, shortened } from "@/lib/engine";

/* Numbers stated here are measured, not estimated, and each one names the file or command
   that produces it. Nothing on this page is a projection. */
const headline = [
  { value: "3336", unit: "reference cases", note: "docs/ACCURACY.md" },
  { value: "41", unit: "tests, none failing", note: "cargo test, forge test" },
  { value: "0", unit: "float instructions", note: "scripts/verify_no_floats.sh" },
  { value: "10 / 10", unit: "entry points matching", note: "scripts/verify_onchain.sh" },
];

const proof = [
  {
    cmd: "scripts/verify_onchain.sh",
    what: "Calls every method on the live contract and compares each answer against a local build of the same source.",
  },
  {
    cmd: "scripts/verify_no_floats.sh",
    what: "Validates the compiled module and disassembles it. Any f32 or f64 instruction fails the run.",
  },
  {
    cmd: "cargo run -p arbreport",
    what: "Regenerates the accuracy report from 3336 reference cases produced by CPython's math module.",
  },
  {
    cmd: "py reference/gen_vectors.py",
    what: "Rebuilds those reference vectors from scratch, so the report can be audited rather than believed.",
  },
  {
    cmd: "cargo stylus check",
    what: "Rebuilds the contract, recompresses it, and prices the deployment against a live chain.",
  },
];

const limits = [
  {
    title: "Volatility is an input",
    body: "There is no live implied-volatility surface for tokenised equities to read yet. The contribution is the transform from inputs to price, and the surface belongs behind a signed or committed input that is designed for but not built.",
  },
  {
    title: "Accurate, not exact",
    body: "The normal CDF is Abramowitz and Stegun 26.2.17, with a published absolute error of 7.5e-8 and a measured 7.55e-8 here. Monotonicity in volatility holds to within that, not beyond it.",
  },
  {
    title: "Small numbers flush to zero",
    body: "Premiums below one quantum, 1e-9, are returned as zero rather than as cancellation noise. The band where that happens is derived from the CDF's error rather than chosen.",
  },
  {
    title: "Not audited",
    body: "The engine, the contract and the fixed-point library have had no external review. Deployment is on a testnet. Treat it as a careful prototype, not as infrastructure.",
  },
];

function Section({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section id={id} className="border-t border-line">
      <div className="mx-auto max-w-[1400px] px-5 py-16 sm:px-8 lg:py-20">
        <h2 className="max-w-[24ch] text-2xl font-medium tracking-tight text-ink sm:text-3xl">
          {title}
        </h2>
        <div className="mt-8">{children}</div>
      </div>
    </section>
  );
}

export default function Page() {
  return (
    <>
      <header className="sticky top-0 z-40 border-b border-line bg-bg/85 backdrop-blur">
        <div className="mx-auto flex h-16 max-w-[1400px] items-center justify-between gap-4 px-5 sm:px-8">
          <a href="#top" className="font-mono text-[15px] tracking-tight text-ink">
            arboretum
          </a>
          <div className="flex items-center gap-5">
            <a
              href={EXPLORER}
              target="_blank"
              rel="noreferrer"
              className="group hidden items-center gap-1.5 font-mono text-[13px] text-muted transition-colors hover:text-ink sm:flex"
            >
              {shortened(CONTRACT)}
              <ArrowUpRight size={13} weight="bold" className="transition-transform group-hover:-translate-y-0.5" />
            </a>
            <a
              href="#quote"
              className="whitespace-nowrap rounded-ui border border-line bg-surface px-3 py-1.5 text-[13px] text-ink transition-colors hover:border-accent/50 hover:text-accent active:translate-y-px"
            >
              Quote on-chain
            </a>
          </div>
        </div>
      </header>

      <main id="top">
        {/* Hero. Asymmetric split: the claim on the left, the numbers that back it on the
            right. No centered headline, no feature cards.
            TODO(image): this page wants one dark macro texture behind this band, roughly
            1792x1024, near-black with a faint green specular streak. Image generation is
            unavailable in the authoring environment, so the slot is left empty rather than
            filled with an illustration. Drop a file at web/public/hero-texture.png and
            layer it here at low opacity with a gradient mask. */}
        <div className="mx-auto grid max-w-[1400px] gap-12 px-5 pb-16 pt-20 sm:px-8 lg:grid-cols-12 lg:gap-16 lg:pb-24 lg:pt-24">
          <div className="min-w-0 lg:col-span-7">
            <h1 className="rise text-4xl font-medium leading-[1.05] tracking-tight text-ink sm:text-5xl lg:text-6xl">
              An option price you can recompute.
            </h1>
            <p className="rise rise-1 mt-6 max-w-[52ch] text-base leading-relaxed text-muted">
              Deterministic derivatives pricing in Rust on Arbitrum. No floating point. The
              deployed contract and a local build agree bit for bit.
            </p>
            <div className="rise rise-2 mt-8 flex flex-wrap items-center gap-3">
              <a
                href="#quote"
                className="rounded-ui bg-accent px-4 py-2.5 text-sm font-medium text-accent-ink transition-colors hover:brightness-110 active:translate-y-px"
              >
                Quote on-chain
              </a>
              <a
                href={EXPLORER}
                target="_blank"
                rel="noreferrer"
                className="rounded-ui border border-line bg-surface px-4 py-2.5 text-sm text-ink transition-colors hover:border-accent/50 hover:text-accent active:translate-y-px"
              >
                View the contract
              </a>
            </div>
            <p className="rise rise-3 mt-6 font-mono text-[13px] text-muted">
              {CHAIN_NAME} · no wallet needed, every method is a view
            </p>
          </div>

          <div className="rise rise-2 min-w-0 lg:col-span-5">
            <dl className="rounded-ui border border-line bg-surface">
              {headline.map((row, i) => (
                <div
                  key={row.unit}
                  className={`flex items-baseline justify-between gap-4 px-5 py-4 ${
                    i > 0 ? "border-t border-line" : ""
                  }`}
                >
                  <div className="min-w-0">
                    <dd className="font-mono text-2xl tabular-nums text-ink">{row.value}</dd>
                    <dt className="mt-0.5 text-[13px] text-muted">{row.unit}</dt>
                  </div>
                  <span className="break-all text-right font-mono text-[12px] text-muted">
                    {row.note}
                  </span>
                </div>
              ))}
            </dl>
          </div>
        </div>

        <Section id="quote" title="Ask the chain. Then ask your own machine.">
          <QuoteTool />
        </Section>

        <Section id="lattice" title="Early exercise, and what it costs to price it.">
          <LatticePanel />
        </Section>

        <section id="why" className="border-t border-line">
          <div className="mx-auto max-w-[1400px] px-5 py-16 sm:px-8 lg:py-20">
            <h2 className="max-w-[24ch] text-2xl font-medium tracking-tight text-ink sm:text-3xl">
              Why this belongs on-chain, and where it stops belonging.
            </h2>
            <div className="mt-8 grid gap-8 lg:grid-cols-12">
              <div className="grid max-w-[65ch] gap-5 text-base leading-relaxed text-muted lg:col-span-7">
                <p>
                  An option price in DeFi usually arrives as an assertion. A model runs
                  somewhere off-chain, an oracle delivers the number, and the contract
                  executes what it was told. The figure that decides a liquidation cannot be
                  inspected or reproduced by anyone holding the position.
                </p>
                <p>
                  Computing it on-chain changes its kind. Given the same integers, every
                  validator runs the same integer arithmetic and reaches the same result, so
                  the price becomes a computation with a proof anyone can re-run. That is
                  what the MATCH badges above are showing: not that two systems were
                  configured alike, but that one of them can be checked against the other.
                </p>
              </div>
              <div className="lg:col-span-5">
                <div className="rounded-ui border border-line bg-surface p-5">
                  <p className="text-base leading-relaxed text-ink">
                    Verifiable computation costs one to two orders of magnitude more than
                    trusting a feed.
                  </p>
                  <p className="mt-4 text-[13px] leading-relaxed text-muted">
                    So it is scoped to where the stakes justify it: settlement and expiry
                    prices, fallback marks when a feed is stale or contested, collateral
                    valuation, and reproducible audit. It is not a market maker, and it does
                    not try to be one. Streaming quotes should stay off-chain.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </section>

        <Section id="proof" title="How to check all of this yourself.">
          <div className="grid gap-px overflow-hidden rounded-ui border border-line bg-line sm:grid-cols-2 lg:grid-cols-5">
            {proof.map((item) => (
              <div key={item.cmd} className="bg-surface p-5">
                <code className="block break-all font-mono text-[12px] leading-relaxed text-accent">
                  {item.cmd}
                </code>
                <p className="mt-3 text-[13px] leading-relaxed text-muted">{item.what}</p>
              </div>
            ))}
          </div>
          <div className="mt-8 flex flex-wrap items-center gap-x-8 gap-y-3">
            <span className="flex items-center gap-2 text-[13px] text-muted">
              <CheckCircle size={15} weight="bold" className="text-accent" />
              Deployed{" "}
              <a href={DEPLOY_TX} target="_blank" rel="noreferrer" className="font-mono underline decoration-line underline-offset-2 transition-colors hover:text-ink">
                deploy
              </a>
            </span>
            <span className="flex items-center gap-2 text-[13px] text-muted">
              <CheckCircle size={15} weight="bold" className="text-accent" />
              Activated{" "}
              <a href={ACTIVATION_TX} target="_blank" rel="noreferrer" className="font-mono underline decoration-line underline-offset-2 transition-colors hover:text-ink">
                activation
              </a>
            </span>
            <span className="flex items-center gap-2 text-[13px] text-muted">
              <CheckCircle size={15} weight="bold" className="text-accent" />
              14,668 bytes compressed, against a 96 KB limit
            </span>
          </div>
        </Section>

        <Section id="limits" title="What this does not do.">
          <div className="grid gap-8 sm:grid-cols-2 lg:gap-10">
            {limits.map((l) => (
              <div key={l.title}>
                <h3 className="text-base font-medium text-ink">{l.title}</h3>
                <p className="mt-2 max-w-[52ch] text-[13px] leading-relaxed text-muted">{l.body}</p>
              </div>
            ))}
          </div>
        </Section>
      </main>

      <footer className="border-t border-line">
        <div className="mx-auto flex max-w-[1400px] flex-col gap-6 px-5 py-10 sm:px-8 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <span className="font-mono text-[13px] text-muted">{shortened(CONTRACT)}</span>
            <span className="text-[13px] text-muted">{CHAIN_NAME}</span>
            <span className="text-[13px] text-muted">MIT</span>
          </div>
          <div className="flex items-center gap-5">
            {/* One real mark, monochrome, from Simple Icons. There is no Arbitrum entry in
                that set, so the chain is named in text rather than illustrated with a
                broken image or someone else's logo. */}
            <span className="flex items-center gap-2">
              <img
                src="https://cdn.simpleicons.org/rust/9c9ca8"
                alt=""
                width={16}
                height={16}
                loading="lazy"
              />
              <span className="text-[13px] text-muted">Rust engine</span>
            </span>
            <span className="text-[13px] text-muted">Arbitrum Stylus contract</span>
          </div>
        </div>
      </footer>
    </>
  );
}