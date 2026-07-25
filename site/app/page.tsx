import { CommandCard } from "@/components/command-card";
import { HeroGlobe } from "@/components/hero-globe";

export default function Home() {
  return (
    <main className="page">
      <section className="hero" aria-label="Overview">
        <div className="hero-top">
          <p className="brand">Network Cartographer</p>
          <a
            className="star-button star-button-inline"
            href="https://github.com/jonbng/network-cartographer"
            target="_blank"
            rel="noopener noreferrer"
          >
            Star on GitHub
          </a>
        </div>

        <h1 className="hero-title">See which apps talk to the internet.</h1>

        <p className="hero-lede">
          Local CLI. Per-app TCP map, traceroutes, globe UI on <code>127.0.0.1</code>. No account.
        </p>

        <CommandCard />

        <HeroGlobe />
      </section>

      <section className="section detail" aria-labelledby="how-heading">
        <h2 id="how-heading">How it works</h2>
        <ol className="steps">
          <li>Read the OS socket table and map each connection to its owning process.</li>
          <li>
            For each destination, run an unprivileged traceroute (
            <code>traceroute</code> / <code>tracepath</code> / <code>tracert</code>).
          </li>
          <li>
            Geocode hops via Network Cartographer after consent, or keep lookups offline with a
            local MaxMind database.
          </li>
          <li>
            Serve the globe UI on <code>127.0.0.1</code> only. Connection and process data
            stay on your machine.
          </li>
        </ol>
        <p className="muted">
          Limits: TCP focus. No HTTPS payload inspection. No cross-platform UDP peer map.
        </p>
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
