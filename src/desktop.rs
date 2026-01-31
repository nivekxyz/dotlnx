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

/// Escape app name for use inside double quotes in the Exec key (Desktop Entry spec).
/// Escapes `\`, `"`, `` ` ``, `$` so the name is always one argument.
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

/// Generate .desktop file content for an app. Exec is always `dotlnx run "<name>" %u`.
/// The name is quoted so it is always parsed as a single argument (required when name contains spaces).
/// All user-controlled values (name, comment, icon, categories) are escaped.
pub fn generate_desktop(config: &Config) -> String {
    let name = escape_desktop_value(&config.name);
    let exec_arg = escape_exec_argument(&config.name);
    let exec = format!("dotlnx run \"{}\" %u", exec_arg);
    let mut out = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={}\n\
         Exec={}\n",
        name, exec
    );
    if let Some(ref comment) = config.comment {
        out.push_str(&format!("Comment={}\n", escape_desktop_value(comment)));
    }
    if let Some(ref icon) = config.icon {
        out.push_str(&format!("Icon={}\n", escape_desktop_value(icon)));
    }
    if let Some(ref cats) = config.categories {
        let escaped: Vec<String> = cats.iter().map(|s| escape_desktop_value(s)).collect();
        out.push_str(&format!("Categories={}\n", escaped.join(";")));
    }
    out
}

/// Write generated .desktop to the given applications directory.
/// Returns the path of the created file so the caller can chown when needed.
pub fn install_desktop(apps_dir: &Path, config: &Config) -> Result<std::path::PathBuf> {
    let name = format!("dotlnx-{}.desktop", config.name);
    let path = apps_dir.join(&name);
    let content = generate_desktop(config);
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
        let cfg = minimal_config();
        let out = generate_desktop(&cfg);
        assert!(out.contains("[Desktop Entry]"));
        assert!(out.contains("Name=myapp"));
        assert!(out.contains("Exec=dotlnx run \"myapp\" %u"));
        assert!(out.contains("Type=Application"));
    }

    #[test]
    fn generate_desktop_escapes_name() {
        let mut cfg = minimal_config();
        cfg.name = "App \"With\" Quotes".into();
        let out = generate_desktop(&cfg);
        assert!(out.contains("Exec=dotlnx run \"App \\\"With\\\" Quotes\" %u"));
    }

    #[test]
    fn generate_desktop_optional_fields() {
        let mut cfg = minimal_config();
        cfg.comment = Some("A test app".into());
        cfg.icon = Some("myapp".into());
        cfg.categories = Some(vec!["Utility".into()]);
        let out = generate_desktop(&cfg);
        assert!(out.contains("Comment=A test app"));
        assert!(out.contains("Icon=myapp"));
        assert!(out.contains("Categories=Utility"));
    }

    #[test]
    fn install_and_uninstall_desktop() {
        let dir = tempfile::tempdir().unwrap();
        let apps_dir = dir.path();
        let cfg = minimal_config();
        let desktop_path = install_desktop(apps_dir, &cfg).unwrap();
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
