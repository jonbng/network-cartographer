"use client";

import { useState } from "react";

const commands = {
  unix: "curl -fsSL https://mapmy.network/run | sh",
  windows: "irm https://mapmy.network/run.ps1 | iex",
} as const;

type Platform = keyof typeof commands;

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "unix";

  const platform = navigator.platform ?? "";
  const ua = navigator.userAgent ?? "";
  if (/win/i.test(platform) || /windows/i.test(ua)) return "windows";
  return "unix";
}

export function CommandCard() {
  const [platform, setPlatform] = useState<Platform>(detectPlatform);
  const [copied, setCopied] = useState(false);

  async function copyCommand() {
    await navigator.clipboard.writeText(commands[platform]);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_600);
  }

  return (
    <div className="run-block" id="run">
      <p className="run-heading">Run this command to launch the live map</p>

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
        Paste it into your terminal. The app runs locally and opens in your browser.
      </p>
    </div>
  );
}
