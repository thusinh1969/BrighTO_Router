#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IMAGE="${IMAGE:-thusinh1969/brighto_airouter}"
TAG="${1:-${BRIGHTO_RELEASE_TAG:-v1.1.0}}"
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"
PUSH_LATEST="${PUSH_LATEST:-0}"
CHECK_ONLY="${2:-}"

if [[ ! "$TAG" =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.-]{0,127}$ ]] || [[ -n "$CHECK_ONLY" && "$CHECK_ONLY" != "--check" ]]; then
  echo "usage: $0 <tag> [--check]" >&2
  exit 2
fi

if ! docker buildx version >/dev/null 2>&1; then
  echo "docker buildx is required" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "Docker daemon is not reachable" >&2
  exit 1
fi

builder="${BRIGHTO_BUILDX_BUILDER:-default}"
docker buildx inspect "$builder" --bootstrap >/dev/null

tags=(-t "$IMAGE:$TAG")
if [[ "$PUSH_LATEST" == "1" ]]; then
  tags+=(-t "$IMAGE:latest")
fi

cat <<MSG

==> Building and testing BrighTO-Router before publication
Image:     $IMAGE
Tag:       $TAG
Platforms: $PLATFORMS
Latest:    $PUSH_LATEST

MSG

IFS=',' read -r -a platforms <<< "$PLATFORMS"
local_images=()
remote_images=()
for platform in "${platforms[@]}"; do
  case "$platform" in linux/amd64|linux/arm64) ;; *) echo "Unsupported platform: $platform" >&2; exit 2 ;; esac
  arch="${platform#linux/}"
  local_image="brighto-router-release:$TAG-$arch"
  docker buildx build --builder "$builder" --platform "$platform" \
    --provenance=false --load -t "$local_image" .
  local_images+=("$local_image")
  remote_images+=("$IMAGE:$TAG-$arch")
done

# Run the same HTTP/HTTPS API and real browser checks on both actual images.
# ARM tests require native ARM hardware or QEMU (Docker Desktop includes it).
for local_image in "${local_images[@]}"; do
  python3 smoke/python_sdk/run.py --image "$local_image"
done

if [[ "$CHECK_ONLY" == "--check" ]]; then
  printf '\nBuild and runtime checks passed; nothing published.\n'
  exit 0
fi

# Publish only images whose runtime checks passed, then expose the public tag.
for i in "${!local_images[@]}"; do
  docker tag "${local_images[$i]}" "${remote_images[$i]}"
  docker push "${remote_images[$i]}"
done
docker buildx imagetools create "${tags[@]}" "${remote_images[@]}"

printf '\n==> Published manifest for %s:%s\n' "$IMAGE" "$TAG"
docker buildx imagetools inspect "$IMAGE:$TAG"

if [[ "$PUSH_LATEST" == "1" ]]; then
  printf '\n==> Published manifest for %s:latest\n' "$IMAGE"
  docker buildx imagetools inspect "$IMAGE:latest"
fi
