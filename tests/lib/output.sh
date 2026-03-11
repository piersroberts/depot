# tests/lib/output.sh - Terminal output helpers
# Source this file: source "$(dirname "${BASH_SOURCE[0]}")/lib/output.sh"

# Colors (disable in CI for cleaner logs)
if [[ -t 1 ]] && [[ -z "$CI" ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

pass() { echo -e "${GREEN}✓ $1${NC}"; }
fail() { echo -e "${RED}✗ $1${NC}"; [[ "$(type -t cleanup)" == "function" ]] && cleanup; exit 1; }
info() { echo -e "${YELLOW}→ $1${NC}"; }
debug() { [[ -n "$DEBUG" ]] && echo -e "${BLUE}# $1${NC}"; }
