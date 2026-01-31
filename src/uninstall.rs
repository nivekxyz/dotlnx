//! Remove app from dotlnx: desktop entries and AppArmor profiles. Does not delete the .lnx folder.

use anyhow::Result;
use std::path::PathBuf;

use crate::apparmor;
use crate::desktop;
use crate::validate;

/// When root + SUDO_USER: use invoking user's desktop dir; when root alone: root's; when non-root: XDG.
fn user_desktop_dir_and_username() -> Result<(PathBuf, String)> {
    if crate::bundle::is_root() {
        let (username, home) = if let Ok(sudo_user) = std::env::var("SUDO_USER") {
            let home = if sudo_user == "root" {
                PathBuf::from("/root")
            } else {
                PathBuf::from("/home").join(&sudo_user)
            };
            (sudo_user, home)
        } else {
            (String::from("root"), PathBuf::from("/root"))
        };
        let desktop_dir = home.join(".local/share/applications");
        Ok((desktop_dir, username))
    } else {
        let desktop_dir = desktop::user_applications_dir()?;
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
        Ok((desktop_dir, username))
    }
}

/// Remove desktop from user dir and (when root) system dir; remove AppArmor profile(s).
/// Does not delete the .lnx bundle folder.
pub fn run(name: &str) -> Result<()> {
    validate::validate_app_name(name)?;
    let is_root = crate::bundle::is_root();
    let (user_desktop, current_user) = user_desktop_dir_and_username()?;

    desktop::uninstall_desktop(&user_desktop, name)?;
    let user_profile = apparmor::profile_name_user(&current_user, name);
    let _ = apparmor::unload_profile(&user_profile);

    if is_root {
        let system_desktop = desktop::system_applications_dir();
        desktop::uninstall_desktop(&system_desktop, name)?;
        let system_profile = apparmor::profile_name_system(name);
        let _ = apparmor::unload_profile(&system_profile);
    }

    Ok(())
}
