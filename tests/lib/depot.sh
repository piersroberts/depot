# tests/lib/depot.sh - Depot server management
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/depot.sh"
# Requires: lib/output.sh, lib/utils.sh

# Find project root (parent of tests/)
_find_project_dir() {
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    echo "$(dirname "$(dirname "$script_dir")")"
}

DEPOT_PROJECT_DIR="${DEPOT_PROJECT_DIR:-$(_find_project_dir)}"
DEPOT_PID=""
DEPOT_CONFIG=""

# Build depot if needed
depot_build() {
    local mode="${1:-release}"
    local binary="$DEPOT_PROJECT_DIR/target/$mode/depot"
    
    if [[ ! -f "$binary" ]]; then
        info "Building depot ($mode)..."
        (cd "$DEPOT_PROJECT_DIR" && cargo build --$mode --quiet)
    fi
    echo "$binary"
}

# Start depot server
# Usage: depot_start <config_file> [port] [host]
depot_start() {
    local config="$1"
    local port="${2:-4450}"
    local host="${3:-127.0.0.1}"
    
    DEPOT_CONFIG="$config"
    
    info "Starting depot server on port $port..."
    
    # Run from project dir so relative paths in config work
    (cd "$DEPOT_PROJECT_DIR" && "$DEPOT_PROJECT_DIR/target/release/depot" -c "$config" > /dev/null 2>&1) &
    DEPOT_PID=$!
    
    # Wait for server to be ready
    if ! wait_for_port "$host" "$port" 30; then
        if ! kill -0 "$DEPOT_PID" 2>/dev/null; then
            echo "Depot server failed to start"
            return 1
        fi
        echo "Timeout waiting for depot server on port $port"
        return 1
    fi
    
    pass "Depot server started (PID: $DEPOT_PID)"
    return 0
}

# Stop depot server
depot_stop() {
    if [[ -n "$DEPOT_PID" ]] && kill -0 "$DEPOT_PID" 2>/dev/null; then
        info "Stopping depot server (PID: $DEPOT_PID)..."
        kill "$DEPOT_PID" 2>/dev/null || true
        wait "$DEPOT_PID" 2>/dev/null || true
        DEPOT_PID=""
    fi
}

# Get depot PID (for external cleanup handlers)
depot_pid() {
    echo "$DEPOT_PID"
}
