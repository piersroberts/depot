# tests/lib/ftp.sh - FTP test utilities
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/ftp.sh"
# Requires: lib/output.sh

# Check if curl is available (used for FTP operations)
ftp_check_client() {
    if ! command -v curl &>/dev/null; then
        echo "Error: curl is required for FTP tests"
        echo "Install with: brew install curl (macOS) or apt install curl (Linux)"
        return 1
    fi
    return 0
}

# List directory contents via FTP
# Usage: ftp_ls <host> <port> [path] [user] [pass]
ftp_ls() {
    local host="$1"
    local port="$2"
    local path="${3:-/}"
    local user="${4:-anonymous}"
    local pass="${5:-}"
    
    curl -s --list-only "ftp://${user}:${pass}@${host}:${port}${path}" 2>/dev/null
}

# Download file via FTP
# Usage: ftp_get <host> <port> <remote_path> <local_path> [user] [pass]
ftp_get() {
    local host="$1"
    local port="$2"
    local remote="$3"
    local local_path="$4"
    local user="${5:-anonymous}"
    local pass="${6:-}"
    
    curl -s -o "$local_path" "ftp://${user}:${pass}@${host}:${port}${remote}" 2>/dev/null
}

# Check if FTP server is responding
# Usage: ftp_check_server <host> <port>
ftp_check_server() {
    local host="$1"
    local port="$2"
    
    # Try to connect and get banner
    curl -s --connect-timeout 2 "ftp://${host}:${port}/" >/dev/null 2>&1
}

# Get file size via FTP
# Usage: ftp_size <host> <port> <path> [user] [pass]
ftp_size() {
    local host="$1"
    local port="$2"
    local path="$3"
    local user="${4:-anonymous}"
    local pass="${5:-}"
    
    curl -sI "ftp://${user}:${pass}@${host}:${port}${path}" 2>/dev/null | grep -i "Content-Length" | awk '{print $2}' | tr -d '\r'
}
