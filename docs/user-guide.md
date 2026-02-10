# User guide

This guide is for people who **use** dotlnx as end users: you add or remove application bundles, and the system takes care of menu entries and (when available) AppArmor confinement.

## You don’t run dotlnx

End users don’t run the `dotlnx` command. You only:

1. **Add** a `.lnx` bundle into your Applications directory (or an admin puts it in `/Applications`).
2. **Launch** the app from your application menu like any other app.
3. **Remove** the bundle when you no longer want the app (or an admin removes it from `/Applications`).

A background service (or an admin running `dotlnx sync`) keeps the menu and security profiles in sync with what’s in those directories.

## Where to put apps

### Just for you (user tier)

Put `.lnx` bundles in:

- **`~/Applications`** (default), or  
- The directory set by **`DOTLNX_APPLICATIONS`** if your system is configured to use it.

Only you will see these apps in your menu. The generated `.desktop` files go into `~/.local/share/applications`.

### For everyone (system tier)

An administrator puts `.lnx` bundles in:

- **`/Applications`** (default), or  
- The directory set by **`DOTLNX_SYSTEM_APPLICATIONS`** if configured.

These apps appear in the application menu for all users. The generated `.desktop` files go into `/usr/share/applications`. This tier requires root; normal users cannot add system-tier apps.

## Adding an app

1. Get a `.lnx` bundle (e.g. `MyApp.lnx` from the developer or your distro).
2. Copy the **entire bundle** into `~/Applications` (or into `/Applications` if you have admin rights and want it system-wide).
   ```bash
   cp -r MyApp.lnx ~/Applications/
   ```
3. Wait a few seconds for the watcher to run a sync, or ask an admin to run `dotlnx sync`.
4. Open your application menu; the app should appear with its name and icon (if the bundle provides one). Launch it like any other app.

## Removing an app

1. Remove the `.lnx` **bundle** from `~/Applications` or `/Applications`.
   ```bash
   rm -r ~/Applications/MyApp.lnx
   ```
2. After the next sync, the menu entry (and AppArmor profile, if any) is removed. The app will no longer appear in the menu.

You do **not** need to run `dotlnx uninstall` yourself; the watcher (or an admin running `dotlnx sync`) handles that when the bundle is gone.

## What’s in a .lnx bundle?

You don’t need to edit anything inside. A typical application bundle contains:

- **config.toml** — Tells dotlnx the app name, which executable to run, and optional security/desktop settings.
- **bin/** (or similar) — The actual program or script.
- **assets/** — Optional icons and other files.

Developers and packagers create these; as a user you just drop the bundle in place.

## Troubleshooting

- **App doesn’t appear in the menu**  
  - Check that the bundle name ends with `.lnx` and that it’s directly under `~/Applications` or `/Applications` (not in a subdirectory).  
  - Ensure the watcher is running: `systemctl status dotlnx.service` (if using the systemd service).  
  - An admin can run `dotlnx sync --dry-run` to see what would be synced, or `dotlnx validate ~/Applications/YourApp.lnx` to check the bundle.

- **App launches but then fails or is restricted**  
  - Some apps (e.g. certain Electron/Chromium apps) don’t work well under AppArmor. The bundle author can set `confine = false` in `config.toml`; if you’re not the author, ask them or your distro to provide an updated bundle.

- **I want to change the app’s name or icon**  
  - That’s controlled by the bundle’s `config.toml`. If you’re not the bundle author, you’d need to edit that file (or get an updated bundle). See [Config reference](config-reference.md) for the options.

For admins and developers, the [Bundle author guide](bundle-author-guide.md), [Config reference](config-reference.md), and [Security (AppArmor)](security.md) docs have more detail.
