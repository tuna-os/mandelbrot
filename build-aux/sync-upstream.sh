#!/usr/bin/env bash
# Upstream synchronization script for Mandelbrot (downstream fork of GNOME Fractal).
#
# Fetches upstream commits/tags from https://gitlab.gnome.org/World/fractal.git,
# computes divergence from the pinned baseline (or merge-base), and prepares an
# upstream sync branch.

set -euo pipefail

UPSTREAM_URL="${UPSTREAM_URL:-https://gitlab.gnome.org/World/fractal.git}"
UPSTREAM_BRANCH="${1:-main}"
BASELINE_COMMIT="a262c7d656cc4c3a87656370e10cd279cf6e081a"

echo "=== Mandelbrot Upstream Sync ==="
echo "Upstream URL:    $UPSTREAM_URL"
echo "Upstream Branch: $UPSTREAM_BRANCH"
echo "Baseline Commit: $BASELINE_COMMIT"
echo "================================"

# Configure upstream remote
if git remote get-url upstream >/dev/null 2>&1; then
    git remote set-url upstream "$UPSTREAM_URL"
else
    git remote add upstream "$UPSTREAM_URL"
fi

echo "Fetching upstream refs..."
git fetch upstream --tags
git fetch upstream "$UPSTREAM_BRANCH"

LATEST_UPSTREAM_COMMIT=$(git rev-parse "upstream/$UPSTREAM_BRANCH")
MERGE_BASE=$(git merge-base HEAD "upstream/$UPSTREAM_BRANCH" || echo "$BASELINE_COMMIT")
NEW_COMMITS_COUNT=$(git rev-list --count "$MERGE_BASE..upstream/$UPSTREAM_BRANCH")

echo "Merge Base:              $MERGE_BASE"
echo "Latest Upstream Commit:  $LATEST_UPSTREAM_COMMIT"
echo "New upstream commits:    $NEW_COMMITS_COUNT"

if [ "$NEW_COMMITS_COUNT" -eq 0 ]; then
    echo "Up to date with upstream/$UPSTREAM_BRANCH. No new commits to sync."
    exit 0
fi

echo ""
echo "New upstream commits since merge base:"
git log -n 25 --format="* %h - %s (%an, %as)" "$MERGE_BASE..upstream/$UPSTREAM_BRANCH"
echo ""

SYNC_BRANCH="sync/upstream-$(date +%Y%m%d)"
echo "Creating sync branch: $SYNC_BRANCH"
git checkout -B "$SYNC_BRANCH"

echo "Attempting automated merge from upstream/$UPSTREAM_BRANCH..."
if git merge --no-commit --no-ff "upstream/$UPSTREAM_BRANCH"; then
    echo "Automated merge succeeded without conflicts."
    echo "Review staged changes, run tests, and commit with:"
    echo "  git commit -s -m 'chore(upstream): sync from upstream Fractal ($UPSTREAM_BRANCH)'"
else
    echo "Merge conflicts detected. Please resolve conflicts manually."
    echo "Use 'git status' and 'git diff' to review conflicts."
fi
