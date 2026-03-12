# Depot

A portable, single-binary file sharing server designed for compatibility with both modern and retro systems.

> ⚠️ **SECURITY WARNING**: Depot intentionally implements legacy protocols (SMB1, FTP) that have **NO ENCRYPTION** and **WEAK OR NO AUTHENTICATION**. This is by design, to support vintage systems that cannot use modern secure protocols. **DO NOT** use Depot to share sensitive files. **DO NOT** expose Depot to the public internet. This tool is intended for trusted local networks only (e.g., home labs, retro computing setups, isolated museum exhibits).

## Features

- **FTP Server** - Full FTP support with anonymous access and user authentication
- **HTTP Server** - Simple directory listings with HTML 3.2 compatible output (works with Win3.11 browsers!)
- **SMB1/CIFS Server** - Native Windows file sharing for Win98/XP/2000 *(insecure by design)*
- **Virtual Filesystem** - Merge multiple directories into a unified share structure
- **Web Admin Panel** - Monitor and manage the server through a web interface
- **Cross-Platform** - Runs on Windows, macOS, and Linux
- **Single Binary** - No runtime dependencies, just one file to deploy

## Downloads

Pre-built binaries are available for all major platforms:

| Platform              | Download                                                                                                                |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Linux (x86_64)        | [depot-x86_64-unknown-linux-gnu.tar.gz](../../releases/latest/download/depot-x86_64-unknown-linux-gnu.tar.gz)           |
| Linux (ARM64)         | [depot-aarch64-unknown-linux-gnu.tar.gz](../../releases/latest/download/depot-aarch64-unknown-linux-gnu.tar.gz)         |
| Linux (ARM32)         | [depot-armv7-unknown-linux-gnueabihf.tar.gz](../../releases/latest/download/depot-armv7-unknown-linux-gnueabihf.tar.gz) |
| macOS (Intel)         | [depot-x86_64-apple-darwin.tar.gz](../../releases/latest/download/depot-x86_64-apple-darwin.tar.gz)                     |
| macOS (Apple Silicon) | [depot-aarch64-apple-darwin.tar.gz](../../releases/latest/download/depot-aarch64-apple-darwin.tar.gz)                   |
| Windows (x86_64)      | [depot-x86_64-pc-windows-msvc.zip](../../releases/latest/download/depot-x86_64-pc-windows-msvc.zip)                     |
| Windows (x86, 32-bit) | [depot-i686-pc-windows-msvc.zip](../../releases/latest/download/depot-i686-pc-windows-msvc.zip)                         |

**Raspberry Pi:** Use ARM64 for Pi 3/4/5/Zero 2W with 64-bit OS, or ARM32 for 32-bit Raspberry Pi OS.

**Windows 32-bit:** Supports Windows 7 SP1 and later.

> **Note:** The `latest` release is automatically updated on every push to master. For stable versions, see [tagged releases](../../releases).

## Quick Start

```bash
# Generate example configuration
depot --init

# Edit depot.toml to configure your shares
# Then start the server
depot

# Or use a specific config file
depot -c myconfig.toml
```

## Configuration

Depot uses a TOML configuration file. Run `depot --init` to create an example:

```toml
server_name = "Depot"
log_level = "info"

[shares.Public]
path = "/path/to/public/files"
virtual_path = "/public"
read_only = true
description = "Public files"
enabled = true

[protocols.ftp]
enabled = true
port = 2121
anonymous = true

[protocols.http]
enabled = true
port = 8080
retro_compatible = true

[admin]
enabled = true
port = 8888
username = "admin"
password = "changeme"
```

### Shares

Each share maps a local directory to a virtual path. The share name is the table key (e.g., `[shares.Games]`):

- `path` - Local filesystem path to share
- `virtual_path` - How clients see this share (e.g., `/games` makes files accessible at `/games/filename.zip`)
- `read_only` - Whether write operations are allowed (default: true)
- `description` - Optional description for the share
- `enabled` - Whether this share is active (default: true)

### Protocols

#### FTP
- Default port: 2121 (non-privileged)
- Supports anonymous access
- Supports user/password authentication
- Passive mode with configurable port range

#### HTTP
- Default port: 8080
- HTML 3.2 compatible directory listings
- No JavaScript required
- Works with ancient browsers

#### SMB1/CIFS
- Default port: 4450 (non-privileged) or 445 (standard, requires root)
- Windows native file sharing protocol
- Guest authentication (no passwords required)
- Read-only access for security
- Compatible with Windows 98, ME, 2000, XP

**Note:** SMB is disabled by default. Standard SMB uses port 445, which requires elevated privileges. See [Privileged Ports](docs/PRIVILEGED_PORTS.md).

### SMB Configuration Example

To enable SMB, add this to your `depot.toml`:

```toml
[protocols.smb]
enabled = true
# Port 4450 (default) works without root but requires Windows registry changes
# Port 445 is the standard SMB port but requires root/setcap
port = 4450
netbios_name = "DEPOT"
workgroup = "WORKGROUP"
guest_access = true
```

To connect from Windows to a non-standard port (4450), see [Connecting](docs/CONNECTING.md).

## Building

See [docs/BUILDING.md](docs/BUILDING.md) for build instructions and cross-compilation.

```bash
cargo build --release
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│               Virtual Filesystem                    │
│    (merges multiple directories into one)           │
└─────────────────────────────────────────────────────┘
         ▲           ▲           ▲           ▲
         │           │           │           │
    ┌────┴───┐  ┌────┴───┐  ┌────┴───┐  ┌────┴───┐
    │  FTP   │  │  HTTP  │  │  SMB1  │  │ Future │
    │ :2121  │  │ :8080  │  │ :445   │  │  AFP   │
    └────────┘  └────────┘  └────────┘  └────────┘
```

## Documentation

- [Connecting to Depot](docs/CONNECTING.md) - Client guides for SMB, FTP, HTTP
- [Privileged Ports](docs/PRIVILEGED_PORTS.md) - Binding to ports 445, 21, 80
- [Running as a Service](docs/RUNNING_AS_SERVICE.md) - systemd, launchd, Windows services, Docker
- [Building](docs/BUILDING.md) - Compilation and cross-compilation

## Planned Features

- [x] SMB1 protocol support (for Win95/98/XP shares)
- [ ] AppleTalk/AFP support (for classic Macs)
- [ ] SQLite indexing for large file collections
- [ ] File search functionality
- [ ] Transfer logging/statistics

## Security Considerations

**Depot is intentionally insecure.** This is not a bug—it's the core design goal.

### Why?

Modern security protocols (SMB3, SFTP, HTTPS with TLS 1.3) are incompatible with vintage operating systems. A Windows 98 machine cannot negotiate TLS 1.2. A classic Mac cannot speak SMB3. Depot exists to bridge this gap.

### What's insecure?

| Protocol  | Encryption | Authentication        | Vulnerabilities                                       |
| --------- | ---------- | --------------------- | ----------------------------------------------------- |
| SMB1/CIFS | ❌ None     | ❌ Guest/plaintext     | MITM, credential sniffing, EternalBlue-class exploits |
| FTP       | ❌ None     | ❌ Plaintext passwords | Credential sniffing, session hijacking                |
| HTTP      | ❌ None     | N/A                   | Content inspection, MITM                              |

### Safe usage guidelines

✅ **DO:**
- Use on isolated/air-gapped networks
- Use on trusted home networks behind NAT
- Share only files you'd be comfortable making public
- Disable SMB when not actively needed

❌ **DON'T:**
- Expose to the internet (even behind a firewall)
- Share sensitive documents, passwords, or personal data
- Use on corporate/enterprise networks
- Assume any transferred data is private

### Network isolation recommendations

For maximum safety, consider running Depot on:
- A dedicated VLAN for retro machines
- A separate physical network segment
- A VM or container with restricted network access

**You have been warned.** Use responsibly.

## License

MIT
