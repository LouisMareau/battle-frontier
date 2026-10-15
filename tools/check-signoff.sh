#!/usr/bin/env bash
# Every commit in a range must be signed off (Developer Certificate of Origin, see
# CONTRIBUTING.md): a "Signed-off-by: Name <email>" line, which `git commit -s` adds.
#     bash tools/check-signoff.sh BASE HEAD
set -euo pipefail
base="$1"
head="$2"
missing=0
for c in $(git rev-list --no-merges "$base..$head"); do
  if ! git log -1 --format=%B "$c" | grep -q -E '^Signed-off-by: .+ <.+@.+>'; then
    echo "not signed off: $(git log -1 --format='%h %s' "$c")" >&2
    missing=1
  fi
done
if [ "$missing" = 1 ]; then
  echo "Sign off every commit (git commit -s; git rebase --signoff $base to fix existing ones)." >&2
  exit 1
fi
echo "every commit in $base..$head is signed off"
