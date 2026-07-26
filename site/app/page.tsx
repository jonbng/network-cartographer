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
            href="/source"
            target="_blank"
            rel="noopener noreferrer"
          >
            Star on GitHub
          </a>
        </div>

        <h1 className="hero-title">See where every app connects on a live 3D globe</h1>

        <p className="hero-lede">
          A tool to view a live map of your apps’ network requests and trace where they go.
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
            Geocode each hop through Network Cartographer, or use a local MaxMind database for
            offline lookups.
          </li>
          <li>
            Serve the globe UI on <code>127.0.0.1</code> only. Connection and process data
            stay on your machine.
          </li>
        </ol>
        <p className="muted">
          Limits: no payload inspection. UDP coverage includes connected sockets only.
        </p>
      </section>

      <footer className="page-footer">
        <span>MIT</span>
        <span aria-hidden="true">·</span>
        <a href="https://jonathanb.dk">Jonathan Bangert</a>
        <span aria-hidden="true">·</span>
        <a href="/source">source</a>
      </footer>
    </main>
  );
}
