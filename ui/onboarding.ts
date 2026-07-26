const ONBOARDING_KEY = "network-cartographer.onboarding.v1";

const TIPS = [
  "This is a near-live view · connections and routes appear after a short delay",
  "Want to see the globe light up? Open websites hosted in a few different countries",
  "Drag to orbit the globe · scroll to zoom (globe only, not the page)",
  "Pink ★ markers are confirmed destinations · amber ◌ markers are partial routes",
  "Amber dashes bridge missing evidence · they move only while traceroute is measuring",
  "Click an app to focus its paths · Shift-click to multi-select",
  "Select a route or node to inspect hops, networks, and location confidence",
];

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

/** Show optional, non-blocking usage tips on first run. */
export function mountOnboarding(host: HTMLElement): void {
  mountTips(host);
}
