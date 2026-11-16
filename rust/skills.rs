use crate::models::{
    AgentDef, AgentSkillPlacement, AgentSummary, DoctorIssue, DoctorReport, SkillMetadata,
    SkillsMatrixReport, SyncReport,
};
use crate::scanner::{
    combined_skills_dirs, expand_tilde, is_agent_installed, load_registry, probe_path_entry,
    probe_skills_dir_entry,
};
use crate::state::{
    central_hub_dir, ensure_central_hub_dir, load_state, resolve_home_dir, save_state,
    state_file_path, LocatorState, ManagedPlacementRecord,
};
use chrono::{Local, SecondsFormat};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct ParsedSkillInfo {
    has_skill_md: bool,
    name: Option<String>,
    description: Option<String>,
    content_hash: String,
}

/// Validates that `skill_id` is a safe relative name with no path traversal or leading dots.
pub fn validate_skill_id(skill_id: &str) -> Result<String, String> {
    let trimmed = skill_id.trim();
    if trimmed.is_empty() {
        return Err("Skill ID cannot be empty".to_string());
    }
    let path = Path::new(trimmed);
    let mut parts = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy().to_string();
                if s.starts_with('.') {
                    return Err(format!("Invalid skill ID '{}': cannot start with '.'", trimmed));
                }
                parts.push(s);
            }
            _ => {
                return Err(format!(
                    "Invalid skill ID '{}': path traversal or absolute path is not allowed",
                    trimmed
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(format!("Invalid skill ID '{}'", trimmed));
    }
    Ok(parts.join("/"))
}

/// Parses YAML frontmatter (`name`, `description`) from `<skill_dir>/SKILL.md`.
pub fn parse_skill_frontmatter(skill_dir: &Path) -> (bool, Option<String>, Option<String>) {
    let skill_md = skill_dir.join("SKILL.md");
    if !skill_md.is_file() {
        return (false, None, None);
    }

    let content = match fs::read_to_string(&skill_md) {
        Ok(c) => c,
        Err(_) => return (true, None, None),
    };

    let normalized = content.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    let first = match lines.next() {
        Some(l) => l.trim(),
        None => return (true, None, None),
    };
    if first != "---" {
        return (true, None, None);
    }

    let mut yaml_lines = Vec::new();
    let mut found_end = false;
    for line in lines {
        if line.trim() == "---" {
            found_end = true;
            break;
        }
        yaml_lines.push(line);
    }
    if !found_end {
        return (true, None, None);
    }

    let yaml_block = yaml_lines.join("\n");
    let val: serde_yaml::Value = match serde_yaml::from_str(&yaml_block) {
        Ok(v) => v,
        Err(_) => return (true, None, None),
    };

    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let description = val
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    (true, name, description)
}

/// Computes a deterministic SHA-256 hash over all files in `dir` (sorted by relative path).
pub fn compute_dir_hash(dir: &Path) -> Result<String, String> {
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    let mut it = WalkDir::new(dir).follow_links(true).into_iter();
    while let Some(entry_res) = it.next() {
        let entry = entry_res.map_err(|e| format!("Failed to walk {}: {}", dir.display(), e))?;
        if entry.depth() == 0 {
            continue;
        }
        let fname = entry.file_name().to_string_lossy();
        if fname == ".git" || fname == ".DS_Store" {
            if entry.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .map_err(|e| format!("Path prefix error: {}", e))?;
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if entry.file_type().is_dir() {
            dirs.push(rel_str);
        } else if entry.file_type().is_file() {
            files.push((rel_str, entry.path().to_path_buf()));
        }
    }

    dirs.sort();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for d in dirs {
        hasher.update(b"D:");
        hasher.update(d.as_bytes());
        hasher.update(b"\0");
    }
    for (rel_path, full_path) in files {
        let bytes = fs::read(&full_path)
            .map_err(|e| format!("Failed to read file {}: {}", full_path.display(), e))?;
        hasher.update(b"F:");
        hasher.update(rel_path.as_bytes());
        hasher.update(b"\0");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn inspect_skill_dir(dir: &Path) -> Result<ParsedSkillInfo, String> {
    let (has_skill_md, name, description) = parse_skill_frontmatter(dir);
    let content_hash = compute_dir_hash(dir)?;
    Ok(ParsedSkillInfo {
        has_skill_md,
        name,
        description,
        content_hash,
    })
}

/// Recursively copies `src` directory into `dst`.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_dir() {
        return Err(format!("Source directory does not exist: {}", src.display()));
    }
    let canon_src = fs::canonicalize(src)
        .map_err(|e| format!("Failed to canonicalize {}: {}", src.display(), e))?;
    if dst.exists() {
        if let Ok(canon_dst) = fs::canonicalize(dst) {
            if canon_dst == canon_src || canon_dst.starts_with(&canon_src) {
                return Err(format!(
                    "Cannot copy directory {} into itself or its subdirectory {}",
                    src.display(),
                    dst.display()
                ));
            }
        }
    }

    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create destination directory {}: {}", dst.display(), e))?;

    let mut it = WalkDir::new(src).follow_links(true).into_iter();
    while let Some(entry_res) = it.next() {
        let entry = entry_res.map_err(|e| format!("Failed walking {}: {}", src.display(), e))?;
        if entry.depth() == 0 {
            continue;
        }
        let fname = entry.file_name().to_string_lossy();
        if fname == ".DS_Store" || fname == ".git" {
            if entry.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| format!("Strip prefix failed: {}", e))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Failed creating dir {}: {}", target.display(), e))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed creating parent {}: {}", parent.display(), e))?;
            }
            fs::copy(entry.path(), &target).map_err(|e| {
                format!(
                    "Failed copying {} -> {}: {}",
                    entry.path().display(),
                    target.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

/// Removes a file, symlink (including broken symlink), or directory.
pub fn remove_path_any(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || meta.is_file() {
                fs::remove_file(path)
                    .map_err(|e| format!("Failed to remove file/symlink {}: {}", path.display(), e))
            } else if meta.is_dir() {
                fs::remove_dir_all(path)
                    .map_err(|e| format!("Failed to remove directory {}: {}", path.display(), e))
            } else {
                fs::remove_file(path)
                    .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to stat {}: {}", path.display(), e)),
    }
}

#[cfg(unix)]
fn create_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(src, dst).map_err(|e| {
        format!(
            "Failed to create symlink {} -> {}: {}",
            dst.display(),
            src.display(),
            e
        )
    })
}

#[cfg(windows)]
fn create_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(src, dst).map_err(|e| {
        format!(
            "Failed to create symlink {} -> {}: {}",
            dst.display(),
            src.display(),
            e
        )
    })
}

/// Selects the target global skills directory for an agent.
/// Prefers an already-existing `global_skills_dirs` entry, then one matching an existing `detect_paths`
/// prefix, and falls back to `global_skills_dirs[0]`.
pub fn select_agent_skills_dir(agent: &AgentDef, home_dir: &Path) -> Result<PathBuf, String> {
    if agent.global_skills_dirs.is_empty() {
        return Err(format!(
            "Agent '{}' ({}) has no global_skills_dirs configured",
            agent.display_name, agent.key
        ));
    }

    for dir in &agent.global_skills_dirs {
        let expanded = expand_tilde(dir, home_dir);
        if expanded.exists() && expanded.is_dir() {
            return Ok(expanded);
        }
    }

    for detect in &agent.detect_paths {
        let detect_exp = expand_tilde(detect, home_dir);
        if detect_exp.exists() {
            for dir in &agent.global_skills_dirs {
                let dir_exp = expand_tilde(dir, home_dir);
                if dir_exp.starts_with(&detect_exp) {
                    return Ok(dir_exp);
                }
            }
        }
    }

    Ok(expand_tilde(&agent.global_skills_dirs[0], home_dir))
}

fn collect_skill_entries_in_dir(
    skills_dir: &Path,
    recursive_scan: bool,
) -> Vec<(String, PathBuf, bool)> {
    // Returns (skill_id, path, is_broken_symlink)
    let mut results = Vec::new();
    let entries = match fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return results,
    };

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.is_empty() || fname.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.file_type().is_symlink() && !path.exists() {
            results.push((fname, path, true));
            continue;
        }
        if !path.is_dir() {
            continue;
        }

        if !recursive_scan {
            results.push((fname, path, false));
        } else {
            let mut found_nested = Vec::new();
            let mut it = WalkDir::new(&path).follow_links(true).into_iter();
            while let Some(Ok(dent)) = it.next() {
                let dname = dent.file_name().to_string_lossy();
                if dent.depth() > 0 && dname.starts_with('.') {
                    if dent.file_type().is_dir() {
                        it.skip_current_dir();
                    }
                    continue;
                }
                if dent.file_type().is_dir() && dent.path().join("SKILL.md").is_file() {
                    if let Ok(rel) = dent.path().strip_prefix(skills_dir) {
                        let rel_str = rel
                            .components()
                            .map(|c| c.as_os_str().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/");
                        if !rel_str.is_empty() {
                            found_nested.push((rel_str, dent.path().to_path_buf(), false));
                        }
                    }
                    it.skip_current_dir();
                }
            }
            if found_nested.is_empty() {
                results.push((fname, path, false));
            } else {
                results.extend(found_nested);
            }
        }
    }

    results
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn find_state_copy_match<'a>(
    state: &'a LocatorState,
    skill_id: &str,
    agent_key: &str,
    target_path: &Path,
) -> Option<&'a ManagedPlacementRecord> {
    let target_str = target_path.to_string_lossy();
    state.placements.iter().find(|p| {
        p.skill_id == skill_id
            && p.mode == "copy"
            && (p.agent_key == agent_key || p.target_path == target_str.as_ref())
    })
}

/// Computes the complete matrix of known installed agents, central hub skills, and placements.
pub fn get_skills_matrix(registry_path: Option<&str>) -> Result<SkillsMatrixReport, String> {
    let home_dir = resolve_home_dir()?;
    let hub_dir = central_hub_dir(&home_dir);
    let state_path = state_file_path(&home_dir);
    let state = load_state(&home_dir)?;
    let (_, registry) = load_registry(registry_path)?;

    // 1. Scan central hub skills
    let mut central_skills: BTreeMap<String, (PathBuf, ParsedSkillInfo)> = BTreeMap::new();
    if hub_dir.is_dir() {
        for (skill_id, path, broken) in collect_skill_entries_in_dir(&hub_dir, false) {
            if broken {
                continue;
            }
            if let Ok(info) = inspect_skill_dir(&path) {
                central_skills.insert(skill_id, (path, info));
            }
        }
    }
    // Also inspect nested skills in central hub (e.g. imported from recursive_scan agents)
    if hub_dir.is_dir() {
        for (skill_id, path, broken) in collect_skill_entries_in_dir(&hub_dir, true) {
            if broken || central_skills.contains_key(&skill_id) {
                continue;
            }
            if let Ok(info) = inspect_skill_dir(&path) {
                central_skills.insert(skill_id, (path, info));
            }
        }
    }

    // 2. Determine installed agents & scan their global_skills_dirs
    let mut installed_defs: Vec<AgentDef> = Vec::new();
    let mut installed_summaries: Vec<AgentSummary> = Vec::new();
    let mut placements_by_skill: BTreeMap<String, Vec<AgentSkillPlacement>> = BTreeMap::new();
    let mut fallback_info_by_skill: BTreeMap<String, ParsedSkillInfo> = BTreeMap::new();

    for agent in &registry.agents {
        let detect_hits: Vec<_> = agent
            .detect_paths
            .iter()
            .map(|p| probe_path_entry(p, &home_dir))
            .collect();
        let combined = combined_skills_dirs(agent);
        let skills_dir_entries: Vec<_> = combined
            .iter()
            .map(|p| probe_skills_dir_entry(p, &home_dir, agent.recursive_scan))
            .collect();

        if !is_agent_installed(agent, &detect_hits, &skills_dir_entries) {
            continue;
        }

        let primary_dir = select_agent_skills_dir(agent, &home_dir)
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let hit_paths: Vec<String> = detect_hits
            .iter()
            .filter(|h| h.exists)
            .map(|h| h.configured_path.clone())
            .collect();

        let mut agent_unique_skills: BTreeSet<String> = BTreeSet::new();
        let mut seen_dir_paths: BTreeSet<PathBuf> = BTreeSet::new();

        for configured in &agent.global_skills_dirs {
            let expanded = expand_tilde(configured, &home_dir);
            if !seen_dir_paths.insert(expanded.clone()) {
                continue;
            }
            if !expanded.exists() || !expanded.is_dir() {
                continue;
            }

            for (skill_id, entry_path, is_broken) in
                collect_skill_entries_in_dir(&expanded, agent.recursive_scan)
            {
                agent_unique_skills.insert(skill_id.clone());

                if is_broken {
                    let symlink_target = fs::read_link(&entry_path)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string());
                    placements_by_skill.entry(skill_id).or_default().push(
                        AgentSkillPlacement {
                            agent_key: agent.key.clone(),
                            agent_display_name: agent.display_name.clone(),
                            target_path: entry_path.to_string_lossy().to_string(),
                            resolved_path: entry_path.to_string_lossy().to_string(),
                            mode: "broken_symlink".to_string(),
                            status: "broken".to_string(),
                            symlink_target,
                            content_hash: None,
                        },
                    );
                    continue;
                }

                let meta = match fs::symlink_metadata(&entry_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let is_symlink = meta.file_type().is_symlink();
                let symlink_target = if is_symlink {
                    fs::read_link(&entry_path)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                } else {
                    None
                };
                let resolved_buf = canonical_or_raw(&entry_path);
                let resolved_path = resolved_buf.to_string_lossy().to_string();

                let info = inspect_skill_dir(&entry_path).ok();
                if let Some(ref parsed) = info {
                    fallback_info_by_skill
                        .entry(skill_id.clone())
                        .or_insert_with(|| parsed.clone());
                }

                let central_entry = central_skills.get(&skill_id);
                let expected_central = hub_dir.join(&skill_id);

                let (mode, status) = if is_symlink {
                    let points_to_central = canonical_or_raw(&expected_central) == resolved_buf;
                    if points_to_central {
                        let st = match &info {
                            Some(i) if !i.has_skill_md => "missing_skill_md",
                            _ => "synced",
                        };
                        ("symlink".to_string(), st.to_string())
                    } else {
                        ("symlink".to_string(), "unmanaged".to_string())
                    }
                } else if let Some(_record) =
                    find_state_copy_match(&state, &skill_id, &agent.key, &entry_path)
                {
                    let st = match (central_entry, &info) {
                        (Some((_, c_info)), Some(a_info)) => {
                            if a_info.content_hash != c_info.content_hash {
                                "drifted"
                            } else if !a_info.has_skill_md {
                                "missing_skill_md"
                            } else {
                                "synced"
                            }
                        }
                        (None, _) => "drifted",
                        (_, None) => "drifted",
                    };
                    ("copy".to_string(), st.to_string())
                } else {
                    ("unmanaged".to_string(), "unmanaged".to_string())
                };

                placements_by_skill.entry(skill_id).or_default().push(
                    AgentSkillPlacement {
                        agent_key: agent.key.clone(),
                        agent_display_name: agent.display_name.clone(),
                        target_path: entry_path.to_string_lossy().to_string(),
                        resolved_path,
                        mode,
                        status,
                        symlink_target,
                        content_hash: info.map(|i| i.content_hash),
                    },
                );
            }
        }

        installed_summaries.push(AgentSummary {
            key: agent.key.clone(),
            display_name: agent.display_name.clone(),
            category: agent.category.clone(),
            primary_skills_dir: primary_dir,
            skills_dirs: agent.global_skills_dirs.clone(),
            detect_hits: hit_paths,
            skills_count: agent_unique_skills.len() as u32,
        });
        installed_defs.push(agent.clone());
    }

    // 3. Build unified SkillMetadata list across central hub & discovered placements
    let mut all_ids: BTreeSet<String> = BTreeSet::new();
    for id in central_skills.keys() {
        all_ids.insert(id.clone());
    }
    for id in placements_by_skill.keys() {
        all_ids.insert(id.clone());
    }

    let mut skills = Vec::new();
    for id in all_ids {
        let placements = placements_by_skill.remove(&id).unwrap_or_default();
        if let Some((c_path, c_info)) = central_skills.get(&id) {
            skills.push(SkillMetadata {
                id,
                name: c_info.name.clone(),
                description: c_info.description.clone(),
                has_skill_md: c_info.has_skill_md,
                content_hash: c_info.content_hash.clone(),
                in_central_hub: true,
                central_path: Some(c_path.to_string_lossy().to_string()),
                placements,
            });
        } else {
            let fallback = fallback_info_by_skill.get(&id);
            skills.push(SkillMetadata {
                id,
                name: fallback.and_then(|f| f.name.clone()),
                description: fallback.and_then(|f| f.description.clone()),
                has_skill_md: fallback.map(|f| f.has_skill_md).unwrap_or(false),
                content_hash: fallback
                    .map(|f| f.content_hash.clone())
                    .unwrap_or_default(),
                in_central_hub: false,
                central_path: None,
                placements,
            });
        }
    }

    Ok(SkillsMatrixReport {
        central_hub_dir: hub_dir.to_string_lossy().to_string(),
        state_file_path: state_path.to_string_lossy().to_string(),
        installed_agents: installed_summaries,
        all_agents: registry.agents,
        skills,
    })
}

fn find_skill_metadata(skill_id: &str, registry_path: Option<&str>) -> Result<SkillMetadata, String> {
    let matrix = get_skills_matrix(registry_path)?;
    matrix
        .skills
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill '{}' not found after operation", skill_id))
}

fn validate_mode(mode: &str) -> Result<String, String> {
    let normalized = mode.trim().to_lowercase();
    match normalized.as_str() {
        "symlink" | "copy" => Ok(normalized),
        _ => Err(format!(
            "Invalid distribution mode '{}': expected 'symlink' or 'copy'",
            mode
        )),
    }
}

/// Adds a skill from a local directory path into `~/.agent-locator/skills/<skill_id>`.
pub fn add_skill(
    source_path: &str,
    skill_id: Option<&str>,
    overwrite: Option<bool>,
    registry_path: Option<&str>,
) -> Result<SkillMetadata, String> {
    let src = PathBuf::from(source_path);
    if !src.exists() || !src.is_dir() {
        return Err(format!(
            "Source skill path does not exist or is not a directory: {}",
            src.display()
        ));
    }

    let derived_id = match skill_id.filter(|s| !s.trim().is_empty()) {
        Some(id) => validate_skill_id(id)?,
        None => {
            let fname = src
                .file_name()
                .ok_or_else(|| format!("Cannot infer skill ID from {}", src.display()))?
                .to_string_lossy()
                .to_string();
            validate_skill_id(&fname)?
        }
    };

    let home_dir = resolve_home_dir()?;
    let hub = ensure_central_hub_dir(&home_dir)?;
    let dst = hub.join(&derived_id);

    if dst.exists() {
        if canonical_or_raw(&src) == canonical_or_raw(&dst) {
            return find_skill_metadata(&derived_id, registry_path);
        }
        let src_hash = compute_dir_hash(&src)?;
        let dst_hash = compute_dir_hash(&dst)?;
        if src_hash != dst_hash {
            if !overwrite.unwrap_or(false) {
                return Err(format!(
                    "Skill '{}' already exists in central hub with different content (pass overwrite=true to replace)",
                    derived_id
                ));
            }
            remove_path_any(&dst)?;
            copy_dir_recursive(&src, &dst)?;
        }
    } else {
        copy_dir_recursive(&src, &dst)?;
    }

    find_skill_metadata(&derived_id, registry_path)
}

/// Imports an unmanaged skill from an installed agent's skills directory into `~/.agent-locator/skills`
/// and replaces the original entry with either a `symlink` or tracked `copy`.
pub fn import_skill(
    agent_key: &str,
    skill_id: &str,
    mode: &str,
    overwrite: Option<bool>,
    registry_path: Option<&str>,
) -> Result<SkillMetadata, String> {
    let mode = validate_mode(mode)?;
    let id = validate_skill_id(skill_id)?;
    let home_dir = resolve_home_dir()?;
    let hub = ensure_central_hub_dir(&home_dir)?;
    let (_, registry) = load_registry(registry_path)?;

    let agent = registry
        .agents
        .iter()
        .find(|a| a.key == agent_key)
        .ok_or_else(|| format!("Unknown agent key '{}'", agent_key))?;

    let mut found_src: Option<PathBuf> = None;
    for dir in &agent.global_skills_dirs {
        let candidate = expand_tilde(dir, &home_dir).join(&id);
        if fs::symlink_metadata(&candidate).is_ok() {
            found_src = Some(candidate);
            break;
        }
    }
    let src_path = found_src.ok_or_else(|| {
        format!(
            "Skill '{}' not found in global_skills_dirs of agent '{}'",
            id, agent_key
        )
    })?;

    let central_path = hub.join(&id);
    let src_meta = fs::symlink_metadata(&src_path)
        .map_err(|e| format!("Failed to stat {}: {}", src_path.display(), e))?;

    // Check if src_path is already a symlink pointing to central_path
    let already_linked_to_central = src_meta.file_type().is_symlink()
        && src_path.exists()
        && central_path.exists()
        && canonical_or_raw(&src_path) == canonical_or_raw(&central_path);

    if !already_linked_to_central {
        if src_meta.file_type().is_symlink() && !src_path.exists() {
            return Err(format!(
                "Cannot import broken symlink at {}",
                src_path.display()
            ));
        }
        let src_hash = compute_dir_hash(&src_path)?;
        if central_path.exists() {
            let central_hash = compute_dir_hash(&central_path)?;
            if central_hash != src_hash {
                if !overwrite.unwrap_or(false) {
                    return Err(format!(
                        "Conflict: skill '{}' already exists in central hub with different content (use overwrite to replace)",
                        id
                    ));
                }
                remove_path_any(&central_path)?;
                copy_dir_recursive(&src_path, &central_path)?;
            }
        } else {
            copy_dir_recursive(&src_path, &central_path)?;
        }
    }

    // Replace agent entry according to requested mode
    let central_hash = compute_dir_hash(&central_path)?;
    if !already_linked_to_central || mode == "copy" {
        remove_path_any(&src_path)?;
        if let Some(parent) = src_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent {}: {}", parent.display(), e))?;
        }
        if mode == "symlink" {
            create_symlink(&central_path, &src_path)?;
        } else {
            copy_dir_recursive(&central_path, &src_path)?;
        }
    }

    let mut state = load_state(&home_dir)?;
    state.upsert_placement(ManagedPlacementRecord {
        skill_id: id.clone(),
        agent_key: agent.key.clone(),
        target_path: src_path.to_string_lossy().to_string(),
        mode,
        synced_hash: central_hash,
        synced_at: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
    });
    save_state(&home_dir, &state)?;

    find_skill_metadata(&id, registry_path)
}

/// Distributes a central-hub skill to one or more agents using `symlink` or `copy`.
pub fn distribute_skill(
    skill_id: &str,
    agent_keys: Vec<String>,
    mode: &str,
    registry_path: Option<&str>,
) -> Result<SkillMetadata, String> {
    let mode = validate_mode(mode)?;
    let id = validate_skill_id(skill_id)?;
    let home_dir = resolve_home_dir()?;
    let hub = central_hub_dir(&home_dir);
    let central_path = hub.join(&id);

    if !central_path.exists() || !central_path.is_dir() {
        return Err(format!(
            "Skill '{}' does not exist in central hub ({}). Import or add it first.",
            id,
            central_path.display()
        ));
    }

    let central_hash = compute_dir_hash(&central_path)?;
    let (_, registry) = load_registry(registry_path)?;
    let mut state = load_state(&home_dir)?;
    let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, false);

    for agent_key in &agent_keys {
        let agent = registry
            .agents
            .iter()
            .find(|a| a.key == *agent_key)
            .ok_or_else(|| format!("Unknown agent key '{}'", agent_key))?;

        let target_skills_dir = select_agent_skills_dir(agent, &home_dir)?;
        let target_path = target_skills_dir.join(&id);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        remove_path_any(&target_path)?;
        if mode == "symlink" {
            create_symlink(&central_path, &target_path)?;
        } else {
            copy_dir_recursive(&central_path, &target_path)?;
        }

        state.upsert_placement(ManagedPlacementRecord {
            skill_id: id.clone(),
            agent_key: agent.key.clone(),
            target_path: target_path.to_string_lossy().to_string(),
            mode: mode.clone(),
            synced_hash: central_hash.clone(),
            synced_at: now.clone(),
        });
    }

    save_state(&home_dir, &state)?;
    find_skill_metadata(&id, registry_path)
}

/// Removes a distributed skill from one or more agents without deleting the canonical central copy.
pub fn undistribute_skill(
    skill_id: &str,
    agent_keys: Vec<String>,
    registry_path: Option<&str>,
) -> Result<SkillMetadata, String> {
    let id = validate_skill_id(skill_id)?;
    let home_dir = resolve_home_dir()?;
    let (_, registry) = load_registry(registry_path)?;
    let mut state = load_state(&home_dir)?;

    for agent_key in &agent_keys {
        let agent = registry
            .agents
            .iter()
            .find(|a| a.key == *agent_key)
            .ok_or_else(|| format!("Unknown agent key '{}'", agent_key))?;

        if let Some(record) = state.find_placement(&id, &agent.key).cloned() {
            remove_path_any(Path::new(&record.target_path))?;
        }

        for dir in &agent.global_skills_dirs {
            let candidate = expand_tilde(dir, &home_dir).join(&id);
            remove_path_any(&candidate)?;
        }

        state.remove_placement(&id, &agent.key);
    }

    save_state(&home_dir, &state)?;

    let matrix = get_skills_matrix(registry_path)?;
    if let Some(found) = matrix.skills.into_iter().find(|s| s.id == id) {
        Ok(found)
    } else {
        Ok(SkillMetadata {
            id,
            name: None,
            description: None,
            has_skill_md: false,
            content_hash: String::new(),
            in_central_hub: false,
            central_path: None,
            placements: Vec::new(),
        })
    }
}

/// Synchronizes all managed placements recorded in `state.json`.
pub fn sync_all_skills() -> Result<SyncReport, String> {
    let home_dir = resolve_home_dir()?;
    let hub = central_hub_dir(&home_dir);
    let mut state = load_state(&home_dir)?;

    let mut synced_count = 0u32;
    let mut repaired_count = 0u32;
    let mut skipped_count = 0u32;
    let mut details = Vec::new();
    let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, false);

    for record in &mut state.placements {
        let central_path = hub.join(&record.skill_id);
        if !central_path.exists() || !central_path.is_dir() {
            skipped_count += 1;
            details.push(format!(
                "SKIP {} ({}) - not present in central hub",
                record.skill_id, record.agent_key
            ));
            continue;
        }

        let central_hash = compute_dir_hash(&central_path)?;
        let target_path = PathBuf::from(&record.target_path);

        if record.mode == "symlink" {
            let ok_link = fs::symlink_metadata(&target_path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
                && target_path.exists()
                && canonical_or_raw(&target_path) == canonical_or_raw(&central_path);
            if ok_link {
                record.synced_hash = central_hash;
                synced_count += 1;
            } else {
                remove_path_any(&target_path)?;
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed creating parent {}: {}", parent.display(), e)
                    })?;
                }
                create_symlink(&central_path, &target_path)?;
                record.synced_hash = central_hash;
                record.synced_at = now.clone();
                repaired_count += 1;
                details.push(format!(
                    "REPAIRED symlink {} -> {}",
                    record.skill_id, record.agent_key
                ));
            }
        } else {
            let meta = fs::symlink_metadata(&target_path).ok();
            let is_real_dir = meta
                .as_ref()
                .map(|m| m.is_dir() && !m.file_type().is_symlink())
                .unwrap_or(false);
            let up_to_date = if is_real_dir {
                compute_dir_hash(&target_path)
                    .map(|h| h == central_hash)
                    .unwrap_or(false)
            } else {
                false
            };

            if up_to_date {
                record.synced_hash = central_hash;
                synced_count += 1;
            } else {
                remove_path_any(&target_path)?;
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed creating parent {}: {}", parent.display(), e)
                    })?;
                }
                copy_dir_recursive(&central_path, &target_path)?;
                record.synced_hash = central_hash;
                record.synced_at = now.clone();
                repaired_count += 1;
                details.push(format!(
                    "UPDATED copy {} -> {}",
                    record.skill_id, record.agent_key
                ));
            }
        }
    }

    save_state(&home_dir, &state)?;

    Ok(SyncReport {
        synced_count,
        repaired_count,
        skipped_count,
        details,
    })
}

/// Checks for broken symlinks, drifted copies, unmanaged skills, and missing `SKILL.md` files.
/// When `fix` is true, repairs fixable issues safely.
pub fn run_doctor(registry_path: Option<&str>, fix: Option<bool>) -> Result<DoctorReport, String> {
    let should_fix = fix.unwrap_or(false);
    let home_dir = resolve_home_dir()?;
    let hub_dir = central_hub_dir(&home_dir);
    let matrix = get_skills_matrix(registry_path)?;
    let mut issues: Vec<DoctorIssue> = Vec::new();
    let mut fixed_count = 0u32;

    // 1. Per-agent placement checks (deduplicate by physical target_path so shared dirs aren't reported twice)
    let mut seen_paths: BTreeSet<String> = BTreeSet::new();
    let mut newly_imported_central_paths: BTreeMap<String, PathBuf> = BTreeMap::new();

    for skill in &matrix.skills {
        for placement in &skill.placements {
            if !seen_paths.insert(placement.target_path.clone()) {
                continue;
            }

            if placement.status == "broken" {
                let mut fixed = false;
                if should_fix {
                    let target = Path::new(&placement.target_path);
                    let mut state = load_state(&home_dir)?;
                    if skill.in_central_hub {
                        if let Some(ref c_path) = skill.central_path {
                            if remove_path_any(target).is_ok()
                                && create_symlink(Path::new(c_path), target).is_ok()
                            {
                                fixed = true;
                                fixed_count += 1;
                            }
                        }
                    } else if remove_path_any(target).is_ok() {
                        state.remove_placement(&skill.id, &placement.agent_key);
                        let _ = save_state(&home_dir, &state);
                        fixed = true;
                        fixed_count += 1;
                    }
                }
                issues.push(DoctorIssue {
                    issue_type: "broken_symlink".to_string(),
                    severity: "error".to_string(),
                    skill_id: skill.id.clone(),
                    agent_key: Some(placement.agent_key.clone()),
                    path: placement.target_path.clone(),
                    message: format!(
                        "Broken symlink '{}' in {} ({})",
                        skill.id, placement.agent_display_name, placement.target_path
                    ),
                    fixable: true,
                    fixed,
                });
            } else if placement.status == "drifted" {
                let mut fixed = false;
                if should_fix {
                    if distribute_skill(
                        &skill.id,
                        vec![placement.agent_key.clone()],
                        "copy",
                        registry_path,
                    )
                    .is_ok()
                    {
                        fixed = true;
                        fixed_count += 1;
                    }
                }
                issues.push(DoctorIssue {
                    issue_type: "drifted_copy".to_string(),
                    severity: "warning".to_string(),
                    skill_id: skill.id.clone(),
                    agent_key: Some(placement.agent_key.clone()),
                    path: placement.target_path.clone(),
                    message: format!(
                        "Copied skill '{}' in {} has drifted from central hub",
                        skill.id, placement.agent_display_name
                    ),
                    fixable: true,
                    fixed,
                });
            } else if placement.status == "unmanaged" {
                let mut fixed = false;
                if should_fix
                    && import_skill(
                        &placement.agent_key,
                        &skill.id,
                        "symlink",
                        Some(false),
                        registry_path,
                    )
                    .is_ok()
                {
                    fixed = true;
                    fixed_count += 1;
                    newly_imported_central_paths
                        .insert(skill.id.clone(), hub_dir.join(&skill.id));
                }
                issues.push(DoctorIssue {
                    issue_type: "unmanaged_skill".to_string(),
                    severity: "info".to_string(),
                    skill_id: skill.id.clone(),
                    agent_key: Some(placement.agent_key.clone()),
                    path: placement.target_path.clone(),
                    message: format!(
                        "Unmanaged skill '{}' found in {} ({})",
                        skill.id, placement.agent_display_name, placement.target_path
                    ),
                    fixable: true,
                    fixed,
                });
            }
        }
    }

    // 2. SKILL.md & YAML frontmatter metadata checks across central hub and unmanaged skills
    for skill in &matrix.skills {
        let has_broken_only = !skill.in_central_hub
            && skill.placements.iter().all(|p| p.status == "broken");
        if has_broken_only {
            continue;
        }

        let target_dir = if let Some(ref c_path) = skill.central_path {
            PathBuf::from(c_path)
        } else if let Some(imported_path) = newly_imported_central_paths.get(&skill.id) {
            imported_path.clone()
        } else if let Some(p) = skill.placements.iter().find(|p| p.status != "broken") {
            PathBuf::from(&p.target_path)
        } else {
            continue;
        };

        if !skill.has_skill_md {
            let mut fixed = false;
            if should_fix && target_dir.is_dir() {
                let starter = format!(
                    "---\nname: {}\ndescription: Skill managed by agent-locator\n---\n\n# {}\n",
                    skill.id, skill.id
                );
                if fs::write(target_dir.join("SKILL.md"), starter).is_ok() {
                    fixed = true;
                    fixed_count += 1;
                }
            }
            issues.push(DoctorIssue {
                issue_type: "missing_skill_md".to_string(),
                severity: "warning".to_string(),
                skill_id: skill.id.clone(),
                agent_key: None,
                path: target_dir.to_string_lossy().to_string(),
                message: format!("Skill '{}' is missing SKILL.md", skill.id),
                fixable: true,
                fixed,
            });
        } else if skill.name.is_none() || skill.description.is_none() {
            issues.push(DoctorIssue {
                issue_type: "missing_skill_md".to_string(),
                severity: "info".to_string(),
                skill_id: skill.id.clone(),
                agent_key: None,
                path: target_dir.to_string_lossy().to_string(),
                message: format!(
                    "Skill '{}' SKILL.md is missing frontmatter 'name' or 'description'",
                    skill.id
                ),
                fixable: false,
                fixed: false,
            });
        }
    }

    let total_issues = issues.len() as u32;
    let healthy = issues.iter().all(|i| i.fixed);

    Ok(DoctorReport {
        healthy,
        total_issues,
        fixed_count,
        issues,
    })
}
