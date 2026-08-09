use std::path::Path;

use anyhow::Result;

use crate::config::{self, GlobalConfig, WikiEntry};
use crate::engine::WikiEngine;
use crate::registry;

/// Create a wiki and hot-reload it into the running engine.
#[allow(clippy::too_many_arguments)]
pub fn admin_create(
    path: &Path,
    name: &str,
    description: Option<&str>,
    force: bool,
    set_default: bool,
    config_path: &Path,
    engine: Option<&WikiEngine>,
    content_root: Option<&str>,
) -> Result<registry::CreateReport> {
    let report = registry::create(
        path,
        name,
        description,
        force,
        set_default,
        config_path,
        content_root,
    )?;

    // Hot reload: mount the new wiki in the running engine
    if report.registered
        && let Some(engine) = engine
    {
        let entry = WikiEntry {
            name: name.to_string(),
            path: report.path.clone(),
            description: description.map(|s| s.to_string()),
            remote: None,
        };
        engine.mount_wiki(&entry)?;
    }

    Ok(report)
}

/// Register an existing wiki and hot-reload it into the running engine.
pub fn admin_register(
    path: &Path,
    name: &str,
    description: Option<&str>,
    content_root: Option<&str>,
    config_path: &Path,
    engine: Option<&WikiEngine>,
) -> Result<registry::RegisterReport> {
    let report = registry::register_existing(path, name, description, content_root, config_path)?;

    if report.registered
        && let Some(engine) = engine
    {
        let entry = WikiEntry {
            name: name.to_string(),
            path: report.path.clone(),
            description: description.map(|s| s.to_string()),
            remote: None,
        };
        engine.mount_wiki(&entry)?;
    }

    Ok(report)
}

/// List registered wikis, optionally filtered to a single name.
pub fn admin_list(config: &GlobalConfig, name: Option<&str>) -> Vec<config::WikiEntry> {
    let all = registry::load_all(config);
    match name {
        Some(n) => all.into_iter().filter(|e| e.name == n).collect(),
        None => all,
    }
}

/// Unmount a wiki from the engine and remove it from config.
pub fn admin_remove(
    name: &str,
    delete: bool,
    config_path: &Path,
    engine: Option<&WikiEngine>,
) -> Result<()> {
    // Hot reload: unmount before removing from config
    if let Some(engine) = engine {
        engine.unmount_wiki(name)?;
    }
    registry::remove(name, delete, config_path)
}

/// Set the default wiki in config and update the running engine.
pub fn admin_set_default(
    name: &str,
    config_path: &Path,
    engine: Option<&WikiEngine>,
) -> Result<()> {
    registry::set_default_wiki(name, config_path)?;

    // Hot reload: update default in the running engine
    if let Some(engine) = engine {
        engine.set_default(name)?;
    }
    Ok(())
}
