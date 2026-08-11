#!/usr/bin/env bash
# pre-commit helper: fail if any given file still contains git merge-conflict
# markers (<<<<<<< / >>>>>>> ). receives the staged filenames from pre-commit.
set -u

status=0
for f in "$@"; do
    if grep -nE '^(<<<<<<< |>>>>>>> )' "$f"; then
        echo "merge-conflict markers in: $f" >&2
        status=1
    fi
done
exit "$status"
