//! Bundler: create .lnx bundle scaffolds (appimage, bin/script/binary, etc.).

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::validate;

/// Slugify app name for directory: lowercase, spaces to hyphens, drop non-alphanumeric.
pub fn slugify_app_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() {
                Some(if c.is_ascii_alphabetic() {
                (c as u8).to_ascii_lowercase() as char
            } else {
                c
            })
            } else if c == ' ' || c == '-' || c == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect();
    let s: String = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let s = s.trim_matches('-');
    if s.is_empty() {
        "app".to_string()
    } else {
        s.to_string()
    }
}

/// Derive a glob pattern from an AppImage path so run.sh can pick the newest of multiple versions.
/// E.g. "Cursor-0.1.0-x86_64.appimage" -> "Cursor-*-x86_64.appimage".
pub fn derive_appimage_pattern(appimage_path: &Path) -> String {
    let name = appimage_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("*.appimage");
    if !name.ends_with(".appimage") {
        return "*.appimage".to_string();
    }
    // Replace first version-like segment (digits and dots) with *
    let base = &name[..name.len() - ".appimage".len()];
    let mut in_version = false;
    let mut version_start = 0;
    for (i, c) in base.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            if !in_version {
                in_version = true;
                version_start = i;
            }
        } else {
            if in_version {
                // Replace segment [version_start, i) with *
                return format!(
                    "{}*{}.appimage",
                    &base[..version_start],
                    &base[i..]
                );
            }
        }
    }
    if in_version {
        return format!("{}*.appimage", &base[..version_start]);
    }
    "*.appimage".to_string()
}

/// Escape for use inside a bash double-quoted string (backslash and double-quote).
fn escape_bash_double_quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Generate run.sh content for an appimage bundle: finds latest matching appimage in bin/ and execs it.
fn run_sh_appimage(app_name: &str, appimage_pattern: &str) -> String {
    let name_escaped = escape_bash_double_quoted(app_name);
    format!(
        r#"#!/usr/bin/env bash

APPIMAGE="{pattern}"

set -e
cd "$(dirname "$0")"
latest=$(ls bin/$APPIMAGE 2>/dev/null | sort -V | tail -1)
if [[ -z "$latest" ]]; then
  echo "No {name} appimage (bin/$APPIMAGE) found in $(pwd)" >&2
  exit 1
fi
exec "$(pwd)/bin/$latest" "$@"
"#,
        pattern = appimage_pattern,
        name = name_escaped
    )
}

/// Create an appimage-type .lnx bundle: bin/ (AppImage copied in), config.toml, run.sh, assets/.
pub fn create_appimage_bundle(
    app_name: &str,
    appimage_path: &Path,
    output_dir: &Path,
) -> Result<PathBuf> {
    let dir_name = format!("{}.lnx", app_name.trim());
    let bundle_root = output_dir.join(&dir_name);

    if bundle_root.exists() {
        anyhow::bail!(
            "bundle directory already exists: {}",
            bundle_root.display()
        );
    }

    if !appimage_path.exists() {
        anyhow::bail!("AppImage not found: {}", appimage_path.display());
    }
    if !appimage_path.is_file() {
        anyhow::bail!("AppImage path is not a file: {}", appimage_path.display());
    }

    let bin_dir = bundle_root.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    std::fs::create_dir_all(bundle_root.join("assets"))?;

    let filename = appimage_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app.appimage");
    let dest = bin_dir.join(filename);
    std::fs::copy(appimage_path, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }

    let pattern = derive_appimage_pattern(appimage_path);
    let run_sh = run_sh_appimage(app_name, &pattern);
    let run_sh_path = bundle_root.join("run.sh");
    std::fs::write(&run_sh_path, run_sh)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&run_sh_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&run_sh_path, perms)?;
    }

    let config_toml = format!(
        r#"# dotlnx bundle: {}
# bin/ (AppImage copied in). run.sh launches the newest in bin/. Drop icon.png into assets/.

name = "{}"
executable = "run.sh"
icon = "assets/icon.png"
"#,
        app_name,
        app_name.replace('"', "\\\"")
    );
    std::fs::write(bundle_root.join("config.toml"), config_toml)?;

    Ok(bundle_root)
}

/// Create a bin-type .lnx bundle: bin/ (script or binary copied in), config.toml, assets/. That file is the executable (no run.sh). Copied file is chmod +x on Unix.
pub fn create_bin_bundle(
    app_name: &str,
    executable_path: &Path,
    output_dir: &Path,
) -> Result<PathBuf> {
    let dir_name = format!("{}.lnx", app_name.trim());
    let bundle_root = output_dir.join(&dir_name);

    if bundle_root.exists() {
        anyhow::bail!(
            "bundle directory already exists: {}",
            bundle_root.display()
        );
    }

    if !executable_path.exists() {
        anyhow::bail!("executable not found: {}", executable_path.display());
    }
    if !executable_path.is_file() {
        anyhow::bail!("executable path is not a file: {}", executable_path.display());
    }

    let bin_dir = bundle_root.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    std::fs::create_dir_all(bundle_root.join("assets"))?;

    let filename = executable_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("run");
    let dest = bin_dir.join(filename);
    std::fs::copy(executable_path, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }

    let executable = format!("bin/{}", filename);
    let config_toml = format!(
        r#"# dotlnx bundle: {}
# bin/ (script or binary copied in). That file is the executable. Drop icon.png into assets/.

name = "{}"
executable = "{}"
icon = "assets/icon.png"
"#,
        app_name,
        app_name.replace('"', "\\\""),
        executable
    );
    std::fs::write(bundle_root.join("config.toml"), config_toml)?;

    Ok(bundle_root)
}

/// Entry point for `dotlnx bundle --appname "..." --appimage <path>` or `--bin <path>`.
pub fn run(
    appname: &str,
    appimage: Option<&Path>,
    bin: Option<&Path>,
    output_dir: &Path,
) -> Result<()> {
    if appname.trim().is_empty() {
        anyhow::bail!("app name must not be empty");
    }
    validate::validate_app_name(appname)?;

    match (appimage, bin) {
        (Some(path), None) => {
            let bundle_root = create_appimage_bundle(appname, path, output_dir)?;
            tracing::info!(
                "Created {} with bin/ (AppImage copied in), config.toml, run.sh, and assets/. Add more AppImages to bin/ or assets/icon.png if desired, then run: dotlnx validate {}",
                bundle_root.display(),
                bundle_root.display()
            );
        }
        (None, Some(path)) => {
            let bundle_root = create_bin_bundle(appname, path, output_dir)?;
            tracing::info!(
                "Created {} with bin/ (executable copied in), config.toml, and assets/. Add assets/icon.png if desired, then run: dotlnx validate {}",
                bundle_root.display(),
                bundle_root.display()
            );
        }
        (None, None) => anyhow::bail!("specify exactly one of --appimage or --bin"),
        (Some(_), Some(_)) => anyhow::bail!("specify exactly one of --appimage or --bin"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_simple() {
        assert_eq!(slugify_app_name("My App"), "my-app");
        assert_eq!(slugify_app_name("Cursor"), "cursor");
    }

    #[test]
    fn slugify_spaces_and_special() {
        assert_eq!(slugify_app_name("App  Name"), "app-name");
        assert_eq!(slugify_app_name("  x  "), "x");
    }

    #[test]
    fn derive_pattern_version_in_middle() {
        let p = Path::new("/tmp/Cursor-0.1.0-x86_64.appimage");
        assert_eq!(derive_appimage_pattern(p), "Cursor-*-x86_64.appimage");
    }

    #[test]
    fn derive_pattern_simple() {
        let p = Path::new("foo.appimage");
        assert_eq!(derive_appimage_pattern(p), "*.appimage");
    }

    #[test]
    fn test_escape_bash_double_quoted() {
        assert_eq!(super::escape_bash_double_quoted("x"), "x");
        assert_eq!(super::escape_bash_double_quoted(r#"a"b"#), r#"a\"b"#);
        assert_eq!(super::escape_bash_double_quoted("a\\b"), "a\\\\b");
    }

    #[test]
    fn create_appimage_bundle_then_validate_passes() {
        let out = tempfile::tempdir().unwrap();
        let appimage = out.path().join("fake.appimage");
        std::fs::write(&appimage, b"fake").unwrap();
        let bundle_root = create_appimage_bundle("MyApp", &appimage, out.path()).unwrap();
        assert_eq!(
            bundle_root.file_name().and_then(|n| n.to_str()),
            Some("MyApp.lnx")
        );
        assert!(validate::validate_bundle(&bundle_root).is_ok());
    }

    #[test]
    fn create_bin_bundle_then_validate_passes() {
        let out = tempfile::tempdir().unwrap();
        let script = out.path().join("mytool.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0").unwrap();
        let bundle_root = create_bin_bundle("MyTool", &script, out.path()).unwrap();
        assert_eq!(
            bundle_root.file_name().and_then(|n| n.to_str()),
            Some("MyTool.lnx")
        );
        assert!(validate::validate_bundle(&bundle_root).is_ok());
    }

    #[test]
    fn bundle_dir_name_preserves_app_name() {
        let out = tempfile::tempdir().unwrap();
        let script = out.path().join("test.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0").unwrap();
        let bundle_root = create_bin_bundle("Test App", &script, out.path()).unwrap();
        assert_eq!(
            bundle_root.file_name().and_then(|n| n.to_str()),
            Some("Test App.lnx")
        );
        assert!(validate::validate_bundle(&bundle_root).is_ok());
    }

    #[test]
    fn run_empty_appname_bails() {
        let out = tempfile::tempdir().unwrap();
        let f = out.path().join("x.appimage");
        std::fs::write(&f, b"x").unwrap();
        let e = run("", Some(&f), None, out.path()).unwrap_err();
        assert!(e.to_string().to_lowercase().contains("empty"));
    }

    #[test]
    fn run_invalid_appname_bails() {
        let out = tempfile::tempdir().unwrap();
        let f = out.path().join("x.appimage");
        std::fs::write(&f, b"x").unwrap();
        let e = run("bad/name", Some(&f), None, out.path()).unwrap_err();
        assert!(e.to_string().contains("name"));
    }
}
