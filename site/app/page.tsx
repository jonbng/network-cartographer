import { CommandCard } from "@/components/command-card";
import { HeroGlobe } from "@/components/hero-globe";

export default function Home() {
  return (
    <main className="page">
      <header className="page-header">
        <h1>Map My Network</h1>
        <p className="tag">local CLI · open source</p>
      </header>

      <section className="section" aria-labelledby="what-heading">
        <h2 id="what-heading">What it does</h2>
        <p>
          A local command that shows which apps on your machine open TCP connections, where
          those connections go, and the traceroute path to each destination — plotted on a
          globe UI served only on loopback.
        </p>
        <p className="muted">
          No account. No installer. Data stays in the local process and goes away when you quit.
        </p>
      </section>

      <section className="section" aria-labelledby="how-heading">
        <h2 id="how-heading">How it works</h2>
        <ol className="steps">
          <li>Read the OS socket table and map each connection to its owning process.</li>
          <li>
            For each destination, run an unprivileged traceroute (
            <code>traceroute</code> / <code>tracepath</code> / <code>tracert</code>).
          </li>
          <li>
            Geocode hops with a local MaxMind database; optional online lookup only after
            explicit consent.
          </li>
          <li>
            Serve the globe UI on <code>127.0.0.1</code> only. Connection data never leaves
            your machine.
          </li>
        </ol>
        <p className="muted">
          Limits: TCP focus. No HTTPS payload inspection. No cross-platform UDP peer map.
        </p>
      </section>

      <section className="section" aria-labelledby="run-heading">
        <h2 id="run-heading">Run it</h2>
        <CommandCard />
      </section>

      <section className="section globe-section" aria-labelledby="globe-heading">
        <h2 id="globe-heading">Preview</h2>
        <p className="muted">
          Sample routes (not your machine). The real tool uses the same kind of hop geometry.
        </p>
        <HeroGlobe />
      </section>

      <footer className="page-footer">
        <span>MIT</span>
        <span aria-hidden="true">·</span>
        <a href="/source">source</a>
        <span aria-hidden="true">·</span>
        <a href="https://github.com/jonbng/network-cartographer/blob/main/SECURITY.md">
          security model
        </a>
      </footer>
    </main>
  );
}
