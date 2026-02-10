# Config reference

Every `.lnx` bundle must have a **config.toml** at its root. It is parsed as TOML and drives how the app is run, how the generated `.desktop` entry looks, and (optionally) how the AppArmor profile is generated.

## Required fields

| Key | Description |
|-----|-------------|
| **name** | App name used in the menu and for the AppArmor profile. Must not contain path separators, `..`, `;`, or control characters. |
| **executable** | Path to the executable **relative to the bundle root** (e.g. `bin/myapp`). Must exist inside the bundle. No leading slash. |

## Run section

All run-related keys are at the top level (no `[run]` section).

| Key | Required | Default | Description |
|-----|----------|---------|-------------|
| **name** | Yes | — | App name (menu and profile). |
| **executable** | Yes | — | Path to executable relative to bundle root. |
| **args** | No | `[]` | List of arguments passed to the executable. |
| **env** | No | `[]` | List of `key=value` environment variables for the process. |
| **working_dir** | No | (bundle root) | Working directory when launching, relative to bundle root. |

### Example (run)

```toml
name = "myapp"
executable = "bin/myapp"
args = ["--verbose", "--config", "data/config.json"]
env = ["APP_DEBUG=1", "HOME=/custom/home"]
working_dir = "data"
```

## Desktop section

These keys control the generated `.desktop` file (menu entry). All are optional and live at the top level.

| Key | Required | Default | Description |
|-----|----------|---------|-------------|
| **icon** | No | — | Icon name (theme) or path for the menu entry. |
| **comment** | No | — | Short description (tooltip / comment in .desktop). |
| **categories** | No | — | List of desktop categories (e.g. `["Utility", "Development"]`). |
| **terminal** | No | `false` | If `true`, add `Terminal=true` so the app runs in a terminal (for CLI apps). |

### Example (desktop)

```toml
name = "myapp"
executable = "bin/myapp"
icon = "myapp"
comment = "A short description of the app"
categories = ["Utility", "Development"]
terminal = false
```

## Security section

Optional **`[security]`** block used to generate the AppArmor profile. If absent, a minimal default profile is still used when `confine` is true (see [Security (AppArmor)](security.md)).

| Key | Required | Default | Description |
|-----|----------|---------|-------------|
| **confine** | No | `true` | If `false`, run **without** AppArmor (no confinement). Use for Electron/Chromium apps that conflict with the sandbox. |
| **read_paths** | No | `[]` | List of absolute paths the app may read. No `#`, `..`, or newlines. |
| **write_paths** | No | `[]` | List of absolute paths the app may read and write. Same rules as read_paths. |
| **network** | No | `false` | If `true`, allow network (inet + inet6 stream). |
| **capabilities** | No | `[]` | Reserved for future capability rules. |

### Example (security)

```toml
name = "myapp"
executable = "bin/myapp"

[security]
confine = true
read_paths = ["/usr/share/myapp/data", "/opt/legacy/config"]
write_paths = ["/var/lib/myapp", "/tmp/myapp"]
network = true
```

### Disabling confinement

For apps that fail under AppArmor (e.g. many Electron/Chromium apps):

```toml
[security]
confine = false
```

No profile is loaded; the app runs like a normal executable (similar to double-clicking without dotlnx).

## Minimal config

The smallest valid `config.toml`:

```toml
name = "myapp"
executable = "bin/myapp"
```

## Validation rules

- **name:** No path separators, `..`, `;`, or control characters.
- **executable:** Must exist as a file under the bundle root; no leading slash.
- **Paths in read_paths / write_paths:** Absolute paths only; must not contain `#`, `..`, or newlines.

Use `dotlnx validate <path>` to check a bundle before distributing. See [Bundle author guide](bundle-author-guide.md).

## Full sample

See [config.toml.sample](config.toml.sample) for a commented sample with every option.
