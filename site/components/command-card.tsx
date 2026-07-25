"use client";

import { useState } from "react";

const commands = {
  unix: "curl -fsSL https://mapmy.network/run | sh",
  windows: "irm https://mapmy.network/run.ps1 | iex",
} as const;

type Platform = keyof typeof commands;

export function CommandCard() {
  const [platform, setPlatform] = useState<Platform>("unix");
  const [copied, setCopied] = useState(false);

  async function copyCommand() {
    await navigator.clipboard.writeText(commands[platform]);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_600);
  }

  return (
    <div className="run-card" id="run">
      <div className="run-tabs" role="tablist" aria-label="Operating system">
        <button
          className={`run-tab${platform === "unix" ? " active" : ""}`}
          type="button"
          role="tab"
          aria-selected={platform === "unix"}
          onClick={() => setPlatform("unix")}
        >
          macOS / Linux
        </button>
        <button
          className={`run-tab${platform === "windows" ? " active" : ""}`}
          type="button"
          role="tab"
          aria-selected={platform === "windows"}
          onClick={() => setPlatform("windows")}
        >
          Windows
        </button>
        <span className="run-label">Run once</span>
      </div>
      <div className="command-row" role="tabpanel">
        <span className="prompt" aria-hidden="true">
          ›
        </span>
        <code>{commands[platform]}</code>
        <button
          className="copy-button"
          type="button"
          onClick={copyCommand}
          aria-label="Copy command"
        >
          <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <rect x="8" y="8" width="10" height="10" rx="2" />
            <path d="M15 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2" />
          </svg>
          <span aria-live="polite">{copied ? "Copied" : "Copy"}</span>
        </button>
      </div>
    </div>
  );
}
