use crate::models::{
    AgentDef, AgentRegistry, HostInfo, PathEntry, RegistryRef, ScanResult, ScanSummary,
    ScannedAgent, SkillsDirEntry,
};
use crate::state::resolve_home_dir;
use chrono::{Local, SecondsFormat};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

const EMBEDDED_REGISTRY_JSON: &str = include_str!("../agent-tools.json");

/// Resolves and loads `agent-tools.json` from an explicit path, `AGENT_LOCATOR_REGISTRY`,
/// current/ancestor directories, or the compiled-in embedded registry.
pub fn load_registry(registry_path: Option<&str>) -> Result<(String, AgentRegistry), String> {
    if let Some(p) = registry_path.filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(p);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read registry at {}: {}", path.display(), e))?;
        let registry: AgentRegistry = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse registry JSON at {}: {}", path.display(), e))?;
        return Ok((path.to_string_lossy().to_string(), registry));
    }

    if let Some(env_path) = env::var_os("AGENT_LOCATOR_REGISTRY") {
        let s = env_path.to_string_lossy().trim().to_string();
        if !s.is_empty() {
            let path = PathBuf::from(&s);
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read registry at {}: {}", path.display(), e))?;
            let registry: AgentRegistry = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse registry JSON at {}: {}", path.display(), e))?;
            return Ok((path.to_string_lossy().to_string(), registry));
        }
    }

    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join("agent-tools.json");
            if candidate.is_file() {
                let content = fs::read_to_string(&candidate).map_err(|e| {
                    format!("Failed to read registry at {}: {}", candidate.display(), e)
                })?;
                let registry: AgentRegistry = serde_json::from_str(&content).map_err(|e| {
                    format!(
                        "Failed to parse registry JSON at {}: {}",
                        candidate.display(),
                        e
                    )
                })?;
                return Ok((candidate.to_string_lossy().to_string(), registry));
            }
        }
    }

    if let Ok(exe) = env::current_exe() {
        for ancestor in exe.ancestors() {
            let candidate = ancestor.join("agent-tools.json");
            if candidate.is_file() {
                let content = fs::read_to_string(&candidate).map_err(|e| {
                    format!("Failed to read registry at {}: {}", candidate.display(), e)
                })?;
                let registry: AgentRegistry = serde_json::from_str(&content).map_err(|e| {
                    format!(
                        "Failed to parse registry JSON at {}: {}",
                        candidate.display(),
                        e
                    )
                })?;
                return Ok((candidate.to_string_lossy().to_string(), registry));
            }
        }
    }

    let registry: AgentRegistry = serde_json::from_str(EMBEDDED_REGISTRY_JSON)
        .map_err(|e| format!("Failed to parse embedded agent-tools.json: {}", e))?;
    Ok(("agent-tools.json".to_string(), registry))
}

/// Expands leading `~` or `~/` using `home_dir`.
pub fn expand_tilde(configured: &str, home_dir: &Path) -> PathBuf {
    if configured == "~" {
        home_dir.to_path_buf()
    } else if let Some(rest) = configured.strip_prefix("~/") {
        home_dir.join(rest)
    } else {
        PathBuf::from(configured)
    }
}

/// Probes a path on disk without crashing on broken symlinks or missing files.
pub fn probe_path_entry(configured: &str, home_dir: &Path) -> PathEntry {
    let expanded = expand_tilde(configured, home_dir);
    match fs::symlink_metadata(&expanded) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                let symlink_target = fs::read_link(&expanded)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
                let exists = expanded.exists();
                let resolved_path = fs::canonicalize(&expanded)
                    .unwrap_or_else(|_| expanded.clone())
                    .to_string_lossy()
                    .to_string();
                PathEntry {
                    configured_path: configured.to_string(),
                    resolved_path,
                    exists,
                    kind: "symlink".to_string(),
                    symlink_target,
                }
            } else if meta.is_dir() {
                let resolved_path = fs::canonicalize(&expanded)
                    .unwrap_or_else(|_| expanded.clone())
                    .to_string_lossy()
                    .to_string();
                PathEntry {
                    configured_path: configured.to_string(),
                    resolved_path,
                    exists: true,
                    kind: "dir".to_string(),
                    symlink_target: None,
                }
            } else {
                let resolved_path = fs::canonicalize(&expanded)
                    .unwrap_or_else(|_| expanded.clone())
                    .to_string_lossy()
                    .to_string();
                PathEntry {
                    configured_path: configured.to_string(),
                    resolved_path,
                    exists: true,
                    kind: "file".to_string(),
                    symlink_target: None,
                }
            }
        }
        Err(_) => PathEntry {
            configured_path: configured.to_string(),
            resolved_path: expanded.to_string_lossy().to_string(),
            exists: false,
            kind: "missing".to_string(),
            symlink_target: None,
        },
    }
}

/// Discovers skill names inside a directory.
/// When `recursive_scan` is false, checks immediate non-hidden child directories (including symlinks to dirs).
/// When `recursive_scan` is true, recursively finds nested directories containing `SKILL.md` (and falls back
/// to top-level non-hidden directories that contain no `SKILL.md` anywhere inside).
pub fn discover_skill_names(dir: &Path, recursive_scan: bool) -> Vec<String> {
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }

    let mut skills = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.is_empty() {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if !recursive_scan {
            skills.push(name);
        } else {
            let mut found_nested = Vec::new();
            let mut it = WalkDir::new(&path).follow_links(true).into_iter();
            while let Some(Ok(dent)) = it.next() {
                let fname = dent.file_name().to_string_lossy();
                if dent.depth() > 0 && fname.starts_with('.') {
                    if dent.file_type().is_dir() {
                        it.skip_current_dir();
                    }
                    continue;
                }
                if dent.file_type().is_dir() && dent.path().join("SKILL.md").is_file() {
                    if let Ok(rel) = dent.path().strip_prefix(dir) {
                        let rel_str = rel
                            .components()
                            .map(|c| c.as_os_str().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/");
                        if !rel_str.is_empty() {
                            found_nested.push(rel_str);
                        }
                    }
                    it.skip_current_dir();
                }
            }
            if found_nested.is_empty() {
                skills.push(name);
            } else {
                skills.extend(found_nested);
            }
        }
    }

    skills.sort();
    skills.dedup();
    skills
}

pub fn probe_skills_dir_entry(
    configured: &str,
    home_dir: &Path,
    recursive_scan: bool,
) -> SkillsDirEntry {
    let base = probe_path_entry(configured, home_dir);
    let expanded = expand_tilde(configured, home_dir);
    let skills = if base.exists && expanded.is_dir() {
        discover_skill_names(&expanded, recursive_scan)
    } else {
        Vec::new()
    };
    SkillsDirEntry {
        configured_path: base.configured_path,
        resolved_path: base.resolved_path,
        exists: base.exists,
        kind: base.kind,
        symlink_target: base.symlink_target,
        skills_count: skills.len() as u32,
        skills,
    }
}

/// Determines whether an agent is installed using `detect_paths` first (or `global_skills_dirs` when
/// `detect_paths` is empty, e.g. `universal`).
pub fn is_agent_installed(
    agent: &AgentDef,
    detect_hits: &[PathEntry],
    skills_dirs: &[SkillsDirEntry],
) -> bool {
    if !agent.detect_paths.is_empty() {
        detect_hits.iter().any(|h| h.exists)
    } else {
        skills_dirs.iter().any(|s| s.exists)
    }
}

/// Returns deduplicated skills directories (`global_skills_dirs` followed by `additional_scan_dirs`).
pub fn combined_skills_dirs(agent: &AgentDef) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut combined = Vec::new();
    for p in agent
        .global_skills_dirs
        .iter()
        .chain(agent.additional_scan_dirs.iter())
    {
        if seen.insert(p.clone()) {
            combined.push(p.clone());
        }
    }
    combined
}

/// Sanitizes the OS hostname into a valid RFC 1123 hostname.
fn sanitize_hostname(raw: &str) -> String {
    let labels: Vec<String> = raw
        .split('.')
        .filter_map(|label| {
            let cleaned: String = label
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            let trimmed = cleaned.trim_matches('-');
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(63).collect())
            }
        })
        .collect();
    if labels.is_empty() {
        "localhost".to_string()
    } else {
        let joined = labels.join(".");
        joined.chars().take(253).collect()
    }
}

/// Executes a read-only scan of all known agents in `agent-tools.json`.
pub fn scan_all_agents(
    registry_path: Option<&str>,
    output_path: Option<&str>,
) -> Result<ScanResult, String> {
    let started = Instant::now();
    let home_dir = resolve_home_dir()?;
    let (resolved_registry_path, registry) = load_registry(registry_path)?;

    let mut installed_agents = Vec::new();
    let mut not_installed_keys = Vec::new();

    for agent in &registry.agents {
        let detect_hits: Vec<PathEntry> = agent
            .detect_paths
            .iter()
            .map(|p| probe_path_entry(p, &home_dir))
            .collect();

        let skills_dirs: Vec<SkillsDirEntry> = combined_skills_dirs(agent)
            .iter()
            .map(|p| probe_skills_dir_entry(p, &home_dir, agent.recursive_scan))
            .collect();

        if is_agent_installed(agent, &detect_hits, &skills_dirs) {
            installed_agents.push(ScannedAgent {
                key: agent.key.clone(),
                display_name: agent.display_name.clone(),
                detect_hits,
                skills_dirs,
            });
        } else {
            not_installed_keys.push(agent.key.clone());
        }
    }

    let elapsed = started.elapsed();
    let scanned_at = Local::now().to_rfc3339_opts(SecondsFormat::Secs, false);
    let scan_duration = format!("PT{}S", elapsed.as_secs());

    let raw_hostname = hostname::get()
        .ok()
        .map(|os_str| os_str.to_string_lossy().to_string())
        .unwrap_or_else(|| "localhost".to_string());
    let hostname_str = sanitize_hostname(&raw_hostname);

    let total_checked = registry.agents.len() as u32;
    let installed_count = installed_agents.len() as u32;
    let not_installed_count = not_installed_keys.len() as u32;

    let result = ScanResult {
        schema_version: "1.0".to_string(),
        scanned_at,
        scan_duration,
        host: HostInfo {
            hostname: hostname_str,
            os: std::env::consts::OS.to_string(),
            home_dir: home_dir.to_string_lossy().to_string(),
        },
        registry_ref: RegistryRef {
            path: resolved_registry_path,
            updated_at: registry.updated_at,
            total: registry.total,
        },
        summary: ScanSummary {
            total_checked,
            installed_count,
            not_installed_count,
            unlisted_local_count: 0,
        },
        installed_agents,
        not_installed_keys,
        unlisted_local_dirs: Vec::new(),
    };

    if let Some(out) = output_path.filter(|s| !s.trim().is_empty()) {
        let out_path = PathBuf::from(out);
        if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create output directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Failed to serialize scan result: {}", e))?;
        fs::write(&out_path, format!("{}\n", json))
            .map_err(|e| format!("Failed to write scan result to {}: {}", out_path.display(), e))?;
    }

    Ok(result)
}
