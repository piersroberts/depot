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

| Platform | Download |
|----------|----------|
| Linux (x86_64) | [depot-x86_64-unknown-linux-gnu.tar.gz](../../releases/latest/download/depot-x86_64-unknown-linux-gnu.tar.gz) |
| macOS (Intel) | [depot-x86_64-apple-darwin.tar.gz](../../releases/latest/download/depot-x86_64-apple-darwin.tar.gz) |
| macOS (Apple Silicon) | [depot-aarch64-apple-darwin.tar.gz](../../releases/latest/download/depot-aarch64-apple-darwin.tar.gz) |
| Windows (x86_64) | [depot-x86_64-pc-windows-msvc.zip](../../releases/latest/download/depot-x86_64-pc-windows-msvc.zip) |

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

**Note:** SMB is disabled by default. Standard SMB uses port 445, which requires elevated privileges. See [Privileged Ports](#privileged-ports) below.

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

To connect from Windows to a non-standard port (4450), you can either:
1. Use port forwarding (iptables/pfctl) to redirect 445 → 4450 on the server
2. Run depot on port 445 with elevated privileges (see [Privileged Ports](#privileged-ports))

## Building

Requirements:
- Rust 1.70+
- CMake (for aws-lc-sys)

```bash
# Debug build
cargo build

# Release build (optimized for size)
cargo build --release

# Cross-compile for different targets
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
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

## Retro Compatibility Notes

### SMB Clients (Native Windows File Sharing)
- Guest authentication for hassle-free access
- Works with: Windows 98, ME, 2000, XP, Windows 7+
- Use: `\\server\sharename` or `net use M: \\server\share`
- Ideal for streaming music to Winamp, foobar2000, WMP
- Album artwork (`folder.jpg`) and ID3 tags work natively

### FTP Clients
- Use passive mode for clients behind NAT
- Classic LIST format for maximum compatibility
- Works with: Amiga FTP clients, Windows 3.11 WS_FTP, early FileZilla

### HTTP Clients
- HTML 3.2 output with simple tables
- No CSS required (optional styling for modern browsers)
- No JavaScript
- HTTP/1.0 compatible responses
- Works with: NCSA Mosaic, early Netscape, Internet Explorer 3+

## Planned Features

- [x] SMB1 protocol support (for Win95/98/XP shares)
- [ ] AppleTalk/AFP support (for classic Macs)
- [ ] SQLite indexing for large file collections
- [ ] File search functionality
- [ ] Transfer logging/statistics

## Privileged Ports

SMB (port 445) and FTP (port 21) require elevated privileges on Unix systems. There are several secure ways to handle this:

### Option 1: setcap (Linux - Recommended)

Grant the binary capability to bind to privileged ports without running as root:

```bash
# Build release binary
cargo build --release

# Grant capability (run once after each build)
sudo setcap 'cap_net_bind_service=+ep' target/release/depot

# Run as normal user
./target/release/depot
```

### Option 2: authbind (Linux)

```bash
sudo apt-get install authbind
sudo touch /etc/authbind/byport/445
sudo chmod 500 /etc/authbind/byport/445
sudo chown $USER /etc/authbind/byport/445

authbind --deep ./target/release/depot
```

### Option 3: iptables Port Forwarding (Linux)

Redirect port 445 to 4450 without running as root:

```bash
# Forward port 445 → 4450
sudo iptables -t nat -A PREROUTING -p tcp --dport 445 -j REDIRECT --to-port 4450
sudo iptables -t nat -A OUTPUT -p tcp --dport 445 -j REDIRECT --to-port 4450

# Then run depot on port 4450 (default)
./target/release/depot
```

### Option 4: pfctl (macOS)

```bash
# Create pf rule file
echo "rdr pass on lo0 inet proto tcp from any to any port 445 -> 127.0.0.1 port 4450" | sudo pfctl -ef -

# Run depot on port 4450
./target/release/depot
```

### Option 5: launchd (macOS - for production)

Create `/Library/LaunchDaemons/com.depot.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.depot.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/depot</string>
        <string>-c</string>
        <string>/etc/depot/depot.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>UserName</key>
    <string>_depot</string>
    <key>Sockets</key>
    <dict>
        <key>SMB</key>
        <dict>
            <key>SockServiceName</key>
            <string>445</string>
            <key>SockType</key>
            <string>stream</string>
        </dict>
    </dict>
</dict>
</plist>
```

### Windows XP Connection Notes

Windows XP always connects to SMB on port 445. If you can't use port 445:

1. **SSH Tunnel** (if XP has an SSH client):
   ```
   ssh -L 445:server:4450 user@gateway
   ```

2. **Run on port 445** using one of the methods above

3. **Use FTP instead** - XP's Explorer supports FTP natively:
   - Open Explorer, type: `ftp://server:2121/` in address bar

## Security Considerations

**Depot is intentionally insecure.** This is not a bug—it's the core design goal.

### Why?

Modern security protocols (SMB3, SFTP, HTTPS with TLS 1.3) are incompatible with vintage operating systems. A Windows 98 machine cannot negotiate TLS 1.2. A classic Mac cannot speak SMB3. Depot exists to bridge this gap.

### What's insecure?

| Protocol | Encryption | Authentication | Vulnerabilities |
|----------|------------|----------------|-----------------|
| SMB1/CIFS | ❌ None | ❌ Guest/plaintext | MITM, credential sniffing, EternalBlue-class exploits |
| FTP | ❌ None | ❌ Plaintext passwords | Credential sniffing, session hijacking |
| HTTP | ❌ None | N/A | Content inspection, MITM |

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
