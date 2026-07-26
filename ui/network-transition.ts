import type { NetworkTransition } from "./globe";

export function shouldPresentTransition(
  transition: NetworkTransition | null | undefined,
  shownId: number,
): boolean {
  return !!transition && transition.ageSeconds <= 90 && transition.id >= shownId;
}

export function transitionCopy(transition: NetworkTransition): {
  title: string;
  detail: string;
} {
  const previous = exitPlace(transition.previousExit);
  const current = exitPlace(transition.currentExit);
  if (transition.status === "detecting") {
    return {
      title: "Your network route changed",
      detail: "Rechecking your public exit and refreshing active paths…",
    };
  }
  if (transition.status === "ready" && transition.currentExit) {
    const moved = transition.previousExit?.ip !== transition.currentExit.ip;
    return {
      title: moved ? `${previous} → ${current}` : `Exit still appears in ${current}`,
      detail: "Wi-Fi, VPN, or proxy state changed · active paths were refreshed",
    };
  }
  if (transition.status === "ready") {
    return {
      title: "Network changed · paths refreshed",
      detail: "Public-exit lookup is disabled, but active routes are being rechecked.",
    };
  }
  return {
    title: "Network changed · exit unavailable",
    detail: "Keeping the last useful paths while fresh route checks continue.",
  };
}

function exitPlace(exit: NetworkTransition["currentExit"]): string {
  if (!exit) return "an unknown exit";
  if (exit.city) return `${exit.city}${exit.country ? `, ${exit.country}` : ""}`;
  return exit.country || exit.organization || exit.ip || "an unknown exit";
}
