#!/usr/bin/env bash
# Mirror the current HEAD to origin/test. This never pushes or rewrites main.
#
# Invoked automatically by the hooks in .githooks/ after commits, merges, and
# history rewrites. Set DSH2_SKIP_TEST_SYNC=1 to skip a single run.
set -u

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
cd "$repo_root" || exit 0

if [ "${DSH2_SKIP_TEST_SYNC:-0}" = "1" ]; then
    exit 0
fi

if ! git remote get-url origin >/dev/null 2>&1; then
    exit 0
fi

if ! git rev-parse --verify HEAD >/dev/null 2>&1; then
    exit 0
fi

commit="$(git rev-parse --short HEAD)"
output="$(git push --quiet --force-with-lease=refs/heads/test \
    origin HEAD:refs/heads/test 2>&1)"
status=$?

# Never block a commit because the network or remote is unavailable.
if [ "$status" -ne 0 ]; then
    printf 'warning: could not sync %s to origin/test\n%s\n' "$commit" "$output" >&2
fi

exit 0
