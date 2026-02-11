# Bundle author guide

This guide explains how to create and distribute **.lnx bundles** so they work with dotlnx: portable, menu-integrated, and optionally confined with AppArmor.

## What is a .lnx bundle?

A **.lnx bundle** is a **directory** whose name ends with `.lnx` (e.g. `MyApp.lnx`). It contains:

- **config.toml** (required) — App name, path to the executable, and optional desktop/security settings.
- **bin/** (optional) — Your executable(s) or scripts.
- **lib/**, **assets/**, etc. (optional) — Any other files your app needs.

dotlnx does **not** use hand-written `.desktop` or AppArmor files. It generates them from `config.toml` when syncing. You only maintain one config file.

## Bundle layout

```
MyApp.lnx/
├── config.toml          # Required: name, executable, optional desktop & security
├── bin/                 # Optional: app binary or script
│   └── myapp            # or run.sh for AppImage bundles
├── lib/                 # Optional: libraries
└── assets/              # Optional: icons, etc.
    └── icon.png
```

The **executable** path in `config.toml` is relative to the bundle root (e.g. `bin/myapp` or `bin/run.sh`).

## Quick scaffold: `dotlnx bundle`

The fastest way to create a new bundle is the `dotlnx bundle` command.

### From an AppImage

Creates a bundle with `bin/` (AppImage copied in), `config.toml`, `run.sh`, and `assets/`. The generated `run.sh` launches the newest AppImage in `bin/`, so the bundle always runs the latest version you put there.

**Why this helps:** Along with installing menu shortcuts and icons, updating is as simple as dropping a new AppImage into the bundle’s `bin/` directory. Your app can do that itself (e.g. an in-app updater that downloads the new AppImage and replaces or adds it under `MyApp.lnx/bin/`). Users get a seamless update with no reinstall: the next launch automatically uses the new version because the bundle always picks the latest file in `bin/`.

```bash
dotlnx bundle --appname "My App" --appimage /path/to/MyApp-1.0.0-x86_64.AppImage
```

This creates **My App.lnx/** in the current directory. You can specify an output directory:

```bash
dotlnx bundle --appname "My App" --appimage /path/to/MyApp.AppImage --output-dir /path/to/output
```

Add an icon in `assets/icon.png` if desired, then validate and distribute.

### From a binary or script

Creates a bundle with `bin/` (your file copied in), `config.toml`, and `assets/`. That file is the executable (no `run.sh`).

```bash
dotlnx bundle --appname "My Tool" --bin /path/to/mytool.sh
# or
dotlnx bundle --appname "My App" --bin /path/to/myapp
```

Creates **My Tool.lnx/** or **My App.lnx/**. Add `assets/icon.png` and any extra paths or security options in `config.toml` as needed.

## Manual bundle creation

1. **Create the directory**
   ```bash
   mkdir -p MyApp.lnx/bin
   ```

2. **Add config.toml** at the bundle root with at least `name` and `executable`:
   ```toml
   name = "MyApp"
   executable = "bin/myapp"
   ```
   See [Config reference](config-reference.md) for all options (args, env, icon, categories, security).

3. **Put your executable** at the path specified by `executable` (e.g. `MyApp.lnx/bin/myapp`). Ensure it’s executable (`chmod +x`).

4. **Validate** before distributing:
   ```bash
   dotlnx validate ./MyApp.lnx
   ```
   Exit code 0 means the bundle is valid.

5. **Distribute** the whole bundle. Users (or admins) place it in `~/Applications` or `/Applications`.

## Validation

`dotlnx validate <path>` checks:

- Path is a directory whose name ends with `.lnx`
- `config.toml` exists and parses
- `name` and `executable` are set and valid (no path separators, `..`, `;`, or control chars in `name`)
- The `executable` file exists under the bundle root
- Optional security and desktop fields are valid

Always run `dotlnx validate ./YourApp.lnx` before shipping or uploading. Use the same path your users will have (e.g. the parent directory containing the bundle, or the bundle directory itself).

## Desktop metadata (optional)

In `config.toml` you can set:

- **icon** — Theme name or path (e.g. `myapp` or path to icon in the bundle).
- **comment** — Short description (tooltip in the menu).
- **categories** — List of desktop categories, e.g. `["Utility", "Development"]`.
- **terminal** — Set to `true` for CLI apps that should run in a terminal (`Terminal=true` in the generated .desktop).

See [Config reference](config-reference.md) for details.

## Security (AppArmor)

By default, dotlnx runs your app under an AppArmor profile generated from `config.toml`. You can:

- **Add paths** — `read_paths` and `write_paths` (absolute paths the app may read or read/write).
- **Allow network** — `network = true` if the app needs the internet.
- **Disable confinement** — `confine = false` for apps that don’t work under AppArmor (e.g. many Electron/Chromium apps).

See [Security (AppArmor)](security.md) for how profiles are generated and when to set `confine = false`.

## Sample config

A full example with every option is in [config.toml.sample](config.toml.sample). Copy it into your bundle as `config.toml` and adjust. Minimal config:

```toml
name = "MyApp"
executable = "bin/myapp"
```

## Distribution

- **Single user:** User copies `MyApp.lnx` into `~/Applications`.
- **All users:** Admin copies `MyApp.lnx` into `/Applications` (root).
- **Packaging:** Distros can ship a `.lnx` bundle in a package that places it in `/Applications` or instructs the user to copy it to `~/Applications`.

Always recommend running `dotlnx validate` in your packaging or release checklist so invalid bundles are caught before release.
