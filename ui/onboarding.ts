const ONBOARDING_KEY = "network-cartographer.onboarding.v1";
const PRIVACY_KEY = "network-cartographer.privacy.accepted";

const TIPS = [
  "Drag to orbit the globe · scroll to zoom (globe only, not the page)",
  "Pink ★ markers are final destinations · colors match apps in the sidebar",
  "Click an app to focus its paths · Shift-click to multi-select",
  "Hover a hop to see every app going through that node · Recenter frames traffic",
];

export type OnboardingOptions = {
  privacyAccepted: boolean;
  onAcceptPrivacy: () => void | Promise<void>;
};

function mountPrivacyModal(onAccept: () => void | Promise<void>): void {
  const backdrop = document.createElement("div");
  backdrop.className = "modal";
  backdrop.id = "privacy-modal";
  backdrop.setAttribute("role", "dialog");
  backdrop.setAttribute("aria-modal", "true");
  backdrop.setAttribute("aria-labelledby", "privacy-title");
  backdrop.innerHTML = `
    <div class="modal-card">
      <h2 id="privacy-title">Privacy notice</h2>
      <p>
        <strong>Network Cartographer</strong> monitors network connections <em>on this machine</em>.
        Connection lists and process names stay local — they are not uploaded to a Network Cartographer server.
      </p>
      <ul>
        <li>
          For map placement, hop / destination <strong>IP addresses</strong> may be sent to
          third-party GeoIP APIs (<code>ip-api.com</code>, <code>ipwho.is</code>) unless you use a
          local MaxMind GeoLite2 database.
        </li>
        <li>Reverse DNS may be queried for hostnames and airport codes.</li>
        <li>Traceroute runs OS tools (<code>traceroute</code> / <code>tracert</code>) with limited concurrency.</li>
      </ul>
      <p class="modal-note">
        Prefer offline geo by placing <code>GeoLite2-City.mmdb</code> on disk or setting
        <code>NETWORK_CARTOGRAPHER_MMDB</code> (see README). Free ip-api.com lookups use HTTP only.
      </p>
      <div class="modal-actions">
        <button type="button" class="btn" data-accept>I understand — continue</button>
      </div>
    </div>
  `;
  // Attach to body so fixed positioning is not constrained by overlay hosts
  document.body.appendChild(backdrop);

  backdrop.querySelector("[data-accept]")!.addEventListener("click", () => {
    void (async () => {
      try {
        await onAccept();
      } finally {
        try {
          localStorage.setItem(PRIVACY_KEY, "1");
        } catch {
          /* ignore */
        }
        backdrop.remove();
      }
    })();
  });
}

function mountTips(host: HTMLElement): void {
  if (localStorage.getItem(ONBOARDING_KEY) === "done") return;

  let i = 0;
  const bar = document.createElement("div");
  bar.className = "onboarding";
  bar.innerHTML = `
    <span class="onboarding-text"></span>
    <span class="onboarding-actions">
      <button type="button" class="btn ghost sm" data-next>Next</button>
      <button type="button" class="btn ghost sm" data-skip>Dismiss</button>
    </span>
  `;
  host.appendChild(bar);
  const text = bar.querySelector(".onboarding-text") as HTMLElement;

  const show = () => {
    text.textContent = TIPS[i] ?? "";
  };
  show();

  bar.querySelector("[data-next]")!.addEventListener("click", () => {
    i += 1;
    if (i >= TIPS.length) {
      localStorage.setItem(ONBOARDING_KEY, "done");
      bar.remove();
    } else show();
  });
  bar.querySelector("[data-skip]")!.addEventListener("click", () => {
    localStorage.setItem(ONBOARDING_KEY, "done");
    bar.remove();
  });
}

/**
 * First-run privacy modal (persisted in settings + localStorage fallback),
 * then optional tip strip.
 */
export function mountOnboarding(
  host: HTMLElement,
  options: OnboardingOptions,
): void {
  const localOk =
    options.privacyAccepted ||
    (() => {
      try {
        return localStorage.getItem(PRIVACY_KEY) === "1";
      } catch {
        return false;
      }
    })();

  if (!localOk) {
    mountPrivacyModal(async () => {
      await options.onAcceptPrivacy();
      mountTips(host);
    });
    return;
  }

  // If settings were reset but the local fallback still records consent,
  // synchronize it back to the backend so its privacy gate can open.
  if (!options.privacyAccepted) {
    void Promise.resolve(options.onAcceptPrivacy()).catch(() => {
      /* preview or temporarily unavailable backend */
    });
  }
  mountTips(host);
}
