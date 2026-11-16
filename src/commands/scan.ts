import fs from "node:fs/promises";
import { defineCommand } from "citty";
import pc from "picocolors";
import Ajv2020Module, { type ErrorObject } from "ajv/dist/2020.js";
import addFormatsImport from "ajv-formats";
import { scanAgents, SCAN_RESULT_SCHEMA_PATH, type ScanResult } from "../native.js";

// Handle CJS/ESM default interop cleanly under NodeNext
const Ajv2020Ctor = (
  (Ajv2020Module as unknown as { Ajv2020?: typeof Ajv2020Module.Ajv2020 }).Ajv2020 ??
  (Ajv2020Module as unknown as { default?: typeof Ajv2020Module.Ajv2020 }).default ??
  Ajv2020Module
) as typeof Ajv2020Module.Ajv2020;

const addFormats = (
  (addFormatsImport as unknown as { default?: (ajv: unknown) => void }).default ??
  addFormatsImport
) as unknown as (ajv: unknown) => void;

export async function validateScanResultSchema(
  result: ScanResult,
  schemaPath: string = SCAN_RESULT_SCHEMA_PATH
): Promise<{ valid: boolean; errors: string[] }> {
  const rawSchema = await fs.readFile(schemaPath, "utf8");
  const schema = JSON.parse(rawSchema);
  const ajv = new Ajv2020Ctor({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const valid = validate(result) as boolean;
  if (valid) {
    return { valid: true, errors: [] };
  }
  const errors = (validate.errors ?? []).map(
    (err: ErrorObject) => `${err.instancePath || "/"} ${err.message ?? "schema error"}`
  );
  return { valid: false, errors };
}

export const scanCommand = defineCommand({
  meta: {
    name: "scan",
    description: "Scan local machine against known agents in agent-tools.json",
  },
  args: {
    json: {
      type: "boolean",
      description: "Print full scan result JSON to stdout",
      default: false,
    },
    output: {
      type: "string",
      alias: "o",
      description: "Write scan result JSON to file (e.g. agent-tools-scan-result.json)",
    },
    registry: {
      type: "string",
      alias: "r",
      description: "Optional path to a custom agent-tools.json registry",
    },
    validate: {
      type: "boolean",
      description: "Validate output against agent-tools-scan-result-schema.json",
      default: true,
    },
  },
  async run({ args }) {
    const result = await scanAgents(args.registry, args.output);

    if (args.validate) {
      const check = await validateScanResultSchema(result);
      if (!check.valid) {
        console.error(pc.red("Schema validation failed:"));
        for (const err of check.errors) {
          console.error(pc.red(`  - ${err}`));
        }
        process.exitCode = 1;
        return;
      }
    }

    if (args.json) {
      console.log(JSON.stringify(result, null, 2));
      return;
    }

    console.log(pc.bold(pc.cyan("Agent Locator Scan Summary")));
    console.log(
      `Host: ${result.host.hostname} (${result.host.os}) | Home: ${result.host.home_dir}`
    );
    console.log(
      `Checked: ${result.summary.total_checked} | ` +
        `${pc.green(`Installed: ${result.summary.installed_count}`)} | ` +
        `${pc.dim(`Not installed: ${result.summary.not_installed_count}`)} | ` +
        `Duration: ${result.scan_duration ?? "N/A"}`
    );

    if (args.output) {
      console.log(pc.green(`Saved & schema-validated: ${args.output}`));
    }

    if (result.installed_agents.length === 0) {
      console.log(pc.yellow("\nNo known agents detected on this machine."));
      return;
    }

    console.log("");
    for (const agent of result.installed_agents) {
      const totalSkills = new Set(agent.skills_dirs.flatMap((d) => d.skills)).size;
      const hitPaths = agent.detect_hits
        .filter((h) => h.exists)
        .map((h) => h.configured_path)
        .join(", ");
      console.log(
        `${pc.green("●")} ${pc.bold(agent.display_name)} ${pc.dim(`(${agent.key})`)} — ` +
          `${pc.cyan(`${totalSkills} skills`)} ${hitPaths ? pc.dim(`[${hitPaths}]`) : ""}`
      );
      for (const sdir of agent.skills_dirs) {
        const statusLabel = sdir.exists
          ? sdir.kind === "symlink"
            ? pc.magenta(`symlink -> ${sdir.symlink_target}`)
            : pc.green(`${sdir.skills_count} skills`)
          : pc.dim("missing");
        console.log(`    ${sdir.configured_path}: ${statusLabel}`);
      }
    }
  },
});
