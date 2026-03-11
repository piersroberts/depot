#!/bin/bash
# SMB1 Test Suite for Depot
# Automatically starts/stops depot server and runs tests
# Usage: ./tests/smb_test.sh [--no-server] [--port=PORT]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"

# Load test libraries
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/utils.sh"
source "$SCRIPT_DIR/lib/depot.sh"
source "$SCRIPT_DIR/lib/smb.sh"

# Test configuration
HOST="127.0.0.1"
PORT="${SMB_TEST_PORT:-4450}"
SHARE="TestShare"
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
    smb_cleanup
    depot_stop
}
trap cleanup EXIT INT TERM

# Setup depot config from fixture (with optional port override)
setup_test_config() {
    local fixture="$FIXTURES_DIR/depot-test.toml"
    if [[ "$PORT" == "4450" ]]; then
        echo "$fixture"
    else
        local tmp=$(make_temp .toml)
        sed "s/port = 4450/port = $PORT/" "$fixture" > "$tmp"
        echo "$tmp"
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════

smb_check_client || exit 1
smb_setup_config

echo ""
echo "═══════════════════════════════════════════════════"
echo "  SMB1 Test Suite"
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

# Test 1: Directory listing
info "Test 1: Directory listing"
OUTPUT=$(smb_ls "$HOST" "$SHARE" "$PORT")
echo "$OUTPUT" | grep -q "test.txt" && pass "Found test.txt" || fail "test.txt not found in: $OUTPUT"
echo "$OUTPUT" | grep -q "subdir" && pass "Found subdir/" || fail "subdir not found"
echo "$OUTPUT" | grep -q "sample.bin" && pass "Found sample.bin" || fail "sample.bin not found"

# Test 2: Text file download
info "Test 2: Text file download"
TMPFILE=$(make_temp)
smb_get "$HOST" "$SHARE" "$PORT" "test.txt" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/test.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Content matches" || fail "Content mismatch: got '$CONTENT', expected '$EXPECTED'"
else
    rm -f "$TMPFILE"
    fail "Download failed"
fi

# Test 3: Binary file integrity
info "Test 3: Binary file integrity"
TMPFILE=$(make_temp)
smb_get "$HOST" "$SHARE" "$PORT" "sample.bin" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    LOCAL_MD5=$(compute_md5 "$FIXTURES_DIR/sample.bin")
    REMOTE_MD5=$(compute_md5 "$TMPFILE")
    rm -f "$TMPFILE"
    [[ "$LOCAL_MD5" == "$REMOTE_MD5" ]] && pass "Binary integrity verified (MD5: $LOCAL_MD5)" || fail "MD5 mismatch: local=$LOCAL_MD5, remote=$REMOTE_MD5"
else
    rm -f "$TMPFILE"
    fail "Binary download failed"
fi

# Test 4: Subdirectory file download
info "Test 4: Subdirectory file download"
TMPFILE=$(make_temp)
smb_get "$HOST" "$SHARE" "$PORT" "subdir/nested.txt" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/subdir/nested.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Nested content matches" || fail "Nested content mismatch: '$CONTENT'"
else
    rm -f "$TMPFILE"
    fail "Subdir download failed"
fi

# Test 5: File size verification  
info "Test 5: File size verification"
TMPFILE=$(make_temp)
smb_get "$HOST" "$SHARE" "$PORT" "sample.bin" "$TMPFILE"
SIZE=$(wc -c < "$TMPFILE" | tr -d ' ')
rm -f "$TMPFILE"
[[ "$SIZE" == "1024" ]] && pass "Binary file size correct (1024 bytes)" || fail "Size mismatch: expected 1024, got $SIZE"

# Test 6: Multiple file downloads
info "Test 6: Multiple sequential downloads"
for i in 1 2 3; do
    TMPFILE=$(make_temp)
    smb_get "$HOST" "$SHARE" "$PORT" "test.txt" "$TMPFILE"
    if [[ ! -s "$TMPFILE" ]]; then
        rm -f "$TMPFILE"
        fail "Sequential download $i failed"
    fi
    rm -f "$TMPFILE"
done
pass "3 sequential downloads successful"

# Test 7: Path with backslash separators (Windows-style)
info "Test 7: Windows-style path separators"
TMPFILE=$(make_temp)
# Use backslash separator like Windows clients do
smbclient "//$HOST/$SHARE" -N -s "$SMB_CONF" -p "$PORT" -c 'get subdir\nested.txt '"$TMPFILE" > /dev/null 2>&1
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/subdir/nested.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Backslash path worked" || fail "Content mismatch"
else
    rm -f "$TMPFILE"
    fail "Backslash path download failed"
fi

# Test 8: Deep path file access
info "Test 8: Download from subdirectory path"
TMPFILE=$(make_temp)
# Use full path to get file from subdirectory
smb_get "$HOST" "$SHARE" "$PORT" 'subdir/nested.txt' "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/subdir/nested.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Subdirectory path download worked" || fail "Content mismatch"
else
    rm -f "$TMPFILE"
    fail "Subdirectory path download failed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════════════════"
echo -e "  ${GREEN}All tests passed!${NC}"
echo "═══════════════════════════════════════════════════"
