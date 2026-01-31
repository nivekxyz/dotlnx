//! Validate .lnx bundle: layout, config.toml, executable path.

use anyhow::Result;
use std::path::Path;

use crate::bundle;
use crate::config;

/// Reject paths that could escape the bundle (absolute or containing "..").
fn path_stays_in_bundle(relative_path: &str) -> Result<()> {
    if relative_path.is_empty() {
        anyhow::bail!("path must not be empty");
    }
    if relative_path.starts_with('/') {
        anyhow::bail!("path must be relative to bundle (no leading /)");
    }
    for component in Path::new(relative_path).components() {
        if matches!(component, std::path::Component::ParentDir) {
            anyhow::bail!("path must not contain ..");
        }
    }
    Ok(())
}

/// Ensure resolved path is under bundle_root (canonicalize and check prefix).
pub fn path_under_bundle(resolved: &Path, bundle_root: &Path) -> Result<()> {
    let bundle_canon = std::fs::canonicalize(bundle_root)
        .map_err(|e| anyhow::anyhow!("bundle path: {}", e))?;
    let resolved_canon = std::fs::canonicalize(resolved).map_err(|e| anyhow::anyhow!("{}", e))?;
    if !resolved_canon.starts_with(&bundle_canon) {
        anyhow::bail!(
            "path {} is outside bundle {}",
            resolved.display(),
            bundle_root.display()
        );
    }
    Ok(())
}

/// Reject strings that contain control characters (would require escaping in .desktop; validate for clarity).
fn validate_desktop_string(label: &str, s: &str) -> Result<()> {
    if s.chars().any(|c| c.is_control()) {
        anyhow::bail!(
            "config.toml: {} must not contain control characters (newline, tab, etc.)",
            label
        );
    }
    Ok(())
}

/// Reject security paths that could break AppArmor profile or are ambiguous (e.g. "..", "#").
fn validate_security_path(label: &str, p: &str) -> Result<()> {
    if p.is_empty() {
        anyhow::bail!("config.toml: security path must not be empty");
    }
    if p.contains('#') {
        anyhow::bail!(
            "config.toml: security {} must not contain # (AppArmor comment character)",
            label
        );
    }
    if p.contains('\n') || p.contains('\r') || p.chars().any(|c| c.is_control()) {
        anyhow::bail!(
            "config.toml: security {} must not contain newlines or control characters",
            label
        );
    }
    for component in Path::new(p).components() {
        if matches!(component, std::path::Component::ParentDir) {
            anyhow::bail!(
                "config.toml: security {} must not contain .. (path is used literally in AppArmor)",
                label
            );
        }
    }
    Ok(())
}

/// Validate a single .lnx bundle at the given path.
pub fn validate_bundle(bundle_root: &Path) -> Result<()> {
    if !bundle::is_lnx_bundle(bundle_root) {
        anyhow::bail!("not a .lnx bundle: {}", bundle_root.display());
    }
    let cfg = config::load(bundle_root)?;
    if cfg.name.is_empty() {
        anyhow::bail!("config.toml: name is required");
    }
    validate_app_name(&cfg.name)?;
    if cfg.executable.is_empty() {
        anyhow::bail!("config.toml: executable is required");
    }
    path_stays_in_bundle(&cfg.executable)?;
    let exe_path = bundle_root.join(&cfg.executable);
    if !exe_path.exists() {
        anyhow::bail!("executable not found: {}", exe_path.display());
    }
    path_under_bundle(&exe_path, bundle_root)?;
    if let Some(ref wd) = cfg.working_dir {
        path_stays_in_bundle(wd)?;
    }
    if let Some(ref comment) = cfg.comment {
        validate_desktop_string("comment", comment)?;
    }
    if let Some(ref icon) = cfg.icon {
        validate_desktop_string("icon", icon)?;
    }
    if let Some(ref cats) = cfg.categories {
        for (i, c) in cats.iter().enumerate() {
            validate_desktop_string(&format!("categories[{}]", i), c)?;
        }
    }
    if let Some(ref sec) = cfg.security {
        for (i, p) in sec.read_paths.iter().enumerate() {
            validate_security_path(&format!("read_paths[{}]", i), p)?;
        }
        for (i, p) in sec.write_paths.iter().enumerate() {
            validate_security_path(&format!("write_paths[{}]", i), p)?;
        }
    }
    Ok(())
}

/// App name must be safe for profile names and .desktop Exec (no path sep, no injection chars).
pub fn validate_app_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("app name must not be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        anyhow::bail!("app name must not contain path separators or ..");
    }
    if name.contains(';') || name.contains('\n') || name.contains('\r') {
        anyhow::bail!("app name must not contain ; or newlines (desktop Exec safety)");
    }
    if name.chars().any(|c| c.is_control()) {
        anyhow::bail!("app name must not contain control characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn validate_app_name_ok() {
        assert!(validate_app_name("myapp").is_ok());
        assert!(validate_app_name("my-app_123").is_ok());
    }

    #[test]
    fn validate_app_name_rejects_invalid() {
        assert!(validate_app_name("").is_err());
        assert!(validate_app_name("a/b").is_err());
        assert!(validate_app_name("a\\b").is_err());
        assert!(validate_app_name("a..b").is_err());
        assert!(validate_app_name("a;b").is_err());
        assert!(validate_app_name("a\nb").is_err());
    }

    #[test]
    fn path_under_bundle_ok() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path();
        let sub = bundle.join("bin").join("app");
        std::fs::create_dir_all(sub.parent().unwrap()).unwrap();
        std::fs::write(&sub, "binary").unwrap();
        assert!(path_under_bundle(&sub, bundle).is_ok());
    }

    #[test]
    fn path_under_bundle_outside_err() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let outside = other.path().join("file");
        std::fs::write(&outside, "x").unwrap();
        assert!(path_under_bundle(&outside, dir.path()).is_err());
    }

    fn make_valid_bundle(root: &Path, name: &str, executable: &str) {
        std::fs::create_dir_all(root.join(Path::new(executable).parent().unwrap_or(Path::new("."))))
            .unwrap();
        std::fs::write(root.join(executable), "#!/bin/sh\nexit 0").unwrap();
        std::fs::write(
            root.join("config.toml"),
            &format!(
                r#"
name = "{}"
executable = "{}"
"#,
                name, executable
            ),
        )
        .unwrap();
    }

    #[test]
    fn validate_bundle_ok() {
        let parent = tempfile::tempdir().unwrap();
        let bundle = parent.path().join("myapp.lnx");
        std::fs::create_dir_all(&bundle).unwrap();
        make_valid_bundle(&bundle, "myapp", "bin/myapp");
        assert!(validate_bundle(&bundle).is_ok());
    }

    #[test]
    fn validate_bundle_not_lnx_dir_err() {
        let dir = tempfile::tempdir().unwrap();
        make_valid_bundle(dir.path(), "x", "bin/x");
        let err = validate_bundle(dir.path()).unwrap_err();
        assert!(err.to_string().contains("not a .lnx bundle"));
    }

    #[test]
    fn validate_bundle_missing_executable_err() {
        let parent = tempfile::tempdir().unwrap();
        let bundle = parent.path().join("myapp.lnx");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("config.toml"),
            r#"name = "myapp"
executable = "bin/nonexistent"
"#,
        )
        .unwrap();
        let err = validate_bundle(&bundle).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("executable"));
    }

    #[test]
    fn validate_bundle_bad_app_name_err() {
        let parent = tempfile::tempdir().unwrap();
        let bundle = parent.path().join("myapp.lnx");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("config.toml"), r#"name = "bad/name"\nexecutable = "bin/app"\n"#).unwrap();
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::write(bundle.join("bin/app"), "x").unwrap();
        let err = validate_bundle(&bundle).unwrap_err();
        assert!(err.to_string().contains("name"));
    }
}

/// Validate one or more .lnx bundles (path can be a .lnx dir or a dir containing .lnx dirs).
pub fn run(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("path does not exist: {}", path.display());
    }
    let mut bundles = Vec::new();
    if bundle::is_lnx_bundle(path) {
        bundles.push(path.to_path_buf());
    } else if path.is_dir() {
        bundles = bundle::discover_lnx_dirs(path);
    } else {
        anyhow::bail!("path is not a .lnx bundle or directory: {}", path.display());
    }
    if bundles.is_empty() {
        anyhow::bail!("no .lnx bundles found at {}", path.display());
    }
    for b in &bundles {
        validate_bundle(b)?;
    }
    Ok(())
}
