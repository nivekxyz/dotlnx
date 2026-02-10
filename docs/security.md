# Security (AppArmor)

dotlnx can run each `.lnx` app under an **AppArmor** profile generated from the bundle’s `config.toml`. This gives declarative, per-app confinement without hand-writing AppArmor files.

## When AppArmor is used

- **AppArmor installed** and **dotlnx runs as root** (e.g. the systemd service): sync generates and loads a profile per app. Profiles are stored under `/etc/apparmor.d/dotlnx.d/`.
- **No AppArmor** or **dotlnx not root**: dotlnx still generates `.desktop` entries but skips profile loading. Apps run without dotlnx-managed confinement.

End users don’t need to do anything; the watcher (or `dotlnx sync`) handles profile generation and loading when bundles are added or updated.

## Profile names

- **User tier** (apps in `~/Applications`): `dotlnx-<username>-<name>` (e.g. `dotlnx-jane-MyApp`) so names don’t collide across users.
- **System tier** (apps in `/Applications`): `dotlnx-<name>` (e.g. `dotlnx-MyApp`).

The generated `.desktop` file uses the **absolute path to the bundle executable**. When confinement is enabled, the launcher runs the app under the corresponding profile (via the profile attached to that path or the process). When `confine = false`, no profile is applied.

## How the profile is generated

When `[security]` is present and `confine = true` (the default), dotlnx generates a profile that:

- Allows the bundle directory (read + execute for traversal, read for files, execute for the main executable).
- Adds **read_paths** as read-only.
- Adds **write_paths** as read/write.
- If **network** is true, allows inet and inet6 stream.
- **capabilities** is reserved for future use.

If `[security]` is omitted, a **minimal default** profile is still used when confine is true (bundle access only, no extra paths, no network). So every confined app gets at least that baseline.

## Config options (recap)

| Option | Effect |
|--------|--------|
| **confine = true** (default) | Generate and load an AppArmor profile for this app. |
| **confine = false** | Do not use AppArmor for this app. Use for apps that break under confinement (e.g. Electron/Chromium). |
| **read_paths** | Absolute paths the app may read. |
| **write_paths** | Absolute paths the app may read and write. |
| **network = true** | Allow network (inet + inet6 stream). |

Path rules must not contain `#`, `..`, or newlines. See [Config reference](config-reference.md).

## Electron / Chromium apps

Chromium’s sandbox often conflicts with AppArmor. If your app is Electron- or Chromium-based and fails to start or run correctly under dotlnx, set in `config.toml`:

```toml
[security]
confine = false
```

The app will run without AppArmor (like running the binary directly). Prefer confining when possible; disable only when necessary.

## Uninstall and profile removal

When a `.lnx` bundle is removed from the Applications directory, the next sync **uninstalls** the app: the `.desktop` file is removed and the AppArmor profile is unloaded (and the file under `/etc/apparmor.d/dotlnx.d/` can be removed by the uninstall logic). So removing the bundle cleans up both menu and security state.

## Inspecting profiles

- Profiles on disk: `/etc/apparmor.d/dotlnx.d/` (when dotlnx has written them).
- List loaded profiles: `aa-status` (when AppArmor is available).
- To debug, run with `RUST_LOG=debug` and watch for profile generation/load messages.

## Summary

| Goal | Action |
|------|--------|
| Default behavior | Omit `[security]` or set `confine = true`; minimal or custom profile is used. |
| Allow extra paths | Set `read_paths` and/or `write_paths` in `[security]`. |
| Allow network | Set `network = true` in `[security]`. |
| Disable confinement | Set `confine = false` in `[security]` (e.g. for Electron/Chromium). |

For full config syntax, see [Config reference](config-reference.md).
