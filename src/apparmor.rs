//! Generate AppArmor profile from config security section; load/unload via apparmor_parser.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::Config;

/// Locations to check for apparmor_parser (root/sudo/systemd often have minimal PATH without /usr/sbin).
const APPARMOR_PARSER_CANDIDATES: &[&str] = &["/usr/sbin/apparmor_parser", "/sbin/apparmor_parser"];

/// Resolve path to apparmor_parser: check /usr/sbin and /sbin first, then PATH.
fn find_apparmor_parser() -> Option<PathBuf> {
    for p in APPARMOR_PARSER_CANDIDATES {
        let path = Path::new(p);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join("apparmor_parser");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Sanitize path for AppArmor rule: strip comments (#), no newline, no comma (would break profile).
fn sanitize_apparmor_path(p: &str) -> String {
    let without_comment = p.split('#').next().unwrap_or(p).trim();
    without_comment
        .replace(['\n', '\r', ','], " ")
        .trim()
        .to_string()
}

/// Quote a path for use in an AppArmor file rule if it contains spaces or other characters that
/// would break the lexer. Rules with embedded spaces or tabs must be quoted per apparmor.d(5).
fn quote_path_for_apparmor(path: &str) -> String {
    let needs_quote = path
        .chars()
        .any(|c| c == ' ' || c == '\t' || c == '"' || c == '\\');
    if !needs_quote {
        return path.to_string();
    }
    let mut escaped = String::with_capacity(path.len() + 8);
    for c in path.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            c => escaped.push(c),
        }
    }
    format!("\"{}\"", escaped)
}

/// Sanitize a segment for use in profile name (no path sep, no ..). Keeps alphanumeric, -, _.
fn sanitize_profile_segment(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Profile name for user tier: dotlnx-<username>-<name> (avoids collision across users).
pub fn profile_name_user(username: &str, app_name: &str) -> String {
    format!(
        "dotlnx-{}-{}",
        sanitize_profile_segment(username),
        sanitize_profile_segment(app_name)
    )
}

/// Profile name for system tier: dotlnx-<name>.
pub fn profile_name_system(app_name: &str) -> String {
    format!("dotlnx-{}", sanitize_profile_segment(app_name))
}

/// Safe profile name for user tier (use when name may not have been validated).
pub fn profile_name_safe(username: &str, app_name: &str) -> String {
    profile_name_user(username, app_name)
}

/// Safe profile name for system tier.
pub fn profile_name_safe_system(app_name: &str) -> String {
    profile_name_system(app_name)
}

/// Generate AppArmor profile text from config (bundle path + security section).
/// `profile_name` is either dotlnx-<username>-<name> (user) or dotlnx-<name> (system).
/// Only used when [security] confine = true; when false, no profile is loaded.
pub fn generate_profile(bundle_root: &Path, config: &Config, profile_name: &str) -> String {
    generate_profile_minimal(bundle_root, config, profile_name)
}

fn generate_profile_minimal(
    bundle_root: &Path,
    config: &Config,
    profile_name: &str,
) -> String {
    let bundle_path = bundle_root.display().to_string();
    let exec_path = bundle_root.join(&config.executable);
    let exec_path_str = exec_path.display().to_string();

    let mut rules = Vec::new();
    rules.push(format!("  {} ix,", quote_path_for_apparmor(&exec_path_str)));
    // rm: read + memory-map executable (needed for loading .so from bundle)
    rules.push(format!(
        "  {} rm,",
        quote_path_for_apparmor(&format!("{}/**", bundle_path))
    ));

    if let Some(ref sec) = config.security {
        for p in &sec.read_paths {
            let safe = sanitize_apparmor_path(p);
            if !safe.is_empty() {
                rules.push(format!("  {} r,", quote_path_for_apparmor(&safe)));
            }
        }
        for p in &sec.write_paths {
            let safe = sanitize_apparmor_path(p);
            if !safe.is_empty() {
                rules.push(format!("  {} rw,", quote_path_for_apparmor(&safe)));
            }
        }
        if sec.network {
            rules.push("  network inet stream,".to_string());
            rules.push("  network inet6 stream,".to_string());
        }
    }

    // Minimal system: libs, proc (read), config/data dirs, tmp, shm
    rules.push("  /usr/lib/** rm,".to_string());
    rules.push("  /lib/** rm,".to_string());
    rules.push("  /proc/sys/** r,".to_string());
    rules.push("  /proc/** r,".to_string());
    rules.push("  owner @{HOME}/.config/** rw,".to_string());
    rules.push("  owner @{HOME}/.local/share/** rw,".to_string());
    rules.push("  /tmp/** rw,".to_string());
    rules.push("  /dev/shm/** rw,".to_string());

    let rules_text = rules.join("\n");
    format!(
        "# dotlnx generated profile for {}\n\
         #include <tunables/global>\n\
         profile {} {{\n\
         #include <abstractions/base>\n\
         {}\n\
         }}\n",
        config.name, profile_name, rules_text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Security};

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
            terminal: false,
        }
    }

    #[test]
    fn profile_name_user_format() {
        assert_eq!(
            profile_name_user("alice", "myapp"),
            "dotlnx-alice-myapp"
        );
    }

    #[test]
    fn profile_name_user_sanitizes() {
        assert_eq!(
            profile_name_user("user@host", "app.name"),
            "dotlnx-user_host-app_name"
        );
    }

    #[test]
    fn profile_name_user_spaces_become_underscores() {
        // Names with spaces (e.g. "hello-world 2") must produce a single profile name used by both sync and run.
        assert_eq!(
            profile_name_user("kevin", "hello-world 2"),
            "dotlnx-kevin-hello-world_2"
        );
    }

    #[test]
    fn profile_name_system_format() {
        assert_eq!(profile_name_system("myapp"), "dotlnx-myapp");
    }

    #[test]
    fn generate_profile_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path();
        let cfg = minimal_config();
        let out = generate_profile(bundle, &cfg, "dotlnx-myapp");
        assert!(out.contains("profile dotlnx-myapp {"));
        assert!(out.contains("# dotlnx generated profile for myapp"));
        assert!(out.contains("ix,"));
        assert!(out.contains("** rm,"));
        assert!(out.contains("/usr/lib/** rm,"));
    }

    #[test]
    fn generate_profile_with_security() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = minimal_config();
        cfg.security = Some(Security {
            confine: true,
            read_paths: vec!["/tmp/read".into()],
            write_paths: vec!["/tmp/write".into()],
            network: true,
            capabilities: vec![],
        });
        let out = generate_profile(dir.path(), &cfg, "dotlnx-myapp");
        assert!(out.contains("/tmp/read r,"));
        assert!(out.contains("/tmp/write rw,"));
        assert!(out.contains("network inet stream"));
    }

    #[test]
    fn generate_profile_skips_empty_sanitized_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = minimal_config();
        cfg.security = Some(Security {
            confine: true,
            read_paths: vec!["###".into(), "/valid".into()],
            write_paths: vec![],
            network: false,
            capabilities: vec![],
        });
        let out = generate_profile(dir.path(), &cfg, "dotlnx-myapp");
        assert!(out.contains("/valid r,"));
        assert!(!out.contains("r,\n  r,"));
    }

    #[test]
    fn generate_profile_quotes_paths_with_spaces() {
        // Bundle path with space (e.g. "hello-world 2.lnx") must be quoted for AppArmor lexer.
        let dir = tempfile::tempdir().unwrap();
        let bundle_with_space = dir.path().join("hello world");
        std::fs::create_dir_all(bundle_with_space.join("bin")).unwrap();
        std::fs::write(bundle_with_space.join("bin/myapp"), b"").unwrap();
        let cfg = minimal_config();
        let out = generate_profile(&bundle_with_space, &cfg, "dotlnx-myapp");
        assert!(
            out.contains("\"/") && out.contains("hello world") && out.contains("\" ix,"),
            "exec path with space should be quoted: {}",
            out.lines().take(10).collect::<Vec<_>>().join("\n")
        );
        assert!(
            out.contains("\"/") && out.contains("hello world") && out.contains("/**\" rm,"),
            "bundle path with space should be quoted"
        );
    }
}

/// Directory under which dotlnx stores generated profiles. Requires root to write.
pub const DOTLNX_APPARMOR_DIR: &str = "/etc/apparmor.d/dotlnx.d";

/// Load a profile (write to DOTLNX_APPARMOR_DIR, then apparmor_parser -r). Requires root when AppArmor is present.
pub fn load_profile(profile_name: &str, profile_content: &str) -> Result<()> {
    let parser = find_apparmor_parser().with_context(|| {
        "apparmor_parser not found (checked /usr/sbin, /sbin, and PATH)"
    })?;
    let path = std::path::Path::new(DOTLNX_APPARMOR_DIR).join(profile_name);
    if path.exists() {
        std::fs::write(&path, profile_content)?;
        let out = std::process::Command::new(&parser)
            .args(["-r", path.to_str().unwrap_or_default()])
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "apparmor_parser -r failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, profile_content)?;
    let out = std::process::Command::new(&parser)
        .args(["-r", path.to_str().unwrap_or_default()])
        .output()?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&path);
        anyhow::bail!(
            "apparmor_parser -r failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Unload/remove a profile (apparmor_parser -R, then remove file). May require root.
pub fn unload_profile(profile_name: &str) -> Result<()> {
    let parser = find_apparmor_parser().with_context(|| {
        "apparmor_parser not found (checked /usr/sbin, /sbin, and PATH)"
    })?;
    let path = std::path::Path::new(DOTLNX_APPARMOR_DIR).join(profile_name);
    if !path.exists() {
        return Ok(());
    }
    let path_str = path.to_str().unwrap_or_default();
    let out = std::process::Command::new(&parser)
        .args(["-R", path_str])
        .output()?;
    if !out.status.success() {
        // Profile may already be unloaded; try removing file anyway
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::remove_file(&path)?;
    Ok(())
}
