#!/usr/bin/env bash
# Tags HEAD as today's release: YYMMDD-proto (UTC), e.g. 260927-proto.
# Pushing the tag runs .github/workflows/release.yml, which builds every
# package and publishes a GitHub release of that name with files named
# moonkale-260927-proto-<platform>.<ext> (markdown/packaging/Packaging Overview.md).
#
#   packaging/tag-release.sh                  # create the tag locally, print it
#   packaging/tag-release.sh --push           # …and push it
#   packaging/tag-release.sh --replace --push # rebuild today's release from HEAD:
#                                             # delete its GitHub release and tag first
#
# One release per day; a rebuild the same day reuses the name on purpose.
# GitHub scopes the Actions caches by tag name, so the rebuild restores what
# the first run cached, which a new name each time never could. Breaking
# changes are expected, so there is no semantic version for now.
set -euo pipefail
cd "$(dirname "$0")/.."

PUSH=0; REPLACE=0
for a in "$@"; do
  case "$a" in
    --push) PUSH=1 ;;
    --replace) REPLACE=1 ;;
    *) echo "unknown flag $a" >&2; exit 2 ;;
  esac
done

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "uncommitted changes: commit first, a tag names a commit" >&2
  exit 1
fi

# The URL rather than a remote name: the push must reach GitHub whichever
# remote (ssh or https) this clone uses.
URL="${MOONKALE_PUSH_URL:-https://github.com/MathStruct/Moonkale.git}"
REPO="${MOONKALE_REPO:-MathStruct/Moonkale}"
TAG="$(date -u +%y%m%d)-proto"

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || git ls-remote --exit-code --tags "$URL" "refs/tags/$TAG" >/dev/null 2>&1; then
  if [ "$REPLACE" != 1 ]; then
    echo "$TAG exists already; --replace rebuilds today's release from HEAD" >&2
    exit 1
  fi
  # The release first (its files go with it), then the tag on GitHub and here.
  gh release delete "$TAG" -R "$REPO" --yes 2>/dev/null && echo "deleted the GitHub release $TAG" || true
  git push -q "$URL" ":refs/tags/$TAG" 2>/dev/null && echo "deleted the tag $TAG on GitHub" || true
  git tag -d "$TAG" >/dev/null 2>&1 || true
fi

git tag -a "$TAG" -m "Moonkale $TAG"
echo "tagged $TAG at $(git rev-parse --short HEAD)"

if [ "$PUSH" = 1 ]; then
  git push "$URL" "refs/tags/$TAG"
fi
