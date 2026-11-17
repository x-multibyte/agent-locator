import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, "..");

const targetTriple = process.argv[2]?.trim() || "";
const outputFileName =
  process.argv[3]?.trim() || `agent-locator.${process.platform}-${process.arch}.node`;

const candidateDirs = [
  ...(targetTriple ? [path.join(ROOT, "target", targetTriple, "release")] : []),
  path.join(ROOT, "target", "release"),
];

function resolveExpectedLibName(triple, platform) {
  if (triple.includes("apple-darwin") || (!triple && platform === "darwin")) {
    return "libagent_locator_native.dylib";
  }
  if (triple.includes("windows") || (!triple && platform === "win32")) {
    return "agent_locator_native.dll";
  }
  return "libagent_locator_native.so";
}

const expectedName = resolveExpectedLibName(targetTriple, process.platform);

let sourceFile = "";
for (const dir of candidateDirs) {
  const full = path.join(dir, expectedName);
  if (fs.existsSync(full)) {
    sourceFile = full;
    break;
  }
}

if (!sourceFile) {
  console.error(
    `Could not find '${expectedName}' in:\n${candidateDirs.map((d) => `  - ${d}`).join("\n")}`
  );
  process.exit(1);
}

const targets = new Set([
  path.join(ROOT, "agent-locator.node"),
  path.join(ROOT, outputFileName),
]);

for (const dest of targets) {
  fs.copyFileSync(sourceFile, dest);
  console.log(`Copied ${path.relative(ROOT, sourceFile)} -> ${path.relative(ROOT, dest)}`);
}
