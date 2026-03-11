# Privileged Ports

SMB (port 445) and FTP (port 21) require elevated privileges on Unix systems. There are several secure ways to handle this without running Depot as root.

## Option 1: setcap (Linux - Recommended)

Grant the binary capability to bind to privileged ports without running as root:

```bash
# Build release binary
cargo build --release

# Grant capability (run once after each build)
sudo setcap 'cap_net_bind_service=+ep' target/release/depot

# Run as normal user
./target/release/depot
```

**Note:** You need to re-run `setcap` after every rebuild.

## Option 2: authbind (Linux)

```bash
sudo apt-get install authbind
sudo touch /etc/authbind/byport/445
sudo chmod 500 /etc/authbind/byport/445
sudo chown $USER /etc/authbind/byport/445

authbind --deep ./target/release/depot
```

To bind multiple ports (e.g., 445 and 21):

```bash
for port in 445 21; do
    sudo touch /etc/authbind/byport/$port
    sudo chmod 500 /etc/authbind/byport/$port
    sudo chown $USER /etc/authbind/byport/$port
done
```

## Option 3: iptables Port Forwarding (Linux)

Redirect port 445 to 4450 without running as root:

```bash
# Forward port 445 → 4450
sudo iptables -t nat -A PREROUTING -p tcp --dport 445 -j REDIRECT --to-port 4450
sudo iptables -t nat -A OUTPUT -p tcp --dport 445 -j REDIRECT --to-port 4450

# Then run depot on port 4450 (default)
./target/release/depot
```

To make the iptables rules persistent:

```bash
# Debian/Ubuntu
sudo apt-get install iptables-persistent
sudo netfilter-persistent save

# RHEL/CentOS
sudo service iptables save
```

## Option 4: pfctl (macOS)

Create a port forwarding rule:

```bash
# Create pf rule file
echo "rdr pass on lo0 inet proto tcp from any to any port 445 -> 127.0.0.1 port 4450" | sudo pfctl -ef -

# Run depot on port 4450
./target/release/depot
```

To make this persistent, create `/etc/pf.anchors/depot`:

```
rdr pass on lo0 inet proto tcp from any to any port 445 -> 127.0.0.1 port 4450
```

Then add to `/etc/pf.conf`:

```
rdr-anchor "depot"
load anchor "depot" from "/etc/pf.anchors/depot"
```

## Option 5: Use Non-Privileged Ports

The simplest option is to use the default non-privileged ports:

| Protocol | Default Port | Standard Port |
|----------|-------------|---------------|
| FTP      | 2121        | 21            |
| HTTP     | 8080        | 80            |
| SMB      | 4450        | 445           |

Most clients can connect to non-standard ports. For SMB, see [Connecting](CONNECTING.md) for Windows-specific workarounds.

## See Also

- [Running as a Service](RUNNING_AS_SERVICE.md) - For production deployments
- [Connecting](CONNECTING.md) - Client connection guides
