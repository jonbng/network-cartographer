import { CommandCard } from "@/components/command-card";

function GlobeMark() {
  return (
    <span className="mark" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none">
        <circle cx="12" cy="12" r="8.5" />
        <path d="M3.5 12h17M12 3.5c2.3 2.4 3.5 5.2 3.5 8.5S14.3 18.1 12 20.5M12 3.5C9.7 5.9 8.5 8.7 8.5 12s1.2 6.1 3.5 8.5" />
      </svg>
    </span>
  );
}

export default function Home() {
  return (
    <>
      <div className="page-grid" aria-hidden="true" />
      <header className="site-header">
        <a className="wordmark" href="#top" aria-label="Map My Network home">
          <GlobeMark />
          <span>Map My Network</span>
        </a>
        <nav aria-label="Main navigation">
          <a href="#privacy">Privacy</a>
          <a href="/source">Source</a>
          <a className="nav-cta" href="#run">
            Try it
          </a>
        </nav>
      </header>

      <main id="top">
        <section className="hero">
          <div className="hero-copy">
            <p className="eyebrow">
              <span>01</span> Your network, made visible
            </p>
            <h1>
              See where your
              <br />
              <em>apps are talking.</em>
            </h1>
            <p className="lede">
              A live, local map of every application, destination, and route leaving your
              machine. No account. No installer. One command.
            </p>

            <CommandCard />
            <p className="run-note">
              Downloads a checksummed binary, runs locally, and disappears when you quit.
            </p>
          </div>

          <div className="product-frame" aria-label="Map My Network application preview">
            <div className="frame-bar">
              <span className="frame-title">
                <i /> netcart · live
              </span>
              <span>127.0.0.1:4769</span>
            </div>
            <div className="frame-body">
              <aside className="mock-sidebar">
                <div className="mock-brand">
                  <span>◉</span> Active applications
                </div>
                <div className="mock-search">Search apps, hosts, IPs…</div>
                <div className="mock-app selected">
                  <i className="cyan" />
                  <span>
                    Firefox<small>8 destinations · 42 hops</small>
                  </span>
                  <b>4.8/s</b>
                </div>
                <div className="mock-app">
                  <i className="green" />
                  <span>
                    Signal<small>3 destinations · 18 hops</small>
                  </span>
                  <b>1.2/s</b>
                </div>
                <div className="mock-app">
                  <i className="pink" />
                  <span>
                    Spotify<small>5 destinations · 29 hops</small>
                  </span>
                  <b>0.7/s</b>
                </div>
                <div className="mock-app">
                  <i className="yellow" />
                  <span>
                    Code<small>2 destinations · 12 hops</small>
                  </span>
                  <b>0.3/s</b>
                </div>
              </aside>
              <div className="mock-map">
                <div className="map-meta">
                  <span>LIVE TOPOLOGY</span>
                  <b>18 routes · 64 nodes</b>
                </div>
                <div className="globe">
                  <span className="continent one" />
                  <span className="continent two" />
                  <span className="continent three" />
                  <i className="node n1" />
                  <i className="node n2" />
                  <i className="node n3" />
                  <i className="node n4" />
                  <span className="arc a1" />
                  <span className="arc a2" />
                  <span className="arc a3" />
                </div>
                <span className="map-city city-one">San Juan</span>
                <span className="map-city city-two">Frankfurt</span>
                <span className="map-city city-three">Singapore</span>
              </div>
            </div>
          </div>
        </section>

        <section className="proof-strip" aria-label="Product qualities">
          <div>
            <strong>100%</strong>
            <span>local monitoring</span>
          </div>
          <div>
            <strong>0</strong>
            <span>accounts required</span>
          </div>
          <div>
            <strong>1</strong>
            <span>command to start</span>
          </div>
          <div>
            <strong>∞</strong>
            <span>curiosity encouraged</span>
          </div>
        </section>

        <section className="features">
          <div className="section-heading">
            <p className="eyebrow">
              <span>02</span> Under the surface
            </p>
            <h2>
              Your machine is always talking.
              <br /> Now you can see the conversation.
            </h2>
          </div>
          <div className="feature-grid">
            <article>
              <span className="feature-index">A / 01</span>
              <h3>Per-app visibility</h3>
              <p>
                See the applications creating connections, their destinations, ports,
                activity, and organizations.
              </p>
            </article>
            <article>
              <span className="feature-index">B / 02</span>
              <h3>Routes on a globe</h3>
              <p>
                Follow normal-user traceroutes across transit nodes and destinations in an
                interactive 3D view.
              </p>
            </article>
            <article>
              <span className="feature-index">C / 03</span>
              <h3>Built to stay local</h3>
              <p>
                The monitor and operational frontend run on your machine through a
                loopback-only local server.
              </p>
            </article>
          </div>
        </section>

        <section className="privacy" id="privacy">
          <div>
            <p className="eyebrow">
              <span>03</span> A deliberate boundary
            </p>
            <h2>
              Your connection graph
              <br /> is not our business.
            </h2>
          </div>
          <div className="privacy-copy">
            <p>
              Process names, connection lists, and route relationships remain inside the local{" "}
              <code>netcart</code> process.
            </p>
            <p>
              Optional geolocation may look up public hop IP addresses after explicit consent.
              The source is public, and offline geolocation remains available.
            </p>
            <a href="https://github.com/jonbng/network-cartographer/blob/main/SECURITY.md">
              Read the security model <span>↗</span>
            </a>
          </div>
        </section>

        <section className="final-cta">
          <p className="eyebrow">
            <span>04</span> Start exploring
          </p>
          <h2>
            The internet leaves a trail.
            <br />
            <em>Map yours.</em>
          </h2>
          <a href="#run">
            Get the command <span>↓</span>
          </a>
        </section>
      </main>

      <footer>
        <a className="wordmark" href="#top">
          <span className="mark mini">◉</span>
          <span>Map My Network</span>
        </a>
        <p>Open source · MIT licensed · built by Jonathan Bangert</p>
        <a href="/source">GitHub ↗</a>
      </footer>
    </>
  );
}
