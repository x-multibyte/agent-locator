#!/usr/bin/env node
import React from "react";
import { defineCommand, runMain } from "citty";
import { render } from "ink";
import { scanCommand } from "./commands/scan.js";
import { agentsCommand } from "./commands/agents.js";
import { skillsCommand } from "./commands/skills.js";
import { App } from "./tui/App.js";

function launchTui(registryPath?: string) {
  const instance = render(React.createElement(App, { registryPath }));
  return instance.waitUntilExit();
}

const tuiCommand = defineCommand({
  meta: {
    name: "tui",
    description: "Launch interactive Ink TUI dashboard",
  },
  args: {
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    await launchTui(args.registry);
  },
});

const mainCommand = defineCommand({
  meta: {
    name: "agent-locator",
    version: "0.1.0",
    description: "Detect installed AI coding agents and manage skills (symlink & copy)",
  },
  subCommands: {
    tui: tuiCommand,
    scan: scanCommand,
    agents: agentsCommand,
    skills: skillsCommand,
  },
});

async function main() {
  const rawArgs = process.argv.slice(2);
  if (rawArgs.length === 0) {
    await launchTui();
    return;
  }
  await runMain(mainCommand);
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
