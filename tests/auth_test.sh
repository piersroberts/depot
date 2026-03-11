#!/bin/bash
# Authentication Test Suite for Depot
# Tests HTTP and FTP authentication scenarios
# Usage: ./tests/auth_test.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Load test libraries
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/utils.sh"
source "$SCRIPT_DIR/lib/depot.sh"
source "$SCRIPT_DIR/lib/http.sh"
source "$SCRIPT_DIR/lib/ftp.sh"

# Test configuration
HOST="127.0.0.1"
HTTP_PORT="${HTTP_AUTH_PORT:-8081}"
FTP_PORT="${FTP_AUTH_PORT:-2122}"

# Test user credentials (will be added to config)
TEST_USER="testuser"
TEST_PASS="testpass123"

# Cleanup function
cleanup() {
    depot_stop
    rm -f "$TEST_CONFIG" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# ═══════════════════════════════════════════════════════════════════════════════
# Setup
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════════════════"
echo "  Authentication Test Suite"
echo "═══════════════════════════════════════════════════"
echo ""

# Build depot
DEPOT=$(depot_build release)
info "Using depot binary: $DEPOT"

# Create test config with auth enabled and a test user
TEST_CONFIG=$(make_temp .toml)

# First create base config
cat > "$TEST_CONFIG" << EOF
server_name = "TestDepot-Auth"
log_level = "warn"

[shares.TestShare]
path = "tests/fixtures"
virtual_path = "/testshare"
read_only = true
enabled = true

[protocols.ftp]
enabled = true
port = $FTP_PORT
bind_address = "127.0.0.1"
pasv_ports_start = 30020
pasv_ports_end = 30030
anonymous = false

[protocols.http]
enabled = true
port = $HTTP_PORT
bind_address = "127.0.0.1"
require_auth = true

[protocols.smb]
enabled = false

[admin]
enabled = false
EOF

# Add test user using expect (for non-interactive password input)
info "Creating test user..."
if command -v expect &>/dev/null; then
    expect << EOF >/dev/null 2>&1
spawn $DEPOT -c $TEST_CONFIG add-user $TEST_USER
expect "Enter password:"
send "$TEST_PASS\r"
expect "Confirm password:"
send "$TEST_PASS\r"
expect eof
EOF
else
    # Fallback: try piping (works on some systems)
    printf "%s\n%s\n" "$TEST_PASS" "$TEST_PASS" | "$DEPOT" -c "$TEST_CONFIG" add-user "$TEST_USER" >/dev/null 2>&1 || {
        fail "Could not create test user. Install 'expect' for reliable password input."
    }
fi

# Verify user was created
if ! grep -q "testuser" "$TEST_CONFIG"; then
    fail "Test user was not created in config"
fi
pass "Test user created"

# Grant access to the test share
"$DEPOT" -c "$TEST_CONFIG" grant "$TEST_USER" "TestShare" >/dev/null 2>&1
pass "User granted access to TestShare"

# Start server
depot_start "$TEST_CONFIG" "$HTTP_PORT" "$HOST"

# ═══════════════════════════════════════════════════════════════════════════════
# HTTP Authentication Tests
# ═══════════════════════════════════════════════════════════════════════════════

info "HTTP Authentication Tests"
echo ""

BASE_URL="http://$HOST:$HTTP_PORT"

# Test 1: No credentials returns 401
info "Test 1: HTTP without credentials returns 401"
STATUS=$(http_get_status "$BASE_URL/")
[[ "$STATUS" == "401" ]] && pass "No auth returns 401" || fail "Expected 401, got $STATUS"

# Test 2: Wrong credentials returns 401
info "Test 2: HTTP with wrong credentials returns 401"
STATUS=$(http_get_status "$BASE_URL/" "wronguser" "wrongpass")
[[ "$STATUS" == "401" ]] && pass "Wrong credentials returns 401" || fail "Expected 401, got $STATUS"

# Test 3: Wrong password returns 401
info "Test 3: HTTP with wrong password returns 401"
STATUS=$(http_get_status "$BASE_URL/" "$TEST_USER" "wrongpassword")
[[ "$STATUS" == "401" ]] && pass "Wrong password returns 401" || fail "Expected 401, got $STATUS"

# Test 4: Correct credentials returns 200
info "Test 4: HTTP with correct credentials returns 200"
STATUS=$(http_get_status "$BASE_URL/" "$TEST_USER" "$TEST_PASS")
[[ "$STATUS" == "200" ]] && pass "Correct auth returns 200" || fail "Expected 200, got $STATUS"

# Test 5: Can access protected content with auth
info "Test 5: HTTP authenticated file download"
TMPFILE=$(make_temp)
http_download "$BASE_URL/testshare/test.txt" "$TMPFILE" "$TEST_USER" "$TEST_PASS"
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/test.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Authenticated download successful" || fail "Content mismatch"
else
    rm -f "$TMPFILE"
    fail "Authenticated download failed"
fi

# Test 6: WWW-Authenticate header is present on 401
info "Test 6: WWW-Authenticate header present"
HEADERS=$(curl -sI "$BASE_URL/" 2>/dev/null)
echo "$HEADERS" | grep -qi "WWW-Authenticate" && pass "WWW-Authenticate header present" || fail "Missing WWW-Authenticate header"

echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# FTP Authentication Tests
# ═══════════════════════════════════════════════════════════════════════════════

info "FTP Authentication Tests"
echo ""

# Test 7: Anonymous login disabled
info "Test 7: FTP anonymous access denied"
OUTPUT=$(curl -s -m 5 --list-only "ftp://anonymous:@$HOST:$FTP_PORT/" 2>&1 || true)
# Should fail or return error
echo "$OUTPUT" | grep -qi "530\|denied\|failed\|Access denied" && pass "Anonymous access denied" || {
    # Also check if curl reported a 530 error
    if [[ -z "$OUTPUT" ]]; then
        pass "Anonymous access denied (connection rejected)"
    else
        fail "Anonymous should be denied, got: $OUTPUT"
    fi
}

# Test 8: Wrong credentials denied
info "Test 8: FTP wrong credentials denied"
OUTPUT=$(curl -s -m 5 --list-only "ftp://wronguser:wrongpass@$HOST:$FTP_PORT/" 2>&1 || true)
echo "$OUTPUT" | grep -qi "530\|denied\|failed\|Access denied" && pass "Wrong credentials denied" || {
    if [[ -z "$OUTPUT" ]]; then
        pass "Wrong credentials denied (connection rejected)"
    else
        fail "Wrong credentials should be denied"
    fi
}

# Test 9: Correct credentials allowed
info "Test 9: FTP with correct credentials"
OUTPUT=$(curl -s -m 5 --list-only "ftp://${TEST_USER}:${TEST_PASS}@$HOST:$FTP_PORT/" 2>&1)
if echo "$OUTPUT" | grep -qi "testshare"; then
    pass "Authenticated FTP listing works"
else
    # Check if it's an auth failure
    if echo "$OUTPUT" | grep -qi "530\|denied"; then
        fail "Auth failed - credentials not recognized"
    else
        fail "FTP listing failed: $OUTPUT"
    fi
fi

# Test 10: Can download file with auth
info "Test 10: FTP authenticated file download"
TMPFILE=$(make_temp)
curl -s -m 10 -o "$TMPFILE" "ftp://${TEST_USER}:${TEST_PASS}@$HOST:$FTP_PORT/testshare/test.txt" 2>/dev/null
if [[ -f "$TMPFILE" ]] && [[ -s "$TMPFILE" ]]; then
    CONTENT=$(cat "$TMPFILE")
    EXPECTED=$(cat "$FIXTURES_DIR/test.txt")
    rm -f "$TMPFILE"
    [[ "$CONTENT" == "$EXPECTED" ]] && pass "Authenticated FTP download works" || fail "Content mismatch"
else
    rm -f "$TMPFILE"
    fail "FTP download failed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════════════════"
echo -e "  ${GREEN}All authentication tests passed!${NC}"
echo "═══════════════════════════════════════════════════"
