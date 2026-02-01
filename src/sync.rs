//! One-shot sync: scan app folders, validate, generate AppArmor, generate .desktop.
//! Used by the watch service and for scripts/CI.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn};

use crate::apparmor;
use crate::bundle;
use crate::config;
use crate::desktop;
use crate::validate;

/// Run full sync: make installed state match folders (add/update .lnx → install; remove .lnx → uninstall).
/// When root + SUDO_USER: sync invoking user only. When root (daemon): sync all users. When non-root: current user only.
pub fn run(dry_run: bool) -> Result<()> {
    let is_root = bundle::is_root();

    for (apps_dir, desktop_dir, username) in bundle::user_tier_entries()? {
        if apps_dir.exists() {
            sync_dir(
                &apps_dir,
                &desktop_dir,
                Tier::User(username),
                dry_run,
                is_root,
            )?;
        }
    }

    if is_root {
        let system_apps = bundle::system_applications_dir();
        if system_apps.exists() {
            sync_dir(
                &system_apps,
                &desktop::system_applications_dir(),
                Tier::System,
                dry_run,
                true,
            )?;
        }
    }
    Ok(())
}

enum Tier {
    User(String),
    System,
}

/// Sync a single Applications directory: discover .lnx, validate, install (desktop + AppArmor), then reconcile (uninstall removed).
fn sync_dir(
    apps_root: &Path,
    target_desktop_dir: &Path,
    tier: Tier,
    dry_run: bool,
    is_root: bool,
) -> Result<()> {
    let dirs = bundle::discover_lnx_dirs(apps_root);
    let mut current_names = HashSet::new();

    for dir in &dirs {
        if let Err(e) = validate::validate_bundle(dir) {
            warn!(bundle = %dir.display(), "skipping invalid bundle: {}", e);
            continue;
        }
        let cfg = match config::load(dir) {
            Ok(c) => c,
            Err(e) => {
                warn!(bundle = %dir.display(), "skipping bundle (config error): {}", e);
                continue;
            }
        };
        current_names.insert(cfg.name.clone());

        if dry_run {
            info!(
                app = %cfg.name,
                desktop = %target_desktop_dir.join(format!("dotlnx-{}.desktop", cfg.name)).display(),
                "would install"
            );
            continue;
        }

        std::fs::create_dir_all(target_desktop_dir)?;
        let desktop_path = desktop::install_desktop(target_desktop_dir, &cfg, Some(dir))?;
        #[cfg(unix)]
        if is_root {
            if let Tier::User(ref username) = tier {
                if let Err(e) = desktop::chown_to_user(&desktop_path, username) {
                    warn!(path = %desktop_path.display(), user = %username, "chown desktop to user: {}", e);
                }
            }
        }

        if let Err(e) = desktop::write_bundle_directory_file(dir, &cfg) {
            warn!(bundle = %dir.display(), "could not write .directory for folder icon: {}", e);
        }
        #[cfg(unix)]
        if is_root && cfg.icon.is_some() {
            if let Tier::User(ref username) = tier {
                let dir_file = dir.join(".directory");
                if dir_file.exists() {
                    if let Err(e) = desktop::chown_to_user(&dir_file, username) {
                        warn!(path = %dir_file.display(), user = %username, "chown .directory to user: {}", e);
                    }
                }
            }
        }
        let run_as_user = match &tier {
            Tier::User(u) if is_root => Some(u.as_str()),
            _ => None,
        };
        if let Err(e) = desktop::set_gnome_folder_icon(dir, &cfg, run_as_user) {
            warn!(bundle = %dir.display(), "could not set GNOME folder icon: {}", e);
        }

        if is_root {
            let confine = cfg.security.as_ref().map(|s| s.confine).unwrap_or(true);
            let profile_name = match &tier {
                Tier::User(u) => apparmor::profile_name_user(u, &cfg.name),
                Tier::System => apparmor::profile_name_system(&cfg.name),
            };
            if confine {
                let profile_content = apparmor::generate_profile(dir, &cfg, &profile_name);
                if let Err(e) = apparmor::load_profile(&profile_name, &profile_content) {
                    warn!(profile = %profile_name, "could not load AppArmor profile: {}", e);
                }
            } else {
                // App runs unconfined; remove profile if it existed (e.g. switched from confined)
                let _ = apparmor::unload_profile(&profile_name);
            }
        }
    }

    // Reconcile: uninstall desktops (and profiles) for apps no longer in the folder
    if !dry_run && target_desktop_dir.exists() {
        for entry in std::fs::read_dir(target_desktop_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.starts_with("dotlnx-") {
                continue;
            }
            let name = stem.strip_prefix("dotlnx-").unwrap_or(stem);
            if current_names.contains(name) {
                continue;
            }
            if validate::validate_app_name(name).is_err() {
                continue;
            }
            if let Err(e) = uninstall_one(target_desktop_dir, name, &tier, is_root) {
                warn!(app = %name, "uninstall failed: {}", e);
            }
        }
    }

    Ok(())
}

/// Uninstall a single app from a tier: remove desktop and (when root) AppArmor profile.
fn uninstall_one(
    target_desktop_dir: &Path,
    name: &str,
    tier: &Tier,
    is_root: bool,
) -> Result<()> {
    desktop::uninstall_desktop(target_desktop_dir, name)?;
    if is_root {
        let profile_name = match tier {
            Tier::User(u) => apparmor::profile_name_user(u, name),
            Tier::System => apparmor::profile_name_system(name),
        };
        apparmor::unload_profile(&profile_name)?;
    }
    Ok(())
}
