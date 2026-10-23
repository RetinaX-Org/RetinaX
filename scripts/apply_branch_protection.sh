#!/usr/bin/env bash

# Apply the main branch protection ruleset from .github/rulesets/main-branch-protection.json.
#
# Usage: ./scripts/apply_branch_protection.sh [--check | --dry-run] [--repo <owner/name>]
#
#   --check    Offline: verify every required status check matches a workflow job that runs on
#              every pull request. Needs no GitHub access. Exits non-zero on a mismatch.
#   --dry-run  Run --check, then show whether the ruleset would be created or updated.
#   (default)  Run --check, then create or update the ruleset. Needs `gh` logged in as a
#              repository admin.
#
# The ruleset is matched by name, so running the script again updates it in place.

set -euo pipefail

ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
RULESET_FILE="$ROOT_DIR/.github/rulesets/main-branch-protection.json"
WORKFLOWS_DIR="$ROOT_DIR/.github/workflows"
REPO="RetinaX-Org/RetinaX"
MODE="apply"

while [ $# -gt 0 ]; do
    case "$1" in
        --check) MODE="check" ;;
        --dry-run) MODE="dry-run" ;;
        --repo)
            REPO="${2:?--repo needs <owner/name>}"
            shift
            ;;
        -h | --help)
            sed -n '3,13p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

# Prints the `pull_request:` trigger block of a workflow, without its header line.
pull_request_block() {
    awk '
        /^on:/ { in_on = 1; next }
        in_on && /^[^ #]/ { in_on = 0 }
        in_on && /^  pull_request:/ { in_pr = 1; next }
        in_pr && /^  [^ ]/ { in_pr = 0 }
        in_pr { print }
    ' "$1"
}

check_required_checks() {
    local failed=0 found=0 context workflow

    while IFS= read -r context; do
        found=$((found + 1))
        workflow=$(grep -lxF "    name: ${context}" "$WORKFLOWS_DIR"/*.yml | head -n 1 || true)

        if [ -z "$workflow" ]; then
            echo "  FAIL '$context': no workflow job has this name" >&2
            failed=1
            continue
        fi
        if ! grep -qE '^  pull_request:' "$workflow"; then
            echo "  FAIL '$context': $(basename "$workflow") does not run on pull_request" >&2
            failed=1
            continue
        fi
        if pull_request_block "$workflow" | grep -qE '^ +(paths|paths-ignore):'; then
            # A path-filtered required check never reports on PRs outside those paths, which
            # leaves them blocked forever.
            echo "  FAIL '$context': $(basename "$workflow") filters pull_request by path" >&2
            failed=1
            continue
        fi
        echo "  ok   '$context' -> $(basename "$workflow")"
    done < <(sed -n 's/.*"context": "\([^"]*\)".*/\1/p' "$RULESET_FILE")

    if [ "$found" -eq 0 ]; then
        echo "  FAIL no required status checks found in $RULESET_FILE" >&2
        failed=1
    fi
    return "$failed"
}

echo "Checking required status checks in $(basename "$RULESET_FILE"):"
check_required_checks
[ "$MODE" = "check" ] && exit 0

if ! command -v gh > /dev/null; then
    echo "gh (GitHub CLI) is required: https://cli.github.com" >&2
    exit 1
fi

RULESET_NAME=$(sed -n 's/^  "name": "\([^"]*\)".*/\1/p' "$RULESET_FILE")
EXISTING_ID=$(gh api "repos/$REPO/rulesets" \
    --jq ".[] | select(.name == \"$RULESET_NAME\") | .id" | head -n 1)

if [ "$MODE" = "dry-run" ]; then
    if [ -n "$EXISTING_ID" ]; then
        echo "Would update ruleset '$RULESET_NAME' (id $EXISTING_ID) on $REPO."
    else
        echo "Would create ruleset '$RULESET_NAME' on $REPO."
    fi
    exit 0
fi

if [ -n "$EXISTING_ID" ]; then
    gh api --method PUT "repos/$REPO/rulesets/$EXISTING_ID" --input "$RULESET_FILE" > /dev/null
    echo "Updated ruleset '$RULESET_NAME' (id $EXISTING_ID) on $REPO."
else
    EXISTING_ID=$(gh api --method POST "repos/$REPO/rulesets" --input "$RULESET_FILE" --jq .id)
    echo "Created ruleset '$RULESET_NAME' (id $EXISTING_ID) on $REPO."
fi

echo "Rules now enforced on main:"
gh api "repos/$REPO/rules/branches/main" --jq '.[].type' | sort -u | sed 's/^/  - /'
