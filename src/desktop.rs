//! Generate .desktop content from config.toml and write to XDG applications dir.
//! Values are escaped per the Desktop Entry spec (freedesktop.org) so newlines
//! and special characters cannot break the file or inject keys.

use anyhow::Result;
use std::path::Path;

use crate::config::Config;

#[cfg(unix)]
use nix::unistd::{chown, User};

/// Escape a string for use as a .desktop key value per the Desktop Entry spec.
/// Encodes `\` → `\\`, newline → `\n`, tab → `\t`, carriage return → `\r`.
/// Other control characters are replaced with space so they cannot inject keys.
pub fn escape_desktop_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Escape a single argument for the Exec key (Desktop Entry spec).
/// Escapes `\`, `"`, `` ` ``, `$`; if the result contains space or those chars, wraps in double quotes.
fn escape_exec_argument(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '`' => out.push_str("\\`"),
            '$' => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out
}

/// Format one Exec component: quote and escape if it contains space or special chars.
fn escape_for_exec_arg(s: &str) -> String {
    let escaped = escape_exec_argument(s);
    if escaped
        .chars()
        .any(|c| c == ' ' || c == '"' || c == '\\' || c == '`' || c == '$')
    {
        format!("\"{}\"", escaped)
    } else {
        escaped
    }
}

/// Build the Exec= line for a .desktop file: absolute path to the bundle executable
/// (or `aa-exec -p PROFILE -- /path` when confined). Uses canonical path when the executable exists.
fn build_exec_line(
    config: &crate::config::Config,
    bundle_root: &Path,
    profile_name: Option<&str>,
) -> String {
    let exec_path = bundle_root.join(&config.executable);
    let path_str = exec_path
        .canonicalize()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| exec_path.display().to_string());
    let confine = config
        .security
        .as_ref()
        .map(|s| s.confine)
        .unwrap_or(true);
    let mut parts: Vec<String> = if profile_name.is_some() && confine {
        let profile = profile_name.unwrap();
        vec![
            "aa-exec".into(),
            "-p".into(),
            profile.into(),
            "--".into(),
            escape_for_exec_arg(&path_str),
        ]
    } else {
        vec![escape_for_exec_arg(&path_str)]
    };
    for arg in &config.args {
        parts.push(escape_for_exec_arg(arg));
    }
    parts.push("%u".into());
    parts.join(" ")
}

/// User applications dir (XDG_DATA_HOME/applications). Used for user-tier .desktop files.
pub fn user_applications_dir() -> Result<std::path::PathBuf> {
    let dir = xdg::BaseDirectories::with_prefix("")?
        .get_data_home()
        .join("applications");
    Ok(dir)
}

/// System applications dir (/usr/share/applications). Used for system-tier .desktop files; requires root.
pub fn system_applications_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/usr/share/applications")
}

/// Generate .desktop file content for an app. Exec is the absolute path to the bundle executable
/// (or `aa-exec -p PROFILE -- /path` when confined), so the launcher's process is the app, not dotlnx.
/// All user-controlled values (name, comment, icon, categories) are escaped.
/// If `icon` is a relative path under the bundle, it is resolved to an absolute path.
/// When `profile_name` is Some and [security] confine is true, Exec uses aa-exec for AppArmor.
pub fn generate_desktop(
    config: &Config,
    bundle_root: &Path,
    profile_name: Option<&str>,
) -> String {
    let name = escape_desktop_value(&config.name);
    let exec = build_exec_line(config, bundle_root, profile_name);
    let mut out = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={}\n\
         Exec={}\n",
        name, exec
    );
    if let Some(ref workdir) = config.working_dir {
        let path_abs = bundle_root.join(workdir).display().to_string();
        out.push_str(&format!("Path={}\n", escape_desktop_value(&path_abs)));
    }
    if let Some(ref comment) = config.comment {
        out.push_str(&format!("Comment={}\n", escape_desktop_value(comment)));
    }
    if let Some(ref icon) = config.icon {
        let icon_value = resolve_icon_for_desktop(icon, Some(bundle_root));
        out.push_str(&format!("Icon={}\n", escape_desktop_value(&icon_value)));
    }
    if let Some(ref cats) = config.categories {
        let escaped: Vec<String> = cats.iter().map(|s| escape_desktop_value(s)).collect();
        out.push_str(&format!("Categories={}\n", escaped.join(";")));
    }
    out
}

/// Resolve icon value for the Icon= line. If bundle_root is set and icon is a relative path
/// pointing to an existing file in the bundle, return its absolute path; otherwise return icon as-is
/// (theme name or absolute path from config).
fn resolve_icon_for_desktop(icon: &str, bundle_root: Option<&Path>) -> String {
    if icon.is_empty() {
        return icon.to_string();
    }
    if let Some(root) = bundle_root {
        // Relative path: resolve against bundle root so the .desktop file gets an absolute path
        if !icon.starts_with('/') && !icon.starts_with("~/") {
            let resolved = root.join(icon);
            if resolved.is_file() {
                if let Ok(abs) = resolved.canonicalize() {
                    if let Some(s) = abs.to_str() {
                        return s.to_string();
                    }
                }
            }
        }
    }
    icon.to_string()
}

/// Remove the .directory file from the bundle (inverse of write_bundle_directory_file).
pub fn remove_bundle_directory_file(bundle_root: &Path) -> Result<()> {
    let path = bundle_root.join(".directory");
    if path.is_file() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Write a .directory file inside the bundle so file managers (e.g. Dolphin) show the app icon on the .lnx folder.
pub fn write_bundle_directory_file(bundle_root: &Path, config: &Config) -> Result<()> {
    let Some(ref icon) = config.icon else {
        return Ok(());
    };
    let icon_value = resolve_icon_for_desktop(icon, Some(bundle_root));
    let name = escape_desktop_value(&config.name);
    let content = format!(
        "[Desktop Entry]\n\
         Type=Directory\n\
         Name={}\n\
         Icon={}\n",
        name,
        escape_desktop_value(&icon_value)
    );
    std::fs::write(bundle_root.join(".directory"), content)?;
    Ok(())
}

/// Set GNOME/Nautilus folder icon via gio (metadata::custom-icon). Uses the user's D-Bus session
/// when run_as_user is Some so gvfsd-metadata receives the write (required when sync runs as root).
#[cfg(unix)]
pub fn set_gnome_folder_icon(
    bundle_root: &Path,
    config: &Config,
    run_as_user: Option<&str>,
) -> Result<()> {
    let Some(ref icon) = config.icon else {
        return Ok(());
    };
    let icon_value = resolve_icon_for_desktop(icon, Some(bundle_root));
    if !icon_value.starts_with('/') {
        return Ok(());
    }
    let file_url = format!("file://{}", icon_value.replace(' ', "%20"));
    let bundle_str = bundle_root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("bundle path not UTF-8"))?;
    let gio_path = "/usr/bin/gio";
    if !std::path::Path::new(gio_path).exists() {
        return Ok(());
    }
    let mut cmd = if let Some(username) = run_as_user {
        let uid = User::from_name(username).ok().flatten().map(|u| u.uid.as_raw());
        let (dbus_addr, xdg_runtime) = uid.map(|uid| {
            let bus = format!("/run/user/{}/bus", uid);
            let runtime = format!("/run/user/{}", uid);
            (
                std::path::Path::new(&bus).exists().then(|| bus),
                runtime,
            )
        }).unwrap_or((None, String::new()));
        let mut c = std::process::Command::new("runuser");
        c.args(["-u", username, "--", "env"]);
        if let Some(ref bus) = dbus_addr {
            c.arg(format!("DBUS_SESSION_BUS_ADDRESS=unix:path={}", bus));
            c.arg(format!("XDG_RUNTIME_DIR={}", xdg_runtime));
        }
        c.arg(gio_path)
            .args(["set", "-t", "string", bundle_str, "metadata::custom-icon"])
            .arg(&file_url);
        c
    } else {
        let mut c = std::process::Command::new(gio_path);
        c.args(["set", "-t", "string", bundle_str, "metadata::custom-icon"])
            .arg(&file_url);
        c
    };
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(not(unix))]
pub fn set_gnome_folder_icon(
    _bundle_root: &Path,
    _config: &Config,
    _run_as_user: Option<&str>,
) -> Result<()> {
    Ok(())
}

/// Clear GNOME folder icon (metadata::custom-icon). Uses user's D-Bus session when run_as_user is Some.
#[cfg(unix)]
pub fn clear_gnome_folder_icon(bundle_root: &Path, run_as_user: Option<&str>) -> Result<()> {
    let bundle_str = bundle_root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("bundle path not UTF-8"))?;
    let gio_path = "/usr/bin/gio";
    if !std::path::Path::new(gio_path).exists() {
        return Ok(());
    }
    let mut cmd = if let Some(username) = run_as_user {
        let uid = User::from_name(username).ok().flatten().map(|u| u.uid.as_raw());
        let (dbus_addr, xdg_runtime) = uid.map(|uid| {
            let bus = format!("/run/user/{}/bus", uid);
            let runtime = format!("/run/user/{}", uid);
            (
                std::path::Path::new(&bus).exists().then(|| bus),
                runtime,
            )
        }).unwrap_or((None, String::new()));
        let mut c = std::process::Command::new("runuser");
        c.args(["-u", username, "--", "env"]);
        if let Some(ref bus) = dbus_addr {
            c.arg(format!("DBUS_SESSION_BUS_ADDRESS=unix:path={}", bus));
            c.arg(format!("XDG_RUNTIME_DIR={}", xdg_runtime));
        }
        c.arg(gio_path)
            .args(["set", "-t", "unset", bundle_str, "metadata::custom-icon"]);
        c
    } else {
        let mut c = std::process::Command::new(gio_path);
        c.args(["set", "-t", "unset", bundle_str, "metadata::custom-icon"]);
        c
    };
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(not(unix))]
pub fn clear_gnome_folder_icon(_bundle_root: &Path, _run_as_user: Option<&str>) -> Result<()> {
    Ok(())
}

/// Write generated .desktop to the given applications directory.
/// Returns the path of the created file so the caller can chown when needed.
/// Exec is the absolute path to the bundle executable (or aa-exec ... when confined).
/// Pass `profile_name` when AppArmor is in use and [security] confine is true.
pub fn install_desktop(
    apps_dir: &Path,
    config: &Config,
    bundle_root: &Path,
    profile_name: Option<&str>,
) -> Result<std::path::PathBuf> {
    let name = format!("dotlnx-{}.desktop", config.name);
    let path = apps_dir.join(&name);
    let content = generate_desktop(config, bundle_root, profile_name);
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Change ownership of a path to the given username (uid:gid). Used when root creates
/// .desktop files in a user's applications dir so the user owns the file.
#[cfg(unix)]
pub fn chown_to_user(path: &Path, username: &str) -> Result<()> {
    let user = User::from_name(username)
        .map_err(|e| anyhow::anyhow!("lookup user {:?}: {}", username, e))?
        .ok_or_else(|| anyhow::anyhow!("no such user: {:?}", username))?;
    chown(path, Some(user.uid), Some(user.gid))
        .map_err(|e| anyhow::anyhow!("chown {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn minimal_config() -> Config {
        Config {
            name: "myapp".into(),
            executable: "bin/myapp".into(),
            args: vec![],
            env: vec![],
            working_dir: None,
            icon: None,
            comment: None,
            categories: None,
            security: None,
        }
    }

    #[test]
    fn generate_desktop_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let cfg = minimal_config();
        let out = generate_desktop(&cfg, &bundle, None);
        assert!(out.contains("[Desktop Entry]"));
        assert!(out.contains("Name=myapp"));
        let exec_line = out.lines().find(|l| l.starts_with("Exec=")).unwrap();
        assert!(exec_line.contains("bin/myapp"), "Exec should contain bundle path: {}", exec_line);
        assert!(exec_line.ends_with("%u"));
        assert!(out.contains("Type=Application"));
    }

    #[test]
    fn generate_desktop_with_profile_uses_aa_exec() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let cfg = minimal_config();
        let out = generate_desktop(&cfg, &bundle, Some("dotlnx-user-myapp"));
        let exec_line = out.lines().find(|l| l.starts_with("Exec=")).unwrap();
        assert!(exec_line.starts_with("Exec=aa-exec -p dotlnx-user-myapp -- "));
        assert!(exec_line.contains("bin/myapp"));
    }

    #[test]
    fn generate_desktop_escapes_exec_args() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let mut cfg = minimal_config();
        cfg.args = vec!["--path=/foo bar".into()];
        let out = generate_desktop(&cfg, &bundle, None);
        let exec_line = out.lines().find(|l| l.starts_with("Exec=")).unwrap();
        assert!(exec_line.contains("%u"));
        // Path and args with spaces must be quoted in Exec
        assert!(exec_line.contains("bin/myapp"));
    }

    #[test]
    fn generate_desktop_optional_fields() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let mut cfg = minimal_config();
        cfg.comment = Some("A test app".into());
        cfg.icon = Some("myapp".into());
        cfg.categories = Some(vec!["Utility".into()]);
        let out = generate_desktop(&cfg, &bundle, None);
        assert!(out.contains("Comment=A test app"));
        assert!(out.contains("Icon=myapp"));
        assert!(out.contains("Categories=Utility"));
    }

    #[test]
    fn generate_desktop_resolves_bundle_relative_icon() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("icon.png"), b"").unwrap();
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let mut cfg = minimal_config();
        cfg.icon = Some("icon.png".into());
        let out = generate_desktop(&cfg, &bundle, None);
        let icon_line = out.lines().find(|l| l.starts_with("Icon=")).unwrap();
        // Relative path in bundle should become absolute so the desktop can load it
        assert!(
            icon_line.starts_with("Icon=/"),
            "Icon should be absolute path, got: {}",
            icon_line
        );
        assert!(icon_line.contains("icon.png"));
    }

    #[test]
    fn install_and_uninstall_desktop() {
        let dir = tempfile::tempdir().unwrap();
        let apps_dir = dir.path();
        let bundle = dir.path().join("myapp.lnx");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/myapp"), b"").unwrap();
        let cfg = minimal_config();
        let desktop_path = install_desktop(apps_dir, &cfg, &bundle, None).unwrap();
        assert!(desktop_path.exists());
        let content = std::fs::read_to_string(&desktop_path).unwrap();
        assert!(content.contains("Name=myapp"));

        uninstall_desktop(apps_dir, "myapp").unwrap();
        assert!(!desktop_path.exists());
    }

    #[test]
    fn uninstall_desktop_nonexistent_ok() {
        let dir = tempfile::tempdir().unwrap();
        uninstall_desktop(dir.path(), "nonexistent").unwrap();
    }
}

/// Remove .desktop file for an app by name from the given applications directory.
/// Resolved path must stay under apps_dir to prevent path traversal.
pub fn uninstall_desktop(apps_dir: &Path, name: &str) -> Result<()> {
    let path = apps_dir.join(format!("dotlnx-{}.desktop", name));
    if path.exists() {
        if !apps_dir.exists() {
            anyhow::bail!("applications dir does not exist");
        }
        let apps_canon = std::fs::canonicalize(apps_dir)
            .map_err(|e| anyhow::anyhow!("applications dir: {}", e))?;
        let path_canon = std::fs::canonicalize(&path).map_err(|e| anyhow::anyhow!("{}", e))?;
        if !path_canon.starts_with(&apps_canon) || !path_canon.is_file() {
            anyhow::bail!("refusing to remove path outside applications dir");
        }
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
