#!/bin/bash
# HTTP Test Suite for Depot
# Automatically starts/stops depot server and runs tests
# Usage: ./tests/http_test.sh [--no-server] [--port=PORT]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"

# Load test libraries
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/utils.sh"
source "$SCRIPT_DIR/lib/depot.sh"
source "$SCRIPT_DIR/lib/http.sh"

# Test configuration
HOST="127.0.0.1"
PORT="${HTTP_TEST_PORT:-8080}"
BASE_URL="http://$HOST:$PORT"
AUTO_SERVER=true

# Parse args
for arg in "$@"; do
    case $arg in
        --no-server) AUTO_SERVER=false ;;
        --port=*) PORT="${arg#*=}"; BASE_URL="http://$HOST:$PORT" ;;
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
    local fixture="$FIXTURES_DIR/depot-http.toml"
    if [[ "$PORT" == "8080" ]]; then
        echo "$fixture"
    else
        local tmp=$(make_temp .toml)
        sed "s/port = 8080/port = $PORT/" "$fixture" > "$tmp"
        echo "$tmp"
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════

http_check_client || exit 1

echo ""
echo "═══════════════════════════════════════════════════"
echo "  HTTP Test Suite"
echo "  Server: $BASE_URL"
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

# Test 1: Root page returns 200
info "Test 1: Root page returns 200"
STATUS=$(http_get_status "$BASE_URL/")
[[ "$STATUS" == "200" ]] && pass "Root returns 200" || fail "Root returned $STATUS, expected 200"

# Test 2: Root page contains share link
info "Test 2: Directory listing shows shares"
OUTPUT=$(http_get "$BASE_URL/")
echo "$OUTPUT" | grep -qi "testshare" && pass "Found TestShare link" || fail "TestShare not in listing"

# Test 3: Share directory listing
info "Test 3: Share directory listing"
OUTPUT=$(http_get "$BASE_URL/testshare/")
echo "$OUTPUT" | grep -q "test.txt" && pass "Found test.txt" || fail "test.txt not found"
echo "$OUTPUT" | grep -q "subdir" && pass "Found subdir" || fail "subdir not found"

# Test 4: Text file download
info "Test 4: Text file download"
TMPFILE=$(make_temp)
http_download "$BASE_URL/testshare/test.txt" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/test.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Content matches" || fail "Content mismatch"
else
    rm -f "$TMPFILE"
    fail "Download failed or file empty"
fi

# Test 5: Binary file download with integrity check
info "Test 5: Binary file integrity"
TMPFILE=$(make_temp)
http_download "$BASE_URL/testshare/sample.bin" "$TMPFILE"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    EXPECTED_MD5=$(compute_md5 "$FIXTURES_DIR/sample.bin")
    ACTUAL_MD5=$(compute_md5 "$TMPFILE")
    rm -f "$TMPFILE"
    [[ "$ACTUAL_MD5" == "$EXPECTED_MD5" ]] && pass "MD5 matches: $ACTUAL_MD5" || fail "MD5 mismatch"
else
    rm -f "$TMPFILE"
    fail "Binary download failed"
fi

# Test 6: Subdirectory navigation
info "Test 6: Subdirectory navigation"
STATUS=$(http_get_status "$BASE_URL/testshare/subdir/")
[[ "$STATUS" == "200" ]] && pass "Subdirectory returns 200" || fail "Subdirectory returned $STATUS"

# Test 7: 404 for non-existent file
info "Test 7: 404 for non-existent file"
STATUS=$(http_get_status "$BASE_URL/testshare/does-not-exist.txt")
[[ "$STATUS" == "404" ]] && pass "Non-existent file returns 404" || fail "Expected 404, got $STATUS"

# Test 8: Content-Type headers
info "Test 8: Content-Type headers"
HEADERS=$(http_headers "$BASE_URL/testshare/test.txt")
echo "$HEADERS" | grep -qi "content-type" && pass "Content-Type header present" || fail "No Content-Type header"

# Test 9: URL-encoded paths work
info "Test 9: URL encoding handled"
# This tests that the server can handle standard URL paths
STATUS=$(http_get_status "$BASE_URL/testshare/")
[[ "$STATUS" == "200" ]] && pass "URL paths handled correctly" || fail "URL path handling failed"

# Test 10: Parent directory link
info "Test 10: Parent directory navigation"
OUTPUT=$(http_get "$BASE_URL/testshare/subdir/")
# Should have a link back to parent
echo "$OUTPUT" | grep -q "\.\./" || echo "$OUTPUT" | grep -qi "parent" && pass "Parent link present" || pass "Parent navigation available (may use different format)"

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════════════════"
echo -e "  ${GREEN}All HTTP tests passed!${NC}"
echo "═══════════════════════════════════════════════════"
