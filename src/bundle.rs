//! Bundle discovery: find .lnx directories in user and system Application dirs.

use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config;
use crate::desktop;

/// Path to scan for .lnx bundles (user tier). Uses DOTLNX_APPLICATIONS or ~/Applications.
pub fn user_applications_dir() -> PathBuf {
    std::env::var("DOTLNX_APPLICATIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Applications")
        })
}

/// System-wide Applications directory.
pub fn system_applications_dir() -> PathBuf {
    std::env::var("DOTLNX_SYSTEM_APPLICATIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Applications"))
}

/// Discover all .lnx directories under a root path (e.g. ~/Applications or /Applications).
pub fn discover_lnx_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    for entry in WalkDir::new(root)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if p.is_dir() {
            if let Some(ext) = p.extension() {
                if ext == "lnx" {
                    out.push(p.to_path_buf());
                }
            }
        }
    }
    out
}

/// Check if a path is a valid .lnx bundle root (directory name ends with .lnx).
pub fn is_lnx_bundle(path: &Path) -> bool {
    path.is_dir()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.ends_with(".lnx"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_lnx_dirs_finds_bundles() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path();
        std::fs::create_dir_all(apps.join("myapp.lnx")).unwrap();
        std::fs::create_dir_all(apps.join("other.lnx")).unwrap();
        std::fs::write(apps.join("not-bundle.txt"), "").unwrap();
        std::fs::create_dir_all(apps.join("plaindir")).unwrap();
        let found = discover_lnx_dirs(apps);
        assert_eq!(found.len(), 2);
        let names: Vec<_> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"myapp.lnx"));
        assert!(names.contains(&"other.lnx"));
    }

    #[test]
    fn discover_lnx_dirs_empty_for_nonexistent() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        assert!(discover_lnx_dirs(&missing).is_empty());
    }

    #[test]
    fn is_lnx_bundle_true() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("foo.lnx");
        std::fs::create_dir_all(&bundle).unwrap();
        assert!(is_lnx_bundle(&bundle));
    }

    #[test]
    fn is_lnx_bundle_false_for_file() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file.lnx");
        std::fs::write(&file, "").unwrap();
        assert!(!is_lnx_bundle(&file));
    }

    #[test]
    fn is_lnx_bundle_false_for_dir_without_lnx_suffix() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("plain");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_lnx_bundle(&dir));
    }

    #[test]
    fn username_from_bundle_path_linux_style() {
        let path = PathBuf::from("/home/alice/Applications/myapp.lnx");
        assert_eq!(username_from_bundle_path(&path).as_deref(), Some("alice"));
    }

    #[test]
    fn username_from_bundle_path_root_home() {
        let path = PathBuf::from("/root/Applications/myapp.lnx");
        assert_eq!(username_from_bundle_path(&path).as_deref(), Some("root"));
    }

    #[test]
    fn username_from_bundle_path_nested_returns_parent_of_apps() {
        let path = PathBuf::from("/home/bob/Applications/foo.lnx");
        assert_eq!(username_from_bundle_path(&path).as_deref(), Some("bob"));
    }

    #[test]
    fn resolve_bundle_by_name_underscore_fallback() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path();
        let bundle_dir = apps.join("My App.lnx");
        std::fs::create_dir_all(&bundle_dir).unwrap();
        std::fs::write(
            bundle_dir.join("config.toml"),
            r#"name = "My App"
executable = "bin/app"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(bundle_dir.join("bin")).unwrap();
        std::fs::write(bundle_dir.join("bin/app"), "#!/bin/sh\nexit 0").unwrap();

        let prev = std::env::var_os("DOTLNX_APPLICATIONS");
        std::env::set_var("DOTLNX_APPLICATIONS", apps);
        let result = resolve_bundle_by_name("My_App");
        match &prev {
            Some(v) => std::env::set_var("DOTLNX_APPLICATIONS", v),
            None => std::env::remove_var("DOTLNX_APPLICATIONS"),
        }

        let (path, cfg, _) = result.unwrap().unwrap();
        assert_eq!(cfg.name, "My App");
        assert!(path.ends_with("My App.lnx"));
    }
}

/// Resolve an app by name: user tier first (~/Applications), then system (/Applications).
/// Returns (bundle_path, config, is_user_tier). User tier wins when same name exists in both.
/// If the exact name is not found and the name contains underscores, also tries with underscores
/// replaced by spaces (some launchers incorrectly replace spaces with underscores in the Exec command).
pub fn resolve_bundle_by_name(name: &str) -> anyhow::Result<Option<(PathBuf, config::Config, bool)>> {
    if let Some(r) = resolve_bundle_by_name_exact(name)? {
        return Ok(Some(r));
    }
    if name.contains('_') {
        let name_with_spaces = name.replace('_', " ");
        if let Some(r) = resolve_bundle_by_name_exact(&name_with_spaces)? {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

fn resolve_bundle_by_name_exact(name: &str) -> anyhow::Result<Option<(PathBuf, config::Config, bool)>> {
    let user_root = user_applications_dir();
    for dir in discover_lnx_dirs(&user_root) {
        let cfg = match config::load(&dir) {
            Ok(c) => c,
            Err(_) => continue,
            };
        if cfg.name == name {
            return Ok(Some((dir, cfg, true)));
        }
    }
    let system_root = system_applications_dir();
    for dir in discover_lnx_dirs(&system_root) {
        let cfg = match config::load(&dir) {
            Ok(c) => c,
            Err(_) => continue,
            };
        if cfg.name == name {
            return Ok(Some((dir, cfg, false)));
        }
    }
    Ok(None)
}

/// Username for user-tier profile: derived from bundle path (e.g. /home/alice/Applications/foo.lnx -> alice).
pub fn username_from_bundle_path(bundle_path: &Path) -> Option<String> {
    let apps_dir = bundle_path.parent()?;
    let home = apps_dir.parent()?;
    home.file_name().and_then(|n| n.to_str().map(String::from))
}

/// True when running with effective uid 0 (root). On Unix uses geteuid(); otherwise falls back to USER.
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        nix::unistd::geteuid().is_root()
    }
    #[cfg(not(unix))]
    {
        std::env::var("USER").unwrap_or_default() == "root"
    }
}

/// User-tier entries (apps_dir, desktop_dir, username) for sync/watch.
/// When root + SUDO_USER: invoking user only. When root + no SUDO_USER (e.g. daemon): all users. When non-root: current user only.
/// Non-root uses XDG_DATA_HOME/applications for desktop_dir; root/daemon use default .local/share/applications per user.
pub fn user_tier_entries() -> Result<Vec<(PathBuf, PathBuf, String)>> {
    let is_root = is_root();

    if is_root {
        if let Ok(sudo_user) = std::env::var("SUDO_USER") {
            let home: PathBuf = if sudo_user == "root" {
                PathBuf::from("/root")
            } else {
                PathBuf::from("/home").join(&sudo_user)
            };
            let apps = home.join("Applications");
            let desktop = home.join(".local/share/applications");
            return Ok(vec![(apps, desktop, sudo_user)]);
        }
        // Daemon mode (no SUDO_USER): all users
        let mut entries = Vec::new();
        let root_home = PathBuf::from("/root");
        entries.push((
            root_home.join("Applications"),
            root_home.join(".local/share/applications"),
            "root".into(),
        ));
        if let Ok(rd) = std::fs::read_dir("/home") {
            for e in rd.filter_map(|e| e.ok()) {
                let path = e.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push((
                            path.join("Applications"),
                            path.join(".local/share/applications"),
                            name.to_string(),
                        ));
                    }
                }
            }
        }
        return Ok(entries);
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let apps = std::env::var("DOTLNX_APPLICATIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join("Applications"));
    let desktop_dir = desktop::user_applications_dir()?;
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    Ok(vec![(apps, desktop_dir, user)])
}
