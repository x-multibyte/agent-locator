import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const PACKAGE_ROOT = path.resolve(__dirname, "..");
export const DEFAULT_REGISTRY_PATH = path.join(PACKAGE_ROOT, "agent-tools.json");
export const SCAN_RESULT_SCHEMA_PATH = path.join(
  PACKAGE_ROOT,
  "agent-tools-scan-result-schema.json"
);

export interface PathEntry {
  configured_path: string;
  resolved_path: string;
  exists: boolean;
  kind: "dir" | "file" | "symlink" | "missing";
  symlink_target: string | null;
}

export interface SkillsDirEntry extends PathEntry {
  skills_count: number;
  skills: string[];
}

export interface ScannedAgent {
  key: string;
  display_name: string;
  detect_hits: PathEntry[];
  skills_dirs: SkillsDirEntry[];
}

export interface ScanResult {
  schema_version: string;
  scanned_at: string;
  scan_duration?: string;
  host: {
    hostname: string;
    os: string;
    home_dir: string;
  };
  registry_ref: {
    path: string;
    updated_at: string;
    total: number;
  };
  summary: {
    total_checked: number;
    installed_count: number;
    not_installed_count: number;
    unlisted_local_count: number;
  };
  installed_agents: ScannedAgent[];
  not_installed_keys: string[];
  unlisted_local_dirs: Array<{
    path: string;
    resolved_path: string;
    kind: "dir" | "symlink";
    symlink_target: string | null;
    classification: string;
    notes: string;
  }>;
}

export interface AgentDef {
  key: string;
  displayName: string;
  category: string;
  detectPaths: string[];
  globalSkillsDirs: string[];
  projectSkillsDir?: string | null;
  additionalScanDirs: string[];
  recursiveScan: boolean;
  sources: string[];
}

export interface AgentSkillPlacement {
  agentKey: string;
  agentDisplayName: string;
  targetPath: string;
  resolvedPath: string;
  mode: "symlink" | "copy" | "unmanaged" | "broken_symlink";
  status: "synced" | "drifted" | "unmanaged" | "broken" | "missing_skill_md";
  symlinkTarget?: string | null;
  contentHash?: string | null;
}

export interface SkillMetadata {
  id: string;
  name?: string | null;
  description?: string | null;
  hasSkillMd: boolean;
  contentHash: string;
  inCentralHub: boolean;
  centralPath?: string | null;
  placements: AgentSkillPlacement[];
}

export interface AgentSummary {
  key: string;
  displayName: string;
  category: string;
  primarySkillsDir?: string | null;
  skillsDirs: string[];
  detectHits: string[];
  skillsCount: number;
}

export interface SkillsMatrixReport {
  centralHubDir: string;
  stateFilePath: string;
  installedAgents: AgentSummary[];
  allAgents: AgentDef[];
  skills: SkillMetadata[];
}

export interface DoctorIssue {
  issueType: "broken_symlink" | "unmanaged_skill" | "drifted_copy" | "missing_skill_md";
  severity: "error" | "warning" | "info";
  skillId: string;
  agentKey?: string | null;
  path: string;
  message: string;
  fixable: boolean;
  fixed: boolean;
}

export interface DoctorReport {
  healthy: boolean;
  totalIssues: number;
  fixedCount: number;
  issues: DoctorIssue[];
}

export interface SyncReport {
  syncedCount: number;
  repairedCount: number;
  skippedCount: number;
  details: string[];
}

interface NativeBindings {
  scanAgents(registryPath?: string | null, outputPath?: string | null): Promise<ScanResult>;
  getSkillsMatrix(registryPath?: string | null): Promise<SkillsMatrixReport>;
  addSkill(
    sourcePath: string,
    skillId?: string | null,
    overwrite?: boolean | null,
    registryPath?: string | null
  ): Promise<SkillMetadata>;
  importSkill(
    agentKey: string,
    skillId: string,
    mode: string,
    overwrite?: boolean | null,
    registryPath?: string | null
  ): Promise<SkillMetadata>;
  distributeSkill(
    skillId: string,
    agentKeys: string[],
    mode: string,
    registryPath?: string | null
  ): Promise<SkillMetadata>;
  undistributeSkill(
    skillId: string,
    agentKeys: string[],
    registryPath?: string | null
  ): Promise<SkillMetadata>;
  syncAllSkills(): Promise<SyncReport>;
  runDoctor(registryPath?: string | null, fix?: boolean | null): Promise<DoctorReport>;
}

const require = createRequire(import.meta.url);
const addonPath = path.join(PACKAGE_ROOT, "agent-locator.node");
const native = require(addonPath) as NativeBindings;

function resolveRegistry(registryPath?: string): string | undefined {
  if (registryPath && registryPath.trim().length > 0) {
    return registryPath;
  }
  if (process.env.AGENT_LOCATOR_REGISTRY && process.env.AGENT_LOCATOR_REGISTRY.trim().length > 0) {
    return process.env.AGENT_LOCATOR_REGISTRY;
  }
  return DEFAULT_REGISTRY_PATH;
}

export async function scanAgents(
  registryPath?: string,
  outputPath?: string
): Promise<ScanResult> {
  return native.scanAgents(resolveRegistry(registryPath), outputPath ?? null);
}

export async function getSkillsMatrix(registryPath?: string): Promise<SkillsMatrixReport> {
  return native.getSkillsMatrix(resolveRegistry(registryPath));
}

export async function addSkill(
  sourcePath: string,
  options?: { skillId?: string; overwrite?: boolean; registryPath?: string }
): Promise<SkillMetadata> {
  return native.addSkill(
    sourcePath,
    options?.skillId ?? null,
    options?.overwrite ?? false,
    resolveRegistry(options?.registryPath)
  );
}

export async function importSkill(
  agentKey: string,
  skillId: string,
  mode: "symlink" | "copy" = "symlink",
  options?: { overwrite?: boolean; registryPath?: string }
): Promise<SkillMetadata> {
  return native.importSkill(
    agentKey,
    skillId,
    mode,
    options?.overwrite ?? false,
    resolveRegistry(options?.registryPath)
  );
}

export async function distributeSkill(
  skillId: string,
  agentKeys: string[],
  mode: "symlink" | "copy" = "symlink",
  registryPath?: string
): Promise<SkillMetadata> {
  return native.distributeSkill(skillId, agentKeys, mode, resolveRegistry(registryPath));
}

export async function undistributeSkill(
  skillId: string,
  agentKeys: string[],
  registryPath?: string
): Promise<SkillMetadata> {
  return native.undistributeSkill(skillId, agentKeys, resolveRegistry(registryPath));
}

export async function syncAllSkills(): Promise<SyncReport> {
  return native.syncAllSkills();
}

export async function runDoctor(
  options?: { registryPath?: string; fix?: boolean }
): Promise<DoctorReport> {
  return native.runDoctor(resolveRegistry(options?.registryPath), options?.fix ?? false);
}
