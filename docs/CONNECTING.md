# Connecting to Depot

This guide covers how to connect to Depot from various clients and operating systems.

## SMB/CIFS (Windows File Sharing)

### Windows 98/ME/2000/XP

Windows expects SMB on port 445. If Depot is running on the default port 4450, you have several options:

**Option 1: Run Depot on port 445** (recommended)

See [Privileged Ports](PRIVILEGED_PORTS.md) for how to bind to port 445 without running as root.

**Option 2: Use port forwarding on the server**

```bash
# Linux
sudo iptables -t nat -A PREROUTING -p tcp --dport 445 -j REDIRECT --to-port 4450

# macOS  
echo "rdr pass on lo0 inet proto tcp from any to any port 445 -> 127.0.0.1 port 4450" | sudo pfctl -ef -
```

**Option 3: SSH Tunnel** (if XP has an SSH client)

```
ssh -L 445:server:4450 user@gateway
```

Then connect to `\\localhost\sharename`.

**Option 4: Use FTP instead**

Windows XP's Explorer supports FTP natively:
- Open Explorer
- Type in address bar: `ftp://server:2121/`

### Connecting from Windows

Once port 445 is accessible:

```
# In Explorer address bar
\\server\sharename

# Or map a network drive
net use M: \\server\sharename
```

### Windows 7 and Later

Modern Windows may have SMB1 disabled. To enable:

1. Open **Control Panel** → **Programs** → **Turn Windows features on or off**
2. Enable **SMB 1.0/CIFS File Sharing Support**
3. Reboot

Or via PowerShell (admin):

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol
```

### Troubleshooting SMB

**"Windows cannot access \\server"**
- Check if Depot is running and SMB is enabled
- Verify firewall allows port 445 (or your custom port)
- Ensure SMB1 client is enabled on Windows 7+

**"The specified network name is no longer available"**
- This usually indicates a protocol mismatch
- Verify Depot's SMB is enabled in config

## FTP

### Command-Line FTP

```bash
ftp server 2121
# Login: anonymous (or your username)
# Password: (blank for anonymous, or your password)
```

### Windows Explorer

Type in the address bar:
```
ftp://server:2121/
```

For authenticated access:
```
ftp://username:password@server:2121/
```

### Classic FTP Clients

Depot is compatible with vintage FTP clients:

- **Windows 3.11**: WS_FTP, CuteFTP
- **Amiga**: AmiFTP, AmiTradeCenter
- **DOS**: NCSA FTP
- **Mac OS 9**: Fetch, Anarchie

**Tips for retro clients:**
- Use **passive mode** if behind NAT
- Depot uses classic LIST format for maximum compatibility

### Passive Mode

If your client is behind NAT or a firewall, enable passive mode. Depot's passive port range defaults to 60000-60100.

Ensure these ports are accessible through any firewalls.

## HTTP

### Web Browsers

Simply navigate to:
```
http://server:8080/
```

### Retro Browsers

Depot generates HTML 3.2 compatible output that works with:

- **NCSA Mosaic** (1993+)
- **Netscape Navigator** 1.x-4.x
- **Internet Explorer** 3.0+
- **Lynx** (text-mode)
- **Opera** 3.x+

**Tips:**
- Enable `retro_compatible = true` in config for pure HTML 3.2 (no CSS)
- Depot uses HTTP/1.0 compatible responses

### wget/curl

```bash
# List directory
curl http://server:8080/

# Download file
curl -O http://server:8080/path/to/file.zip
wget http://server:8080/path/to/file.zip
```

### Authenticated Access

If `require_auth = true` is set:

```bash
curl -u username:password http://server:8080/
wget --user=username --password=password http://server:8080/
```

In a browser, you'll be prompted for credentials.

## Streaming Media

Depot works great for streaming media to retro music players:

### Winamp / foobar2000

1. Map the SMB share as a network drive: `net use M: \\server\music`
2. Add `M:\` to your media library
3. Album art (`folder.jpg`) and ID3 tags work natively

### Windows Media Player

WMP can browse SMB shares directly:
1. Open WMP
2. Navigate to `\\server\sharename` via the address bar or library

### VLC

VLC can stream directly from any protocol:
- SMB: `smb://server/sharename/file.mp3`
- HTTP: `http://server:8080/file.mp3`
- FTP: `ftp://server:2121/file.mp3`

## See Also

- [Privileged Ports](PRIVILEGED_PORTS.md) - Binding to ports 445, 21, 80
- [Running as a Service](RUNNING_AS_SERVICE.md) - Production deployment
