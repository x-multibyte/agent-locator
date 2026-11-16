import { defineCommand } from "citty";
import pc from "picocolors";
import {
  addSkill,
  distributeSkill,
  getSkillsMatrix,
  importSkill,
  runDoctor,
  syncAllSkills,
  undistributeSkill,
  type AgentSummary,
} from "../native.js";

function parseMode(mode?: string): "symlink" | "copy" {
  const normalized = (mode ?? "symlink").trim().toLowerCase();
  if (normalized !== "symlink" && normalized !== "copy") {
    throw new Error(`Invalid mode '${mode}'. Expected 'symlink' or 'copy'.`);
  }
  return normalized;
}

function resolveTargetAgentKeys(
  rawTo: string,
  installedAgents: AgentSummary[],
  allAgentKeys: Set<string>
): string[] {
  const trimmed = rawTo.trim();
  if (trimmed === "all") {
    return installedAgents
      .filter((a) => a.skillsDirs.length > 0)
      .map((a) => a.key);
  }
  const keys = trimmed
    .split(",")
    .map((k) => k.trim())
    .filter(Boolean);
  for (const key of keys) {
    if (!allAgentKeys.has(key)) {
      throw new Error(`Unknown agent key '${key}'`);
    }
  }
  return keys;
}

const listCommand = defineCommand({
  meta: {
    name: "list",
    description: "List central-hub and agent-installed skills with sync status",
  },
  args: {
    agent: {
      type: "string",
      alias: "a",
      description: "Filter by agent key",
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
    let skills = matrix.skills;
    if (args.agent) {
      skills = skills.filter((s) =>
        s.placements.some((p) => p.agentKey === args.agent)
      );
    }

    if (args.json) {
      console.log(
        JSON.stringify(
          {
            centralHubDir: matrix.centralHubDir,
            skills,
          },
          null,
          2
        )
      );
      return;
    }

    console.log(pc.bold(pc.cyan(`Central Hub: ${matrix.centralHubDir}`)));
    if (skills.length === 0) {
      console.log(pc.yellow("No skills found in central hub or installed agents."));
      return;
    }

    for (const skill of skills) {
      const hubTag = skill.inCentralHub ? pc.green("[central]") : pc.yellow("[unmanaged]");
      const mdTag = skill.hasSkillMd ? "" : pc.red(" [no SKILL.md]");
      const displayName = skill.name ? pc.dim(` (${skill.name})`) : "";
      console.log(`\n${pc.bold(skill.id)}${displayName} ${hubTag}${mdTag}`);
      if (skill.description) {
        console.log(`  ${pc.dim(skill.description)}`);
      }
      if (skill.placements.length === 0) {
        console.log(`  ${pc.dim("Not distributed to any agent")}`);
      } else {
        for (const p of skill.placements) {
          const statusColor =
            p.status === "synced"
              ? pc.green
              : p.status === "drifted" || p.status === "broken"
              ? pc.red
              : pc.yellow;
          console.log(
            `  -> ${p.agentDisplayName} (${p.agentKey}): ` +
              `${pc.cyan(p.mode)} ${statusColor(`[${p.status}]`)} ${pc.dim(p.targetPath)}`
          );
        }
      }
    }
  },
});

const addCommand = defineCommand({
  meta: {
    name: "add",
    description: "Add a local skill folder to ~/.agent-locator/skills and optionally distribute it",
  },
  args: {
    path: {
      type: "positional",
      description: "Path to local skill directory",
      required: true,
    },
    id: {
      type: "string",
      description: "Override skill directory name in central hub",
    },
    to: {
      type: "string",
      description: "Comma-separated agent keys or 'all' to distribute immediately",
    },
    mode: {
      type: "string",
      alias: "m",
      description: "Distribution mode when --to is specified: symlink | copy",
      default: "symlink",
    },
    overwrite: {
      type: "boolean",
      description: "Overwrite if skill ID already exists in central hub with different content",
      default: false,
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    const mode = parseMode(args.mode);
    let skill = await addSkill(args.path, {
      skillId: args.id,
      overwrite: args.overwrite,
      registryPath: args.registry,
    });
    console.log(
      pc.green(`Added skill '${skill.id}' to central hub (${skill.centralPath ?? ""})`)
    );

    if (args.to) {
      const matrix = await getSkillsMatrix(args.registry);
      const targets = resolveTargetAgentKeys(
        args.to,
        matrix.installedAgents,
        new Set(matrix.allAgents.map((a) => a.key))
      );
      if (targets.length === 0) {
        console.log(pc.yellow("No installed target agents with global_skills_dirs found."));
        return;
      }
      skill = await distributeSkill(skill.id, targets, mode, args.registry);
      console.log(
        pc.green(`Distributed '${skill.id}' (${mode}) to: ${targets.join(", ")}`)
      );
    }
  },
});

const importCommand = defineCommand({
  meta: {
    name: "import",
    description: "Import existing unmanaged agent skills into ~/.agent-locator/skills",
  },
  args: {
    from: {
      type: "string",
      description: "Source agent key to import from",
    },
    skill: {
      type: "string",
      description: "Specific skill ID to import",
    },
    mode: {
      type: "string",
      alias: "m",
      description: "Placement mode to keep in agent dir: symlink | copy",
      default: "symlink",
    },
    all: {
      type: "boolean",
      alias: "a",
      description: "Import all unmanaged skills across installed agents (or within --from)",
      default: false,
    },
    overwrite: {
      type: "boolean",
      description: "Overwrite central hub skill on content conflict",
      default: false,
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    const mode = parseMode(args.mode);

    if (args.from && args.skill) {
      const imported = await importSkill(args.from, args.skill, mode, {
        overwrite: args.overwrite,
        registryPath: args.registry,
      });
      console.log(
        pc.green(`Imported '${imported.id}' from '${args.from}' using ${mode} mode.`)
      );
      return;
    }

    if (!args.all && !args.from) {
      console.error(
        pc.red("Specify --from <agent> [--skill <id>] or --all to import unmanaged skills.")
      );
      process.exitCode = 1;
      return;
    }

    const matrix = await getSkillsMatrix(args.registry);
    const candidates: Array<{ agentKey: string; skillId: string }> = [];

    for (const s of matrix.skills) {
      for (const p of s.placements) {
        if (p.status !== "unmanaged") continue;
        if (args.from && p.agentKey !== args.from) continue;
        if (args.skill && s.id !== args.skill) continue;
        candidates.push({ agentKey: p.agentKey, skillId: s.id });
      }
    }

    if (candidates.length === 0) {
      console.log(pc.yellow("No unmanaged skills found matching the criteria."));
      return;
    }

    let importedCount = 0;
    for (const item of candidates) {
      try {
        await importSkill(item.agentKey, item.skillId, mode, {
          overwrite: args.overwrite,
          registryPath: args.registry,
        });
        console.log(
          pc.green(`Imported '${item.skillId}' from '${item.agentKey}' (${mode})`)
        );
        importedCount++;
      } catch (err) {
        console.error(
          pc.red(
            `Failed importing '${item.skillId}' from '${item.agentKey}': ${(err as Error).message}`
          )
        );
      }
    }
    console.log(pc.bold(`Done: imported ${importedCount}/${candidates.length} skill(s).`));
  },
});

function makeDistributeCommand(mode: "symlink" | "copy") {
  const name = mode === "symlink" ? "link" : "copy";
  return defineCommand({
    meta: {
      name,
      description: `Distribute a central-hub skill to agents using ${mode} mode`,
    },
    args: {
      skill: {
        type: "positional",
        description: "Skill ID in ~/.agent-locator/skills",
        required: true,
      },
      to: {
        type: "string",
        description: "Comma-separated agent keys or 'all'",
        required: true,
      },
      registry: {
        type: "string",
        alias: "r",
        description: "Optional path to agent-tools.json",
      },
    },
    async run({ args }) {
      const matrix = await getSkillsMatrix(args.registry);
      const targets = resolveTargetAgentKeys(
        args.to,
        matrix.installedAgents,
        new Set(matrix.allAgents.map((a) => a.key))
      );
      if (targets.length === 0) {
        console.log(pc.yellow("No target agents resolved."));
        return;
      }
      await distributeSkill(args.skill, targets, mode, args.registry);
      console.log(
        pc.green(`Distributed '${args.skill}' via ${mode} to: ${targets.join(", ")}`)
      );
    },
  });
}

const removeCommand = defineCommand({
  meta: {
    name: "remove",
    description: "Remove a distributed skill from target agent(s) without deleting central hub",
  },
  args: {
    skill: {
      type: "positional",
      description: "Skill ID",
      required: true,
    },
    from: {
      type: "string",
      description: "Comma-separated agent keys or 'all'",
      required: true,
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to agent-tools.json",
    },
  },
  async run({ args }) {
    const matrix = await getSkillsMatrix(args.registry);
    const targets = resolveTargetAgentKeys(
      args.from,
      matrix.installedAgents,
      new Set(matrix.allAgents.map((a) => a.key))
    );
    if (targets.length === 0) {
      console.log(pc.yellow("No target agents resolved."));
      return;
    }
    await undistributeSkill(args.skill, targets, args.registry);
    console.log(pc.green(`Removed '${args.skill}' from: ${targets.join(", ")}`));
  },
});

const syncCommand = defineCommand({
  meta: {
    name: "sync",
    description: "Synchronize all managed symlink and copy skill placements",
  },
  async run() {
    const report = await syncAllSkills();
    console.log(
      pc.bold(
        `Sync complete — synced: ${pc.green(String(report.syncedCount))}, ` +
          `repaired: ${pc.cyan(String(report.repairedCount))}, ` +
          `skipped: ${pc.yellow(String(report.skippedCount))}`
      )
    );
    for (const line of report.details) {
      console.log(`  ${line}`);
    }
  },
});

const doctorCommand = defineCommand({
  meta: {
    name: "doctor",
    description: "Check for broken symlinks, drifted copies, unmanaged skills, and missing SKILL.md",
  },
  args: {
    fix: {
      type: "boolean",
      description: "Automatically fix repairable issues (broken links, drifted copies, unmanaged skills)",
      default: false,
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
    const report = await runDoctor({
      registryPath: args.registry,
      fix: args.fix,
    });

    if (args.json) {
      console.log(JSON.stringify(report, null, 2));
      return;
    }

    if (report.totalIssues === 0) {
      console.log(pc.green("✓ All checks passed. No skill issues found."));
      return;
    }

    console.log(
      pc.bold(
        `Doctor found ${report.totalIssues} issue(s)` +
          (args.fix ? ` (fixed: ${report.fixedCount})` : "") +
          ":"
      )
    );

    for (const issue of report.issues) {
      const icon = issue.fixed
        ? pc.green("[FIXED]")
        : issue.severity === "error"
        ? pc.red("[ERROR]")
        : issue.severity === "warning"
        ? pc.yellow("[WARN]")
        : pc.cyan("[INFO]");
      console.log(`  ${icon} (${issue.issueType}) ${issue.message}`);
    }
  },
});

export const skillsCommand = defineCommand({
  meta: {
    name: "skills",
    description: "Manage skills in central hub (~/.agent-locator/skills) with symlink & copy modes",
  },
  subCommands: {
    list: listCommand,
    add: addCommand,
    import: importCommand,
    link: makeDistributeCommand("symlink"),
    copy: makeDistributeCommand("copy"),
    remove: removeCommand,
    sync: syncCommand,
    doctor: doctorCommand,
  },
});
