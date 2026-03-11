# tests/lib/smb.sh - SMB/smbclient test helpers
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/smb.sh"
# Requires: lib/output.sh

SMB_CONF=""

# Check for smbclient, with install hints
smb_check_client() {
    if command -v smbclient &>/dev/null; then
        return 0
    fi
    
    echo "smbclient not found."
    if [[ "$(uname)" == "Darwin" ]]; then
        echo "Install with: brew install samba"
    else
        echo "Install with: apt-get install smbclient (Debian/Ubuntu)"
        echo "           or: dnf install samba-client (Fedora/RHEL)"
    fi
    return 1
}

# Create SMB1 config (modern smbclient defaults to SMB2+)
# Sets SMB_CONF variable
smb_setup_config() {
    SMB_CONF=$(mktemp)
    cat > "$SMB_CONF" << 'EOF'
[global]
client min protocol = NT1
client max protocol = NT1
EOF
}

# Cleanup SMB config
smb_cleanup() {
    if [[ -n "$SMB_CONF" ]] && [[ -f "$SMB_CONF" ]]; then
        rm -f "$SMB_CONF"
        SMB_CONF=""
    fi
}

# Run smbclient command with SMB1 config
# Usage: smb_run <host> <share> <port> <command>
smb_run() {
    local host="$1"
    local share="$2"
    local port="$3"
    shift 3
    local cmd="$*"
    
    smbclient "//$host/$share" -N -s "$SMB_CONF" -p "$port" -c "$cmd" 2>&1
}

# Download file via SMB
# Usage: smb_get <host> <share> <port> <remote_path> <local_path>
smb_get() {
    local host="$1"
    local share="$2"
    local port="$3"
    local remote="$4"
    local local_path="$5"
    
    smbclient "//$host/$share" -N -s "$SMB_CONF" -p "$port" -c "get $remote $local_path" > /dev/null 2>&1
}

# List directory via SMB
# Usage: smb_ls <host> <share> <port> [path]
smb_ls() {
    local host="$1"
    local share="$2"
    local port="$3"
    local path="${4:-.}"
    
    if [[ "$path" == "." ]]; then
        smb_run "$host" "$share" "$port" "ls" || true
    else
        smb_run "$host" "$share" "$port" "cd $path; ls" || true
    fi
}
