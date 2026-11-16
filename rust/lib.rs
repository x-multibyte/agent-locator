pub mod models;
pub mod scanner;
pub mod skills;
pub mod state;

use models::{DoctorReport, SkillMetadata, SkillsMatrixReport, SyncReport};
use napi::bindgen_prelude::*;
use napi_derive::napi;

fn to_napi_err(msg: String) -> Error {
    Error::from_reason(msg)
}

/// Scans all known agents in `agent-tools.json` asynchronously and returns a JSON object
/// strictly matching `agent-tools-scan-result-schema.json`.
#[napi]
pub async fn scan_agents(
    registry_path: Option<String>,
    output_path: Option<String>,
) -> Result<serde_json::Value> {
    tokio::task::spawn_blocking(move || {
        let scan = scanner::scan_all_agents(registry_path.as_deref(), output_path.as_deref())
            .map_err(to_napi_err)?;
        serde_json::to_value(scan).map_err(|e| to_napi_err(e.to_string()))
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Returns the aggregated matrix of installed agents, central hub skills, and placements.
#[napi]
pub async fn get_skills_matrix(registry_path: Option<String>) -> Result<SkillsMatrixReport> {
    tokio::task::spawn_blocking(move || {
        skills::get_skills_matrix(registry_path.as_deref()).map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Adds a local skill folder into the isolated central hub (`~/.agent-locator/skills`).
#[napi]
pub async fn add_skill(
    source_path: String,
    skill_id: Option<String>,
    overwrite: Option<bool>,
    registry_path: Option<String>,
) -> Result<SkillMetadata> {
    tokio::task::spawn_blocking(move || {
        skills::add_skill(
            &source_path,
            skill_id.as_deref(),
            overwrite,
            registry_path.as_deref(),
        )
        .map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Imports an existing unmanaged skill from an installed agent into `~/.agent-locator/skills`
/// and replaces the agent's directory with a symlink or tracked copy.
#[napi]
pub async fn import_skill(
    agent_key: String,
    skill_id: String,
    mode: String,
    overwrite: Option<bool>,
    registry_path: Option<String>,
) -> Result<SkillMetadata> {
    tokio::task::spawn_blocking(move || {
        skills::import_skill(
            &agent_key,
            &skill_id,
            &mode,
            overwrite,
            registry_path.as_deref(),
        )
        .map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Distributes a skill from `~/.agent-locator/skills` to target agents using `symlink` or `copy`.
#[napi]
pub async fn distribute_skill(
    skill_id: String,
    agent_keys: Vec<String>,
    mode: String,
    registry_path: Option<String>,
) -> Result<SkillMetadata> {
    tokio::task::spawn_blocking(move || {
        skills::distribute_skill(&skill_id, agent_keys, &mode, registry_path.as_deref())
            .map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Removes a distributed skill from the specified agents without deleting the central hub copy.
#[napi]
pub async fn undistribute_skill(
    skill_id: String,
    agent_keys: Vec<String>,
    registry_path: Option<String>,
) -> Result<SkillMetadata> {
    tokio::task::spawn_blocking(move || {
        skills::undistribute_skill(&skill_id, agent_keys, registry_path.as_deref())
            .map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Synchronizes all managed skill placements recorded in `~/.agent-locator/state.json`.
#[napi]
pub async fn sync_all_skills() -> Result<SyncReport> {
    tokio::task::spawn_blocking(move || skills::sync_all_skills().map_err(to_napi_err))
        .await
        .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

/// Runs diagnostics across installed agents and central hub, optionally fixing issues.
#[napi]
pub async fn run_doctor(
    registry_path: Option<String>,
    fix: Option<bool>,
) -> Result<DoctorReport> {
    tokio::task::spawn_blocking(move || {
        skills::run_doctor(registry_path.as_deref(), fix).map_err(to_napi_err)
    })
    .await
    .map_err(|e| to_napi_err(format!("Task join error: {}", e)))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<F: FnOnce(&Path)>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var_os("AGENT_LOCATOR_HOME");
        std::env::set_var("AGENT_LOCATOR_HOME", tmp.path());
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f(tmp.path());
        }));
        match prev {
            Some(v) => std::env::set_var("AGENT_LOCATOR_HOME", v),
            None => std::env::remove_var("AGENT_LOCATOR_HOME"),
        }
        if let Err(payload) = res {
            std::panic::resume_unwind(payload);
        }
    }

    fn write_skill(dir: &Path, name: &str, desc: &str, extra: &str) {
        fs::create_dir_all(dir).unwrap();
        let md = format!("---\nname: {}\ndescription: {}\n---\n\n# {}\n{}\n", name, desc, name, extra);
        fs::write(dir.join("SKILL.md"), md).unwrap();
    }

    #[test]
    fn test_scan_installed_detection_avoids_shared_dir_false_positives() {
        with_temp_home(|home| {
            // Only create ~/.claude and ~/.agents/skills (no ~/.warp, ~/.cline, ~/.zed, ~/.codex)
            fs::create_dir_all(home.join(".claude/skills/hello-skill")).unwrap();
            write_skill(&home.join(".claude/skills/hello-skill"), "Hello", "Test skill", "");
            fs::create_dir_all(home.join(".agents/skills/shared-skill")).unwrap();
            write_skill(&home.join(".agents/skills/shared-skill"), "Shared", "Shared skill", "");

            let registry_path = concat!(env!("CARGO_MANIFEST_DIR"), "/agent-tools.json");
            let scan = scanner::scan_all_agents(Some(registry_path), None).unwrap();

            assert_eq!(scan.summary.total_checked, 87);
            assert_eq!(scan.summary.unlisted_local_count, 0);
            assert!(scan.unlisted_local_dirs.is_empty());

            let installed_keys: Vec<&str> = scan
                .installed_agents
                .iter()
                .map(|a| a.key.as_str())
                .collect();
            assert!(installed_keys.contains(&"claude_code"));
            assert!(!installed_keys.contains(&"warp"));
            assert!(!installed_keys.contains(&"cline"));
            assert!(!installed_keys.contains(&"zed"));
            assert!(!installed_keys.contains(&"codex"));

            let claude = scan
                .installed_agents
                .iter()
                .find(|a| a.key == "claude_code")
                .unwrap();
            assert_eq!(claude.skills_dirs[0].skills, vec!["hello-skill"]);
        });
    }

    #[test]
    fn test_skill_lifecycle_symlink_and_copy_and_drift() {
        with_temp_home(|home| {
            // Simulate installed claude_code and cursor
            fs::create_dir_all(home.join(".claude")).unwrap();
            fs::create_dir_all(home.join(".cursor")).unwrap();

            // Create a standalone skill outside hub and add it
            let external = home.join("external-skill");
            write_skill(&external, "Code Reviewer", "Reviews PRs", "v1");

            let registry_path = concat!(env!("CARGO_MANIFEST_DIR"), "/agent-tools.json");
            let added = skills::add_skill(
                external.to_str().unwrap(),
                Some("code-reviewer"),
                None,
                Some(registry_path),
            )
            .unwrap();
            assert_eq!(added.id, "code-reviewer");
            assert!(added.in_central_hub);
            assert_eq!(added.name.as_deref(), Some("Code Reviewer"));

            // Distribute as symlink to claude_code, copy to cursor
            let after_link = skills::distribute_skill(
                "code-reviewer",
                vec!["claude_code".to_string()],
                "symlink",
                Some(registry_path),
            )
            .unwrap();
            assert_eq!(after_link.placements.len(), 1);
            assert_eq!(after_link.placements[0].mode, "symlink");
            assert_eq!(after_link.placements[0].status, "synced");

            let after_copy = skills::distribute_skill(
                "code-reviewer",
                vec!["cursor".to_string()],
                "copy",
                Some(registry_path),
            )
            .unwrap();
            assert_eq!(after_copy.placements.len(), 2);
            let cursor_p = after_copy
                .placements
                .iter()
                .find(|p| p.agent_key == "cursor")
                .unwrap();
            assert_eq!(cursor_p.mode, "copy");
            assert_eq!(cursor_p.status, "synced");

            // Mutate cursor copy to trigger drift
            fs::write(
                home.join(".cursor/skills/code-reviewer/SKILL.md"),
                "---\nname: Code Reviewer\ndescription: Modified locally\n---\n",
            )
            .unwrap();

            let doc = skills::run_doctor(Some(registry_path), Some(false)).unwrap();
            assert!(!doc.healthy);
            assert!(doc.issues.iter().any(|i| i.issue_type == "drifted_copy"));

            // Sync repairs drifted copy
            let sync = skills::sync_all_skills().unwrap();
            assert_eq!(sync.repaired_count, 1);

            let doc_after = skills::run_doctor(Some(registry_path), Some(false)).unwrap();
            assert!(doc_after.healthy);

            // Undistribute from cursor keeps central hub intact
            let after_rm = skills::undistribute_skill(
                "code-reviewer",
                vec!["cursor".to_string()],
                Some(registry_path),
            )
            .unwrap();
            assert_eq!(after_rm.placements.len(), 1);
            assert!(home.join(".agent-locator/skills/code-reviewer/SKILL.md").is_file());
        });
    }

    #[test]
    fn test_import_unmanaged_skill_and_missing_skill_md() {
        with_temp_home(|home| {
            // Put an unmanaged skill without SKILL.md into ~/.claude/skills
            let unmanaged = home.join(".claude/skills/legacy-helper");
            fs::create_dir_all(&unmanaged).unwrap();
            fs::write(unmanaged.join("notes.txt"), "legacy instructions").unwrap();

            let registry_path = concat!(env!("CARGO_MANIFEST_DIR"), "/agent-tools.json");
            let doc = skills::run_doctor(Some(registry_path), Some(false)).unwrap();
            assert!(doc.issues.iter().any(|i| i.issue_type == "unmanaged_skill"));
            assert!(doc.issues.iter().any(|i| i.issue_type == "missing_skill_md"));

            // Import into central hub as symlink
            let imported = skills::import_skill(
                "claude_code",
                "legacy-helper",
                "symlink",
                None,
                Some(registry_path),
            )
            .unwrap();
            assert!(imported.in_central_hub);
            assert!(!imported.has_skill_md);
            assert_eq!(imported.placements[0].mode, "symlink");
            assert_eq!(imported.placements[0].status, "missing_skill_md");

            // Doctor with fix=true creates starter SKILL.md
            let doc_fixed = skills::run_doctor(Some(registry_path), Some(true)).unwrap();
            assert!(doc_fixed.healthy);
            assert!(home.join(".agent-locator/skills/legacy-helper/SKILL.md").is_file());
        });
    }
}
