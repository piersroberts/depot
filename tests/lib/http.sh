# tests/lib/http.sh - HTTP test utilities
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/http.sh"
# Requires: lib/output.sh

# Check if curl is available
http_check_client() {
    if ! command -v curl &>/dev/null; then
        echo "Error: curl is required for HTTP tests"
        echo "Install with: brew install curl (macOS) or apt install curl (Linux)"
        return 1
    fi
    return 0
}

# GET request
# Usage: http_get <url> [user] [pass]
http_get() {
    local url="$1"
    local user="$2"
    local pass="$3"
    
    if [[ -n "$user" ]]; then
        curl -s -u "${user}:${pass}" "$url"
    else
        curl -s "$url"
    fi
}

# GET request with status code
# Usage: http_get_status <url> [user] [pass]
http_get_status() {
    local url="$1"
    local user="$2"
    local pass="$3"
    
    if [[ -n "$user" ]]; then
        curl -s -o /dev/null -w "%{http_code}" -u "${user}:${pass}" "$url"
    else
        curl -s -o /dev/null -w "%{http_code}" "$url"
    fi
}

# Download file
# Usage: http_download <url> <local_path> [user] [pass]
http_download() {
    local url="$1"
    local local_path="$2"
    local user="$3"
    local pass="$4"
    
    if [[ -n "$user" ]]; then
        curl -s -o "$local_path" -u "${user}:${pass}" "$url"
    else
        curl -s -o "$local_path" "$url"
    fi
}

# Get HTTP headers
# Usage: http_headers <url> [user] [pass]
http_headers() {
    local url="$1"
    local user="$2"
    local pass="$3"
    
    if [[ -n "$user" ]]; then
        curl -sI -u "${user}:${pass}" "$url"
    else
        curl -sI "$url"
    fi
}

# Check if response contains text
# Usage: http_contains <url> <text> [user] [pass]
http_contains() {
    local url="$1"
    local text="$2"
    local user="$3"
    local pass="$4"
    
    http_get "$url" "$user" "$pass" | grep -q "$text"
}
