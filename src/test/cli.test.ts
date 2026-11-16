import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { promisify } from "node:util";
import React from "react";
import { render } from "ink";
import {
  addSkill,
  distributeSkill,
  getSkillsMatrix,
  importSkill,
  PACKAGE_ROOT,
  runDoctor,
  scanAgents,
  syncAllSkills,
  undistributeSkill,
} from "../native.js";
import { validateScanResultSchema } from "../commands/scan.js";
import { App } from "../tui/App.js";

const execFileAsync = promisify(execFile);

async function withTempHome(fn: (homeDir: string) => Promise<void>) {
  const tmpHome = await fs.mkdtemp(path.join(os.tmpdir(), "agloc-test-"));
  const prevHome = process.env.AGENT_LOCATOR_HOME;
  process.env.AGENT_LOCATOR_HOME = tmpHome;
  try {
    await fn(tmpHome);
  } finally {
    if (prevHome === undefined) {
      delete process.env.AGENT_LOCATOR_HOME;
    } else {
      process.env.AGENT_LOCATOR_HOME = prevHome;
    }
    await fs.rm(tmpHome, { recursive: true, force: true });
  }
}

async function createSampleSkill(dir: string, name: string, desc: string, body = "Hello") {
  await fs.mkdir(dir, { recursive: true });
  await fs.writeFile(
    path.join(dir, "SKILL.md"),
    `---\nname: ${name}\ndescription: ${desc}\n---\n\n# ${name}\n${body}\n`,
    "utf8"
  );
}

test("scanAgents produces output strictly conforming to agent-tools-scan-result-schema.json", async () => {
  await withTempHome(async (homeDir) => {
    await createSampleSkill(
      path.join(homeDir, ".claude", "skills", "alpha-skill"),
      "Alpha",
      "Alpha skill"
    );
    await createSampleSkill(
      path.join(homeDir, ".agents", "skills", "shared-only"),
      "Shared",
      "Shared dir skill"
    );

    const outPath = path.join(homeDir, "scan-out.json");
    const result = await scanAgents(undefined, outPath);

    const check = await validateScanResultSchema(result);
    assert.equal(check.valid, true, `Schema errors: ${check.errors.join("; ")}`);

    const rawFile = JSON.parse(await fs.readFile(outPath, "utf8"));
    const fileCheck = await validateScanResultSchema(rawFile);
    assert.equal(fileCheck.valid, true, `File schema errors: ${fileCheck.errors.join("; ")}`);

    assert.equal(result.summary.total_checked, 87);
    assert.equal(result.summary.unlisted_local_count, 0);
    assert.deepEqual(result.unlisted_local_dirs, []);

    const installedKeys = result.installed_agents.map((a) => a.key);
    assert.ok(installedKeys.includes("claude_code"));
    assert.ok(!installedKeys.includes("warp"));
    assert.ok(!installedKeys.includes("cline"));
    assert.ok(!installedKeys.includes("zed"));
  });
});

test("skills lifecycle supports add, symlink, copy, drift detection, sync, import, and remove", async () => {
  await withTempHome(async (homeDir) => {
    await fs.mkdir(path.join(homeDir, ".claude"), { recursive: true });
    await fs.mkdir(path.join(homeDir, ".cursor"), { recursive: true });

    // 1. Add skill from external folder
    const extSkill = path.join(homeDir, "external", "my-skill");
    await createSampleSkill(extSkill, "My Skill", "Helps with tasks", "v1");

    const added = await addSkill(extSkill);
    assert.equal(added.id, "my-skill");
    assert.equal(added.inCentralHub, true);
    assert.equal(added.name, "My Skill");

    // 2. Distribute symlink to claude_code, copy to cursor
    await distributeSkill("my-skill", ["claude_code"], "symlink");
    await distributeSkill("my-skill", ["cursor"], "copy");

    const claudeStat = await fs.lstat(path.join(homeDir, ".claude", "skills", "my-skill"));
    assert.equal(claudeStat.isSymbolicLink(), true);

    const cursorStat = await fs.lstat(path.join(homeDir, ".cursor", "skills", "my-skill"));
    assert.equal(cursorStat.isSymbolicLink(), false);
    assert.equal(cursorStat.isDirectory(), true);

    // 3. Mutate cursor copy -> triggers drift
    await fs.writeFile(
      path.join(homeDir, ".cursor", "skills", "my-skill", "SKILL.md"),
      "---\nname: My Skill\ndescription: Edited locally\n---\n",
      "utf8"
    );

    const doctorBefore = await runDoctor({ fix: false });
    assert.equal(doctorBefore.healthy, false);
    assert.ok(doctorBefore.issues.some((i) => i.issueType === "drifted_copy"));

    // 4. Sync repairs drifted copy
    const syncRes = await syncAllSkills();
    assert.equal(syncRes.repairedCount, 1);

    const doctorAfter = await runDoctor({ fix: false });
    assert.equal(doctorAfter.healthy, true);

    // 5. Import an unmanaged skill from claude_code
    const legacyDir = path.join(homeDir, ".claude", "skills", "wild-skill");
    await createSampleSkill(legacyDir, "Wild Skill", "Discovered in claude");

    const imported = await importSkill("claude_code", "wild-skill", "symlink");
    assert.equal(imported.inCentralHub, true);
    assert.equal(imported.placements[0]?.mode, "symlink");
    assert.equal(imported.placements[0]?.status, "synced");

    // 6. Undistribute removes from cursor without deleting central hub
    await undistributeSkill("my-skill", ["cursor"]);
    await assert.rejects(fs.lstat(path.join(homeDir, ".cursor", "skills", "my-skill")));
    const centralStat = await fs.stat(
      path.join(homeDir, ".agent-locator", "skills", "my-skill", "SKILL.md")
    );
    assert.equal(centralStat.isFile(), true);
  });
});

test("CLI subcommands work end-to-end", async () => {
  await withTempHome(async (homeDir) => {
    await createSampleSkill(
      path.join(homeDir, ".codex", "skills", "cli-skill"),
      "CLI Skill",
      "CLI test skill"
    );

    const cliEntry = path.join(PACKAGE_ROOT, "dist", "index.js");
    const env = { ...process.env, AGENT_LOCATOR_HOME: homeDir };

    const scanOut = path.join(homeDir, "cli-scan.json");
    await execFileAsync(process.execPath, [cliEntry, "scan", "--output", scanOut], { env });
    const parsedScan = JSON.parse(await fs.readFile(scanOut, "utf8"));
    const check = await validateScanResultSchema(parsedScan);
    assert.equal(check.valid, true);

    const { stdout: agentsStdout } = await execFileAsync(
      process.execPath,
      [cliEntry, "agents", "list", "--json"],
      { env }
    );
    const agentsList = JSON.parse(agentsStdout);
    assert.ok(agentsList.some((a: { key: string }) => a.key === "codex"));

    const { stdout: skillsStdout } = await execFileAsync(
      process.execPath,
      [cliEntry, "skills", "list", "--json"],
      { env }
    );
    const skillsData = JSON.parse(skillsStdout);
    assert.ok(skillsData.skills.some((s: { id: string }) => s.id === "cli-skill"));
  });
});

test("Ink TUI renders dashboard tabs cleanly", async () => {
  await withTempHome(async (homeDir) => {
    await createSampleSkill(
      path.join(homeDir, ".claude", "skills", "tui-skill"),
      "TUI Skill",
      "Rendered in Ink"
    );
    const [scan, matrix, doctor] = await Promise.all([
      scanAgents(),
      getSkillsMatrix(),
      runDoctor({ fix: false }),
    ]);

    const chunks: string[] = [];
    const stdout = new PassThrough();
    Object.assign(stdout, { columns: 100, rows: 30, isTTY: false });
    stdout.on("data", (chunk) => chunks.push(chunk.toString("utf8")));

    const stdin = new PassThrough();
    Object.assign(stdin, { isTTY: false, setRawMode: () => {} });

    const instance = render(
      React.createElement(App, {
        initialData: { scan, matrix, doctor },
        disableExitOnQ: true,
      }),
      {
        stdout: stdout as unknown as NodeJS.WriteStream,
        stdin: stdin as unknown as NodeJS.ReadStream,
        debug: true,
      }
    );

    await new Promise((r) => setTimeout(r, 50));
    instance.unmount();

    const output = chunks.join("");
    assert.ok(output.includes("Agent Locator"));
    assert.ok(output.includes("Agents 概览"));
    assert.ok(output.includes("Skills 矩阵"));
    assert.ok(output.includes("Doctor 巡检"));
    assert.ok(output.includes("Claude Code"));
  });
});
