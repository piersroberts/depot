#!/bin/bash
# FTP Test Suite for Depot
# Automatically starts/stops depot server and runs tests
# Usage: ./tests/ftp_test.sh [--no-server] [--port=PORT]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"

# Load test libraries
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/utils.sh"
source "$SCRIPT_DIR/lib/depot.sh"
source "$SCRIPT_DIR/lib/ftp.sh"

# Test configuration
HOST="127.0.0.1"
PORT="${FTP_TEST_PORT:-2121}"
AUTO_SERVER=true

# Parse args
for arg in "$@"; do
    case $arg in
        --no-server) AUTO_SERVER=false ;;
        --port=*) PORT="${arg#*=}" ;;
        *) ;;
    esac
done

# Cleanup function
cleanup() {
    depot_stop
}
trap cleanup EXIT INT TERM

# Setup depot config from fixture (with optional port override)
setup_test_config() {
    local fixture="$FIXTURES_DIR/depot-ftp.toml"
    if [[ "$PORT" == "2121" ]]; then
        echo "$fixture"
    else
        local tmp=$(make_temp .toml)
        sed "s/port = 2121/port = $PORT/" "$fixture" > "$tmp"
        echo "$tmp"
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════

ftp_check_client || exit 1

echo ""
echo "═══════════════════════════════════════════════════"
echo "  FTP Test Suite"
echo "  Server: $HOST:$PORT"
echo "  Fixtures: $FIXTURES_DIR"
echo "═══════════════════════════════════════════════════"
echo ""

# Setup and start server if needed
if [[ "$AUTO_SERVER" == "true" ]]; then
    depot_build release > /dev/null
    TEST_CONFIG=$(setup_test_config)
    depot_start "$TEST_CONFIG" "$PORT" "$HOST"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Tests
# ═══════════════════════════════════════════════════════════════════════════════

# Test 1: Root directory listing
info "Test 1: Root directory listing"
OUTPUT=$(ftp_ls "$HOST" "$PORT" "/")
echo "$OUTPUT" | grep -qi "testshare" && pass "Found TestShare" || fail "TestShare not found in: $OUTPUT"

# Test 2: Share directory listing
info "Test 2: Share directory listing"
OUTPUT=$(ftp_ls "$HOST" "$PORT" "/testshare/")
echo "$OUTPUT" | grep -q "test.txt" && pass "Found test.txt" || fail "test.txt not found in: $OUTPUT"
echo "$OUTPUT" | grep -q "subdir" && pass "Found subdir/" || fail "subdir not found"

# Test 3: Text file download
info "Test 3: Text file download"
TMPFILE=$(make_temp)
ftp_get "$HOST" "$PORT" "/testshare/test.txt" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/test.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Content matches" || fail "Content mismatch: got '$CONTENT'"
else
    rm -f "$TMPFILE"
    fail "Download failed or file empty"
fi

# Test 4: Binary file download with integrity check
info "Test 4: Binary file integrity"
TMPFILE=$(make_temp)
ftp_get "$HOST" "$PORT" "/testshare/sample.bin" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    EXPECTED_MD5=$(compute_md5 "$FIXTURES_DIR/sample.bin")
    ACTUAL_MD5=$(compute_md5 "$TMPFILE")
    rm -f "$TMPFILE"
    [[ "$ACTUAL_MD5" == "$EXPECTED_MD5" ]] && pass "MD5 matches: $ACTUAL_MD5" || fail "MD5 mismatch: expected $EXPECTED_MD5, got $ACTUAL_MD5"
else
    rm -f "$TMPFILE"
    fail "Binary download failed"
fi

# Test 5: Subdirectory navigation
info "Test 5: Subdirectory navigation"
OUTPUT=$(ftp_ls "$HOST" "$PORT" "/testshare/subdir/")
# Just check we can list it without error
[[ -n "$OUTPUT" ]] || [[ $? -eq 0 ]] && pass "Subdirectory accessible" || fail "Cannot access subdirectory"

# Test 6: Non-existent file returns error
info "Test 6: Non-existent file handling"
TMPFILE=$(make_temp)
if ftp_get "$HOST" "$PORT" "/testshare/does-not-exist.txt" "$TMPFILE" 2>/dev/null; then
    # If curl succeeded, check if file is empty (404 behavior)
    if [[ ! -s "$TMPFILE" ]]; then
        pass "Non-existent file handled correctly"
    else
        fail "Should have failed for non-existent file"
    fi
else
    pass "Non-existent file returns error"
fi
rm -f "$TMPFILE"

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════════════════"
echo -e "  ${GREEN}All FTP tests passed!${NC}"
echo "═══════════════════════════════════════════════════"
