# Running Depot as a Service

This guide covers how to run Depot as a background service on various operating systems.

## Linux (systemd)

### Basic Setup

1. Copy the binary to a system location:

```bash
sudo cp target/release/depot /usr/local/bin/
sudo chmod +x /usr/local/bin/depot
```

2. Create a configuration directory:

```bash
sudo mkdir -p /etc/depot
sudo cp depot.toml /etc/depot/
```

3. Create a systemd service file at `/etc/systemd/system/depot.service`:

```ini
[Unit]
Description=Depot File Server
After=network.target

[Service]
Type=simple
User=depot
Group=depot
ExecStart=/usr/local/bin/depot -c /etc/depot/depot.toml
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true
ReadWritePaths=/var/log/depot

# Allow binding to privileged ports (alternative to setcap)
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
```

4. Create a dedicated user:

```bash
sudo useradd -r -s /usr/sbin/nologin depot
```

5. Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable depot
sudo systemctl start depot
```

### Managing the Service

```bash
# Check status
sudo systemctl status depot

# View logs
sudo journalctl -u depot -f

# Restart after config changes
sudo systemctl restart depot

# Stop the service
sudo systemctl stop depot
```

### Using Privileged Ports with systemd

The `AmbientCapabilities=CAP_NET_BIND_SERVICE` line allows binding to ports below 1024. Alternatively, use socket activation:

```ini
# /etc/systemd/system/depot.socket
[Unit]
Description=Depot Socket

[Socket]
ListenStream=445
ListenStream=21
ListenStream=80

[Install]
WantedBy=sockets.target
```

## macOS (launchd)

### User Agent (runs when you log in)

Create `~/Library/LaunchAgents/com.depot.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.depot</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/depot</string>
        <string>-c</string>
        <string>/Users/YOUR_USERNAME/.config/depot/depot.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/depot.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/depot.error.log</string>
</dict>
</plist>
```

Load and start:

```bash
launchctl load ~/Library/LaunchAgents/com.depot.plist
```

### System Daemon (runs at boot, can use privileged ports)

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
    <key>KeepAlive</key>
    <true/>
    <key>UserName</key>
    <string>_depot</string>
    <key>GroupName</key>
    <string>_depot</string>
    <key>StandardOutPath</key>
    <string>/var/log/depot.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/depot.error.log</string>
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

Create the service user and load:

```bash
# Create service user (requires admin privileges)
sudo dscl . -create /Users/_depot
sudo dscl . -create /Users/_depot UserShell /usr/bin/false
sudo dscl . -create /Users/_depot UniqueID 299
sudo dscl . -create /Users/_depot PrimaryGroupID 299

# Create config directory
sudo mkdir -p /etc/depot
sudo cp depot.toml /etc/depot/

# Load the daemon
sudo launchctl load /Library/LaunchDaemons/com.depot.plist
```

### Managing launchd Services

```bash
# Check if running
launchctl list | grep depot

# Stop
launchctl unload ~/Library/LaunchAgents/com.depot.plist  # user agent
sudo launchctl unload /Library/LaunchDaemons/com.depot.plist  # daemon

# Start
launchctl load ~/Library/LaunchAgents/com.depot.plist
sudo launchctl load /Library/LaunchDaemons/com.depot.plist

# View logs
tail -f /var/log/depot.log
```

## Windows Service

### Using NSSM (Non-Sucking Service Manager)

1. Download [NSSM](https://nssm.cc/download)

2. Install Depot as a service:

```powershell
nssm install Depot "C:\Program Files\Depot\depot.exe" "-c" "C:\Program Files\Depot\depot.toml"
nssm set Depot DisplayName "Depot File Server"
nssm set Depot Description "Retro-compatible file sharing server"
nssm set Depot Start SERVICE_AUTO_START
nssm set Depot AppStdout "C:\Program Files\Depot\logs\depot.log"
nssm set Depot AppStderr "C:\Program Files\Depot\logs\depot.error.log"
```

3. Start the service:

```powershell
nssm start Depot
```

### Using sc.exe (Built-in)

```powershell
sc.exe create Depot binPath= "C:\Program Files\Depot\depot.exe -c C:\Program Files\Depot\depot.toml" start= auto DisplayName= "Depot File Server"
sc.exe start Depot
```

### Managing Windows Services

```powershell
# Check status
sc.exe query Depot

# Stop
sc.exe stop Depot

# Start  
sc.exe start Depot

# Remove
sc.exe delete Depot
```

## Docker

### Basic Dockerfile

```dockerfile
FROM rust:1.70 as builder
WORKDIR /build
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/depot /usr/local/bin/
EXPOSE 2121 8080 4450
CMD ["depot", "-c", "/etc/depot/depot.toml"]
```

### Docker Compose

```yaml
version: '3.8'
services:
  depot:
    build: .
    ports:
      - "2121:2121"   # FTP
      - "8080:8080"   # HTTP
      - "445:4450"    # SMB (host 445 -> container 4450)
    volumes:
      - ./depot.toml:/etc/depot/depot.toml:ro
      - /path/to/files:/data:ro
    restart: unless-stopped
```

### Running with Docker

```bash
# Build
docker build -t depot .

# Run
docker run -d \
  --name depot \
  -p 2121:2121 \
  -p 8080:8080 \
  -p 445:4450 \
  -v $(pwd)/depot.toml:/etc/depot/depot.toml:ro \
  -v /path/to/files:/data:ro \
  depot
```

## See Also

- [Privileged Ports](PRIVILEGED_PORTS.md) - Binding to ports 445, 21, 80
- [Building](BUILDING.md) - Compiling from source
