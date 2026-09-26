#!/usr/bin/env bash
# Tags HEAD as a release: moonkale-YYMMDD-prototype-<short hash>, e.g.
# moonkale-260926-prototype-342cd5d. Pushing the tag runs
# .github/workflows/release.yml, which builds every package and publishes a
# GitHub release under the same name (markdown/packaging/Packaging Overview.md).
#
#   packaging/tag-release.sh            # create the tag locally, print it
#   packaging/tag-release.sh --push     # …and push it to the GitHub repository
#
# The date is today's (UTC), the hash is HEAD's: the name says when it was
# cut and which commit it is. Breaking changes are expected, so there is no
# semantic version for now.
set -euo pipefail
cd "$(dirname "$0")/.."

PUSH=0
for a in "$@"; do
  case "$a" in
    --push) PUSH=1 ;;
    *) echo "unknown flag $a" >&2; exit 2 ;;
  esac
done

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "uncommitted changes: commit first, a tag names a commit" >&2
  exit 1
fi

TAG="moonkale-$(date -u +%y%m%d)-prototype-$(git rev-parse --short=7 HEAD)"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  echo "$TAG exists already"
else
  git tag -a "$TAG" -m "Moonkale $TAG"
  echo "tagged $TAG"
fi

if [ "$PUSH" = 1 ]; then
  # The URL rather than a remote name: the push must reach GitHub whichever
  # remote (ssh or https) this clone uses.
  git push "${MOONKALE_PUSH_URL:-https://github.com/MathStruct/Moonkale.git}" "refs/tags/$TAG"
fi
