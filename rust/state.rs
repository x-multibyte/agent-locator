use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedPlacementRecord {
    pub skill_id: String,
    pub agent_key: String,
    pub target_path: String,
    /// "symlink" | "copy"
    pub mode: String,
    pub synced_hash: String,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocatorState {
    pub version: String,
    pub placements: Vec<ManagedPlacementRecord>,
}

impl Default for LocatorState {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            placements: Vec::new(),
        }
    }
}

/// Resolves the active home directory. Checks `AGENT_LOCATOR_HOME` first (useful for isolated tests)
/// and falls back to `HOME`.
pub fn resolve_home_dir() -> Result<PathBuf, String> {
    if let Some(custom) = env::var_os("AGENT_LOCATOR_HOME") {
        let s = custom.to_string_lossy().trim().to_string();
        if !s.is_empty() {
            return Ok(PathBuf::from(s));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let s = home.to_string_lossy().trim().to_string();
        if !s.is_empty() {
            return Ok(PathBuf::from(s));
        }
    }
    Err("Unable to resolve HOME directory from environment".to_string())
}

/// Returns `~/.agent-locator`.
pub fn locator_root_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".agent-locator")
}

/// Returns `~/.agent-locator/skills` (isolated canonical skill hub).
pub fn central_hub_dir(home_dir: &Path) -> PathBuf {
    locator_root_dir(home_dir).join("skills")
}

/// Returns `~/.agent-locator/state.json`.
pub fn state_file_path(home_dir: &Path) -> PathBuf {
    locator_root_dir(home_dir).join("state.json")
}

/// Ensures `~/.agent-locator/skills` exists.
pub fn ensure_central_hub_dir(home_dir: &Path) -> Result<PathBuf, String> {
    let hub = central_hub_dir(home_dir);
    fs::create_dir_all(&hub)
        .map_err(|e| format!("Failed to create central skills hub {}: {}", hub.display(), e))?;
    Ok(hub)
}

/// Loads `~/.agent-locator/state.json` if present, or returns an empty default state.
pub fn load_state(home_dir: &Path) -> Result<LocatorState, String> {
    let path = state_file_path(home_dir);
    if !path.exists() {
        return Ok(LocatorState::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read state file {}: {}", path.display(), e))?;
    if content.trim().is_empty() {
        return Ok(LocatorState::default());
    }
    serde_json::from_str::<LocatorState>(&content)
        .map_err(|e| format!("Failed to parse state file {}: {}", path.display(), e))
}

/// Atomically writes `~/.agent-locator/state.json`.
pub fn save_state(home_dir: &Path, state: &LocatorState) -> Result<(), String> {
    let root = locator_root_dir(home_dir);
    fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create state directory {}: {}", root.display(), e))?;
    let path = state_file_path(home_dir);
    let tmp_path = root.join(format!("state.json.tmp.{}", std::process::id()));
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed to serialize state: {}", e))?;
    fs::write(&tmp_path, format!("{}\n", json))
        .map_err(|e| format!("Failed to write temp state {}: {}", tmp_path.display(), e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to replace state file {}: {}", path.display(), e))?;
    Ok(())
}

impl LocatorState {
    pub fn upsert_placement(&mut self, record: ManagedPlacementRecord) {
        if let Some(existing) = self
            .placements
            .iter_mut()
            .find(|p| p.skill_id == record.skill_id && p.agent_key == record.agent_key)
        {
            *existing = record;
        } else {
            self.placements.push(record);
        }
    }

    pub fn remove_placement(&mut self, skill_id: &str, agent_key: &str) {
        self.placements
            .retain(|p| !(p.skill_id == skill_id && p.agent_key == agent_key));
    }

    pub fn find_placement(&self, skill_id: &str, agent_key: &str) -> Option<&ManagedPlacementRecord> {
        self.placements
            .iter()
            .find(|p| p.skill_id == skill_id && p.agent_key == agent_key)
    }
}
