# dotlnx

Drop `.lnx` bundles in `~/Applications` (or `/Applications`) to install; the watcher syncs them to the menu and AppArmor. **End users never run dotlnx.** They just add or remove folders; the service does the rest. Developers can use `dotlnx validate` to check bundles before distributing.

Source and releases: [github.com/nivekxyz/dotlnx](https://github.com/nivekxyz/dotlnx).

**License:** [GPL-3.0](LICENSE). Copyright (C) 2026 Kevin Cordia Jr.

## Build

```bash
cargo build --release
```

Binary: `target/release/dotlnx`. For system-wide install (e.g. package): place in `/usr/bin/dotlnx`.

**Tests:** Run `cargo test`. All tests use temp dirs and cross-platform logic only (no Linux-specific AppArmor load, `aa-exec`, or root), so they pass on macOS and other non-Linux hosts.

### Debian package (.deb)

On a Debian/Ubuntu system (or Linux with `dpkg`), you can build a `.deb` that installs the binary and the systemd service (enable + start on install):

```bash
cargo install cargo-deb
cargo deb
```

The package is written to `target/debian/dotlnx_<version>_<arch>.deb`. Install it with:

```bash
sudo dpkg -i target/debian/dotlnx_*.deb
```

The package post-install enables and starts `dotlnx.service`; on purge it stops and disables the service. Build on Linux (or use `cargo deb --target=...` for cross-compilation; see [cargo-deb](https://github.com/kornelski/cargo-deb)).

### RPM package (.rpm)

On Fedora/RHEL (or any Linux with `rpm`), you can build an `.rpm` that installs the binary and the systemd service (enable + start on install):

```bash
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm
```

The package is written to `target/generate-rpm/dotlnx-<version>.rpm`. Install it with:

```bash
sudo rpm -Uvh target/generate-rpm/dotlnx-*.rpm
```

Or use `dnf install ./target/generate-rpm/dotlnx-*.rpm`. The package post-install enables and starts `dotlnx.service`; on remove it stops and disables the service. No `rpmbuild` is required (see [cargo-generate-rpm](https://github.com/cat-in-136/cargo-generate-rpm)).

## Usage (for admins / developers)

| Command | Description |
|---------|-------------|
| `dotlnx sync [--dry-run]` | One-shot sync (used by watch; scripts/CI). As root: all users + system. With `sudo`: invoking user + system. |
| `dotlnx watch [--once]` | Watch app folders and auto-sync. `--once`: run one sync then exit (e.g. service startup). |
| `dotlnx run <name>` | Launch app by name (diagnostics/scripting). Menu launchers use the direct executable path, not this. |
| `dotlnx validate <path>` | Validate a .lnx bundle (path = .lnx dir or dir containing .lnx dirs). Exit 0 if valid. |
| `dotlnx uninstall <name>` | Remove desktop entry and AppArmor profile for `<name>` (does not delete the .lnx folder). |

**Exit codes:** 0 = success, 1 = error (invalid args, app not found, sync/validate failure). Errors are printed to stderr.

**Logging:** dotlnx uses [tracing](https://docs.rs/tracing); output goes to stderr. Set `RUST_LOG` to control verbosity (e.g. `RUST_LOG=info` or `RUST_LOG=debug`). Default is `info`. For the systemd service, use `Environment=RUST_LOG=info` in the unit or a drop-in.

## Service (systemd)

When installed as root (e.g. by a package), enable and start the watcher so it runs by default:

```bash
# Copy unit (adjust path to binary if needed)
sudo cp contrib/dotlnx.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now dotlnx.service
```

The service runs as root, watches `/Applications` and all users’ `~/Applications` (e.g. `/home/*/Applications`, `/root/Applications`), and runs a full sync on any change. End users only add/remove `.lnx` folders and launch apps from the menu.

## Bundle format (.lnx)

A valid `.lnx` bundle is a **directory** named `*.lnx` (e.g. `myapp.lnx`). Authors do **not** ship `.desktop` files; dotlnx generates the installed `.desktop` from `config.toml` only.

### Layout

```
myapp.lnx/
├── config.toml          # Required: run config + optional security & desktop metadata
├── bin/                 # Optional: app binaries / scripts
├── lib/                 # Optional: dependencies / libraries
└── ...                  # Any other app files
```

### config.toml

Single config file at the bundle root. Parsed as TOML.

| Section / key | Required | Description |
|---------------|----------|-------------|
| **Run** | | |
| `name` | Yes | App name (menu, profile name). No path separators, `..`, `;`, or control chars. |
| `executable` | Yes | Path to executable **relative to bundle root** (e.g. `bin/myapp`). Must exist. |
| `args` | No | List of arguments to pass to the executable. |
| `env` | No | List of `key=value` env vars for the process. |
| `working_dir` | No | Working directory relative to bundle root. |
| **Desktop** (for generated .desktop) | | |
| `icon` | No | Icon name or path for the menu entry. |
| `comment` | No | Short description. |
| `categories` | No | List of desktop categories (e.g. `["Utility"]`). |
| **Security** (for AppArmor profile generation) | | |
| `[security]` | No | Optional. When confine = true (default), dotlnx generates an AppArmor profile from paths/network. |
| `confine` | No | If false, run **without** AppArmor (no confinement). Default true. Use for Electron/Chromium apps that fail under confinement. |
| `read_paths` | No | List of paths the app may read. |
| `write_paths` | No | List of paths the app may read/write. |
| `network` | No | If true, allow network (inet/inet6 stream). |
| `capabilities` | No | Reserved for future capability rules. |

If `[security]` is absent, a minimal default profile is used. Paths in `read_paths`/`write_paths` must not contain `#`, `..`, or newlines.

**Electron/Chromium apps:** Chromium’s sandbox conflicts with AppArmor. Set `confine = false` to run without AppArmor (like double-clicking).

### Validation

`dotlnx validate <path>` checks: path is a directory named `*.lnx`, `config.toml` exists and parses, `name` and `executable` are present, `executable` exists under the bundle root, and optional security/desktop fields are valid.

### Minimal config.toml

```toml
name = "myapp"
executable = "bin/myapp"
```

### Building a .lnx bundle

1. Create a directory `myapp.lnx/`.
2. Add `config.toml` with at least `name` and `executable` (path relative to bundle root).
3. Put your binary (e.g. under `bin/myapp`) and any assets.
4. Run `dotlnx validate ./myapp.lnx` to check.
5. Copy `myapp.lnx` to `~/Applications` or `/Applications`; the watcher (or `dotlnx sync`) will install it.

## Application tiers

- **User tier:** `~/Applications` (or `$DOTLNX_APPLICATIONS`) → `.desktop` in `~/.local/share/applications`. Visible only to that user.
- **System tier:** `/Applications` (or `$DOTLNX_SYSTEM_APPLICATIONS`) → `.desktop` in `/usr/share/applications`. Requires root; visible to all users.

When run as root without `SUDO_USER` (e.g. the daemon), sync and watch cover all users’ `~/Applications` and `/Applications`.

## AppArmor

If AppArmor is installed and dotlnx runs as root, sync generates and loads a profile per app (user: `dotlnx-<username>-<name>`, system: `dotlnx-<name>`). Profiles are stored under `/etc/apparmor.d/dotlnx.d/`. The generated .desktop file uses the **absolute path to the bundle executable** (or `aa-exec -p PROFILE -- /path` when confined), so the launcher’s process is the app. If AppArmor is not available, dotlnx does desktop integration only and skips profile loading.
