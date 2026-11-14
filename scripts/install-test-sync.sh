#!/usr/bin/env bash
# Enable the repository's versioned hooks (see .githooks/).
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

chmod +x .githooks/post-commit .githooks/post-merge .githooks/post-rewrite
chmod +x scripts/sync-test-branch.sh
git config core.hooksPath .githooks

echo "Installed git hooks from .githooks/"
echo "Every commit, merge, amend, and rebase now mirrors HEAD to origin/test."
echo "main is never pushed or rewritten by the automation."
