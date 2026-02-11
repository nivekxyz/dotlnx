# dotlnx documentation

**dotlnx** turns self-contained `.lnx` app bundles into integrated, confined applications. Drop a bundle into `~/Applications` (or `/Applications`) and it appears in the app menu with an AppArmor profile applied automatically. End users don’t need to run dotlnx, they only add or remove bundles—but the command is available for sync, validate, and other tasks; the service handles menu entries and security.

## Documentation

| Document | Audience | Description |
|----------|----------|-------------|
| [Getting started](getting-started.md) | Admins, packagers | Install dotlnx (source, .deb, .rpm, Arch), enable the service, verify it works. |
| [User guide](user-guide.md) | End users | Where to put apps, application tiers, adding and removing applications. |
| [Bundle author guide](bundle-author-guide.md) | Developers | Creating .lnx bundles: layout, `dotlnx bundle`, validation, distribution. |
| [Config reference](config-reference.md) | Bundle authors | Full `config.toml` reference: run, desktop, and security options. |
| [Security (AppArmor)](security.md) | Admins, bundle authors | How confinement works, paths, network, and when to disable it (e.g. Electron). |

## Quick links

- **Source & releases:** [github.com/nivekxyz/dotlnx](https://github.com/nivekxyz/dotlnx)
- **Sample config:** [config.toml.sample](config.toml.sample) — copy into your bundle as `config.toml`
- **Project README:** [../README.md](../README.md) — build, usage table, bundle format summary

## Concepts

- **.lnx bundle** — A directory named `*.lnx` (e.g. `myapp.lnx`) containing `config.toml`, optional `bin/`, `lib/`, and other app files. No hand-written `.desktop` or AppArmor files; dotlnx generates them from the config.
- **Application tiers** — **User:** `~/Applications` → `~/.local/share/applications`. **System:** `/Applications` → `/usr/share/applications` (root only, visible to all users).
- **Watcher** — The `dotlnx watch` service (systemd) watches Application directories and runs a sync on any change, so adding or removing a `.lnx` bundle updates the menu and AppArmor automatically.
