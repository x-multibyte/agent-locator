use napi_derive::napi;
use serde::{Deserialize, Serialize};

/// Static registry file structure (`agent-tools.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistry {
    pub schema_version: String,
    pub updated_at: String,
    pub description: Option<String>,
    pub sources: Vec<String>,
    pub total: u32,
    pub agents: Vec<AgentDef>,
}

/// Static definition of a known AI coding agent from `agent-tools.json`.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub key: String,
    pub display_name: String,
    pub category: String,
    pub detect_paths: Vec<String>,
    pub global_skills_dirs: Vec<String>,
    pub project_skills_dir: Option<String>,
    pub additional_scan_dirs: Vec<String>,
    pub recursive_scan: bool,
    pub sources: Vec<String>,
}

// ============================================================================
// Scan result structures matching `agent-tools-scan-result-schema.json`
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub hostname: String,
    pub os: String,
    pub home_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRef {
    pub path: String,
    pub updated_at: String,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_checked: u32,
    pub installed_count: u32,
    pub not_installed_count: u32,
    pub unlisted_local_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathEntry {
    pub configured_path: String,
    pub resolved_path: String,
    pub exists: bool,
    pub kind: String,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsDirEntry {
    pub configured_path: String,
    pub resolved_path: String,
    pub exists: bool,
    pub kind: String,
    pub symlink_target: Option<String>,
    pub skills_count: u32,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedAgent {
    pub key: String,
    pub display_name: String,
    pub detect_hits: Vec<PathEntry>,
    pub skills_dirs: Vec<SkillsDirEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlistedDir {
    pub path: String,
    pub resolved_path: String,
    pub kind: String,
    pub symlink_target: Option<String>,
    pub classification: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub schema_version: String,
    pub scanned_at: String,
    pub scan_duration: String,
    pub host: HostInfo,
    pub registry_ref: RegistryRef,
    pub summary: ScanSummary,
    pub installed_agents: Vec<ScannedAgent>,
    pub not_installed_keys: Vec<String>,
    pub unlisted_local_dirs: Vec<UnlistedDir>,
}

// ============================================================================
// Skills Management, Matrix, Sync & Doctor models exposed via napi-rs
// ============================================================================

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillPlacement {
    pub agent_key: String,
    pub agent_display_name: String,
    pub target_path: String,
    pub resolved_path: String,
    /// "symlink" | "copy" | "unmanaged" | "broken_symlink"
    pub mode: String,
    /// "synced" | "drifted" | "unmanaged" | "broken" | "missing_skill_md"
    pub status: String,
    pub symlink_target: Option<String>,
    pub content_hash: Option<String>,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub has_skill_md: bool,
    pub content_hash: String,
    pub in_central_hub: bool,
    pub central_path: Option<String>,
    pub placements: Vec<AgentSkillPlacement>,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub key: String,
    pub display_name: String,
    pub category: String,
    pub primary_skills_dir: Option<String>,
    pub skills_dirs: Vec<String>,
    pub detect_hits: Vec<String>,
    pub skills_count: u32,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMatrixReport {
    pub central_hub_dir: String,
    pub state_file_path: String,
    pub installed_agents: Vec<AgentSummary>,
    pub all_agents: Vec<AgentDef>,
    pub skills: Vec<SkillMetadata>,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorIssue {
    /// "broken_symlink" | "unmanaged_skill" | "drifted_copy" | "missing_skill_md"
    pub issue_type: String,
    /// "error" | "warning" | "info"
    pub severity: String,
    pub skill_id: String,
    pub agent_key: Option<String>,
    pub path: String,
    pub message: String,
    pub fixable: bool,
    pub fixed: bool,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub total_issues: u32,
    pub fixed_count: u32,
    pub issues: Vec<DoctorIssue>,
}

#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub synced_count: u32,
    pub repaired_count: u32,
    pub skipped_count: u32,
    pub details: Vec<String>,
}
