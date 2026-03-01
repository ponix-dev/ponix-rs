#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}      Ponix API Test Suite               ${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""

# Check prerequisites
check_prerequisites

# Track results
PASSED=0
FAILED=0
SKIPPED=0

run_test() {
    local script="$1"
    local name="$2"
    local optional="${3:-false}"

    echo -e "${BLUE}------------------------------------------${NC}"
    echo -e "${YELLOW}Running: $name${NC}"
    echo -e "${BLUE}------------------------------------------${NC}"

    set +e
    "$SCRIPT_DIR/$script"
    local exit_code=$?
    set -e

    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}[PASSED] $name${NC}"
        ((PASSED++))
    elif [ "$optional" = "true" ]; then
        echo -e "${YELLOW}[SKIPPED] $name (optional)${NC}"
        ((SKIPPED++))
    else
        echo -e "${RED}[FAILED] $name${NC}"
        ((FAILED++))
    fi

    echo ""
}

# Run core API tests
run_test "test-auth.sh" "Authentication Tests"
run_test "test-organizations.sh" "Organization Tests"
run_test "test-workspaces.sh" "Workspace Tests"
run_test "test-data-streams.sh" "Data Stream Tests"
run_test "test-gateways.sh" "Gateway Tests"

# MQTT pipeline tests
run_test "test-mqtt-pipeline.sh" "MQTT Pipeline Test"
run_test "test-multi-contract-pipeline.sh" "Multi-Contract Pipeline Test"

# Summary
echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}              Test Summary               ${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All required tests passed!${NC}"
    exit 0
fi
