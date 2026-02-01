//! Copyright (C) 2026 Kevin Cordia Jr.
//!
//! This program is free software: you can redistribute it and/or modify
//! it under the terms of the GNU General Public License as published by
//! the Free Software Foundation, either version 3 of the License, or
//! (at your option) any later version.
//!
//! This program is distributed in the hope that it will be useful,
//! but WITHOUT ANY WARRANTY; without even the implied warranty of
//! MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//! GNU General Public License for more details.
//!
//! You should have received a copy of the GNU General Public License
//! along with this program.  If not, see <https://www.gnu.org/licenses/>.

mod apparmor;
mod bundle;
mod config;
mod desktop;
mod sync;
mod uninstall;
mod validate;
mod watch;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "dotlnx")]
#[command(about = "Drop .lnx folders in the app folder to install; watcher syncs to menu + AppArmor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// One-shot sync (used by watch service; also for scripts/CI). Not for end users.
    Sync {
        /// Only print what would be done
        #[arg(long)]
        dry_run: bool,
    },
    /// Watch app folders and auto-sync on change. Default behavior; package starts this.
    Watch {
        /// Run one full sync then exit (useful for service startup)
        #[arg(long)]
        once: bool,
    },
    /// Launch an app by name (invoked by .desktop; end users don't run this manually).
    Run {
        /// App name (from config.toml)
        name: String,
    },
    /// Validate a .lnx bundle. For developers: ensure bundle works before distributing.
    Validate {
        /// Path to .lnx directory or directory containing .lnx dirs
        path: std::path::PathBuf,
    },
    /// Remove app from dotlnx (used by watch when folder removed; or admins). End users just remove the folder.
    Uninstall {
        /// App name (from config.toml)
        name: String,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    if let Err(e) = run() {
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Sync { dry_run } => crate::sync::run(dry_run),
        Commands::Watch { once } => crate::watch::run(once),
        Commands::Run { name } => run_app(&name),
        Commands::Validate { path } => crate::validate::run(&path),
        Commands::Uninstall { name } => uninstall::run(&name),
    }
}

fn run_app(name: &str) -> Result<()> {
    let (bundle_path, config, is_user_tier) = match crate::bundle::resolve_bundle_by_name(name)? {
        Some(t) => t,
        None => anyhow::bail!("app not found: {}", name),
    };
    let profile = if is_user_tier {
        let username = crate::bundle::username_from_bundle_path(&bundle_path)
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "unknown".into()));
        crate::apparmor::profile_name_safe(&username, &config.name)
    } else {
        crate::apparmor::profile_name_safe_system(&config.name)
    };
    let exec_path = bundle_path.join(&config.executable);
    if !exec_path.exists() {
        anyhow::bail!("executable not found: {}", exec_path.display());
    }
    crate::validate::path_under_bundle(&exec_path, &bundle_path)?;
    let cwd = config
        .working_dir
        .as_ref()
        .map(|d| bundle_path.join(d))
        .unwrap_or_else(|| bundle_path.clone());
    if let Some(ref d) = config.working_dir {
        let cwd_resolved = bundle_path.join(d);
        if cwd_resolved.exists() {
            crate::validate::path_under_bundle(&cwd_resolved, &bundle_path)?;
        }
    }
    let mut env: Vec<(String, String)> = config
        .env
        .iter()
        .filter_map(|s| {
            let (k, v) = s.split_once('=')?;
            Some((k.trim().into(), v.trim().into()))
        })
        .collect();
    // Ensure PATH includes bundle bin if present
    let bin_dir = bundle_path.join("bin");
    if bin_dir.exists() {
        let path = std::env::var_os("PATH")
            .and_then(|p| p.into_string().ok())
            .unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), path);
        env.push(("PATH".into(), new_path));
    }
    let confine = config.security.as_ref().map(|s| s.confine).unwrap_or(true);
    let status = if confine {
        run_with_profile(&profile, &exec_path, &config.args, &cwd, &env)?
    } else {
        run_unconfined(&exec_path, &config.args, &cwd, &env)?
    };
    std::process::exit(status.code().unwrap_or(1));
}

/// Run executable without AppArmor (used when [security] confine = false, e.g. Electron apps).
fn run_unconfined(
    exec_path: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &[(String, String)],
) -> Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new(exec_path);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    Ok(cmd.status()?)
}

/// Run executable under AppArmor profile via aa-exec; if aa-exec is unavailable, run without confinement.
fn run_with_profile(
    profile: &str,
    exec_path: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &[(String, String)],
) -> Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new("aa-exec");
    cmd.args(["-p", profile, "--"]);
    cmd.arg(exec_path).args(args);
    cmd.current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    match cmd.status() {
        Ok(s) => return Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    // aa-exec not found (e.g. non-Linux or AppArmor not installed); run without confinement
    let mut fallback = std::process::Command::new(exec_path);
    fallback.args(args).current_dir(cwd);
    for (k, v) in env {
        fallback.env(k, v);
    }
    Ok(fallback.status()?)
}

