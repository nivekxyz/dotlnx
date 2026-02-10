# Getting started

This guide covers installing dotlnx and enabling the watcher service so that `.lnx` bundles in your Applications directories are synced to the app menu and AppArmor.

## Prerequisites

- **Linux** with systemd (dotlnx is designed for Linux; the service and AppArmor integration are Linux-specific).
- **AppArmor** (optional but recommended): if present and dotlnx runs as root, sync will generate and load AppArmor profiles per app. Without AppArmor, dotlnx still performs desktop integration only.
- For **system tier** (apps in `/Applications`): root/sudo to install and run the service.

## Install

Choose one of the following.

### From source

```bash
git clone https://github.com/nivekxyz/dotlnx.git
cd dotlnx
cargo build --release
```

Binary: `target/release/dotlnx`. For system-wide use, copy it to e.g. `/usr/bin/dotlnx`.

### Debian / Ubuntu (.deb)

```bash
cargo install cargo-deb
cargo deb
sudo dpkg -i target/debian/dotlnx_*.deb
```

The package installs the binary and the systemd unit; post-install enables and starts `dotlnx.service`.

### Fedora / RHEL (.rpm)

```bash
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm
sudo rpm -Uvh target/generate-rpm/dotlnx-*.rpm
# or: dnf install ./target/generate-rpm/dotlnx-*.rpm
```

Post-install enables and starts `dotlnx.service`.

### Arch Linux (pacman)

```bash
cd arch
makepkg -si
```

Builds from the GitHub release tarball; ensure a tag `v<pkgver>` exists. The install script enables and starts `dotlnx.service`.

## Enable the service

When installed as root (e.g. via a package), the service is usually enabled and started automatically. Otherwise:

```bash
sudo cp contrib/dotlnx.service /etc/systemd/system/
# If dotlnx is not in /usr/bin, edit ExecStart/ExecStartPre paths
sudo systemctl daemon-reload
sudo systemctl enable --now dotlnx.service
```

The service runs as root, watches `/Applications` and all users’ `~/Applications`, and runs a full sync on any change.

## Verify

1. **Service is running**
   ```bash
   systemctl status dotlnx.service
   ```

2. **One-shot sync (dry-run)**  
   As root (or with sudo if you only want to sync the invoking user):
   ```bash
   sudo dotlnx sync --dry-run
   ```
   You should see no errors; it may report no bundles if `~/Applications` and `/Applications` are empty.

3. **Add a test bundle**  
   Create a minimal `.lnx` bundle in your user Applications directory:
   ```bash
   mkdir -p ~/Applications
   mkdir -p ~/Applications/Test.lnx/bin
   echo '#!/bin/sh' > ~/Applications/Test.lnx/bin/test
   echo 'echo Hello from dotlnx' >> ~/Applications/Test.lnx/bin/test
   chmod +x ~/Applications/Test.lnx/bin/test
   printf 'name = "Test"\nexecutable = "bin/test"\n' > ~/Applications/Test.lnx/config.toml
   dotlnx validate ~/Applications/Test.lnx
   ```
   After a moment (or after running `dotlnx sync`), the app “Test” should appear in your application menu. Launching it should run the script.

## Logging

dotlnx uses [tracing](https://docs.rs/tracing); output goes to stderr. Set `RUST_LOG` to control verbosity:

- `RUST_LOG=info` (default)
- `RUST_LOG=debug` for more detail

For the systemd service, add to the unit or a drop-in:

```ini
Environment=RUST_LOG=info
```

## Next steps

- **End users:** See [User guide](user-guide.md) for where to put apps and how tiers work.
- **Bundle authors:** See [Bundle author guide](bundle-author-guide.md) and [Config reference](config-reference.md).
- **Security:** See [Security (AppArmor)](security.md) for how confinement works and when to disable it.
