# tests/lib/utils.sh - General test utilities
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/utils.sh"

# Compute MD5 hash (cross-platform: Linux, macOS, fallback to openssl)
compute_md5() {
    local file="$1"
    if command -v md5sum &>/dev/null; then
        md5sum "$file" | cut -d' ' -f1
    elif command -v md5 &>/dev/null; then
        md5 -q "$file"
    else
        openssl md5 "$file" | awk '{print $2}'
    fi
}

# Wait for a TCP port to become available
wait_for_port() {
    local host="${1:-127.0.0.1}"
    local port="$2"
    local timeout="${3:-30}"  # deciseconds (30 = 3 seconds)
    
    local retries=$timeout
    while ! nc -z "$host" "$port" 2>/dev/null; do
        retries=$((retries - 1))
        if [[ $retries -le 0 ]]; then
            return 1
        fi
        sleep 0.1
    done
    return 0
}

# Create a temp file (cross-platform)
make_temp() {
    local suffix="${1:-.tmp}"
    mktemp --suffix="$suffix" 2>/dev/null || mktemp
}
