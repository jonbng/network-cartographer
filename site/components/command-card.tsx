"use client";

import { useState } from "react";

const commands = {
  unix: "curl -fsSL https://mapmy.network/run | sh",
  windows: "irm https://mapmy.network/run.ps1 | iex",
} as const;

const GITHUB_URL = "https://github.com/jonbng/network-cartographer";

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
    <div className="run-block" id="run">
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
      </div>

      <div className="command-row" role="tabpanel">
        <span className="prompt" aria-hidden="true">
          $
        </span>
        <code>{commands[platform]}</code>
        <button
          className={`copy-button${copied ? " copied" : ""}`}
          type="button"
          onClick={copyCommand}
          aria-label="Copy command"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>

      <p className="run-note">
        Downloads a checksummed binary, runs locally, exits when you quit. No account.
      </p>

      <a
        className="star-button"
        href={GITHUB_URL}
        target="_blank"
        rel="noopener noreferrer"
      >
        Star on GitHub
      </a>
    </div>
  );
}
