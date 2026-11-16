import { defineCommand } from "citty";
import pc from "picocolors";
import { getSkillsMatrix, scanAgents } from "../native.js";

const listCommand = defineCommand({
  meta: {
    name: "list",
    description: "List installed (or all known) AI coding agents",
  },
  args: {
    all: {
      type: "boolean",
      alias: "a",
      description: "Show all 87 known agents including non-installed ones",
      default: false,
    },
    category: {
      type: "string",
      alias: "c",
      description: "Filter by category (coding, lobster, cli, ide, unknown, etc.)",
    },
    json: {
      type: "boolean",
      description: "Output as JSON",
      default: false,
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    const matrix = await getSkillsMatrix(args.registry);
    const installedMap = new Map(matrix.installedAgents.map((a) => [a.key, a]));

    let rows = matrix.allAgents.map((def) => {
      const installed = installedMap.get(def.key);
      return {
        key: def.key,
        displayName: def.displayName,
        category: def.category,
        installed: Boolean(installed),
        skillsCount: installed?.skillsCount ?? 0,
        primarySkillsDir: installed?.primarySkillsDir ?? def.globalSkillsDirs[0] ?? null,
        detectPaths: def.detectPaths,
      };
    });

    if (!args.all) {
      rows = rows.filter((r) => r.installed);
    }
    if (args.category) {
      const cat = args.category.toLowerCase();
      rows = rows.filter((r) => r.category.toLowerCase() === cat);
    }

    if (args.json) {
      console.log(JSON.stringify(rows, null, 2));
      return;
    }

    if (rows.length === 0) {
      console.log(pc.yellow("No matching agents found."));
      return;
    }

    console.log(
      pc.bold(
        `Agents (${rows.length}${args.all ? " total" : " installed"}):`
      )
    );
    for (const row of rows) {
      const badge = row.installed ? pc.green("[installed]") : pc.dim("[missing]  ");
      const skillsText = row.installed ? pc.cyan(`${row.skillsCount} skills`) : pc.dim("-");
      console.log(
        `  ${badge} ${pc.bold(row.displayName)} ${pc.dim(`(${row.key})`)} ` +
          `${pc.dim(`[${row.category}]`)} ${skillsText} ` +
          `${row.primarySkillsDir ? pc.dim(`-> ${row.primarySkillsDir}`) : ""}`
      );
    }
  },
});

const infoCommand = defineCommand({
  meta: {
    name: "info",
    description: "Inspect detailed paths, symlinks, and skills for a specific agent",
  },
  args: {
    key: {
      type: "positional",
      description: "Agent key (e.g. claude_code, cursor, codex)",
      required: true,
    },
    json: {
      type: "boolean",
      description: "Output as JSON",
      default: false,
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    const [scan, matrix] = await Promise.all([
      scanAgents(args.registry),
      getSkillsMatrix(args.registry),
    ]);

    const def = matrix.allAgents.find((a) => a.key === args.key);
    if (!def) {
      console.error(pc.red(`Unknown agent key '${args.key}'.`));
      process.exitCode = 1;
      return;
    }

    const installed = scan.installed_agents.find((a) => a.key === args.key);
    const payload = {
      definition: def,
      installed: Boolean(installed),
      scanDetails: installed ?? null,
    };

    if (args.json) {
      console.log(JSON.stringify(payload, null, 2));
      return;
    }

    console.log(
      `${pc.bold(pc.cyan(def.displayName))} ${pc.dim(`(${def.key})`)} ` +
        (installed ? pc.green("● installed") : pc.dim("○ not installed"))
    );
    console.log(`Category: ${def.category} | Recursive scan: ${def.recursiveScan}`);
    console.log(`Sources: ${def.sources.join(", ")}`);
    console.log(`Project skills dir: ${def.projectSkillsDir ?? "N/A"}`);

    console.log(pc.bold("\nDetect Paths:"));
    if (installed) {
      for (const hit of installed.detect_hits) {
        const state = hit.exists ? pc.green(hit.kind) : pc.dim("missing");
        const link = hit.symlink_target ? pc.magenta(` -> ${hit.symlink_target}`) : "";
        console.log(`  - ${hit.configured_path} [${state}] (${hit.resolved_path})${link}`);
      }
    } else {
      for (const p of def.detectPaths) {
        console.log(`  - ${p} ${pc.dim("[missing]")}`);
      }
    }

    console.log(pc.bold("\nGlobal & Additional Skills Dirs:"));
    if (installed) {
      for (const dir of installed.skills_dirs) {
        const state = dir.exists ? pc.green(dir.kind) : pc.dim("missing");
        const link = dir.symlink_target ? pc.magenta(` -> ${dir.symlink_target}`) : "";
        console.log(
          `  - ${dir.configured_path} [${state}] (${dir.skills_count} skills)${link}`
        );
        for (const s of dir.skills) {
          console.log(`      • ${s}`);
        }
      }
    } else {
      for (const p of [...def.globalSkillsDirs, ...def.additionalScanDirs]) {
        console.log(`  - ${p} ${pc.dim("[not installed]")}`);
      }
    }
  },
});

export const agentsCommand = defineCommand({
  meta: {
    name: "agents",
    description: "Inspect detected and known AI coding agents",
  },
  subCommands: {
    list: listCommand,
    info: infoCommand,
  },
});
