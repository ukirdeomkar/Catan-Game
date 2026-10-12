#!/usr/bin/env bash
#
# qa-staged.sh - build the current branch in Docker and serve it on the next
# free host port, then print the URL to hand to QA.
#
# Why Docker: the build host has no C compiler (`linker 'cc' not found`), so a
# host `cargo build` / `cargo run` fails. Everything here runs inside Docker;
# no Rust toolchain is required on the host.
#
# Port convention: staged previews start at 3708 and count up. Precedent:
#   catan-pr23 -> :3706, catan-pr24 -> :3707, catan-pr31 -> :3708.
# The script uses the first port Docker can publish, so two staged branches can
# run side by side.
#
# Usage:
#   scripts/qa-staged.sh                 # build current branch, stage it, print URL
#   scripts/qa-staged.sh --no-build      # reuse the image, skip docker build
#   scripts/qa-staged.sh --port 3800     # start searching from 3800
#   scripts/qa-staged.sh --name my-qa    # override the container name
#
# Env overrides: QA_BASE_PORT, QA_INTERNAL_PORT, QA_TIMEOUT, QA_IMAGE, QA_BUILD_ARGS
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_PORT="${QA_BASE_PORT:-3708}"
INTERNAL_PORT="${QA_INTERNAL_PORT:-8080}"
TIMEOUT="${QA_TIMEOUT:-90}"
BUILD_ARGS="${QA_BUILD_ARGS:-}"
IMAGE_OVERRIDE="${QA_IMAGE:-}"
CONTAINER_OVERRIDE=""
NO_BUILD=0

usage() {
  sed -n '2,24p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)     BASE_PORT="$2"; shift 2 ;;
    --name)     CONTAINER_OVERRIDE="$2"; shift 2 ;;
    --image)    IMAGE_OVERRIDE="$2"; shift 2 ;;
    --timeout)  TIMEOUT="$2"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "qa-staged: unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

command -v docker >/dev/null 2>&1 || { echo "qa-staged: docker is not installed" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "qa-staged: cannot reach the Docker daemon (DOCKER_HOST=${DOCKER_HOST:-unset})" >&2; exit 1; }

BRANCH="$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo detached)"
SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
SLUG="$(printf '%s' "$BRANCH" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9' '-' | sed 's/-\+/-/g; s/^-//; s/-$//')"
[[ -n "$SLUG" ]] || SLUG="staged"

IMAGE="${IMAGE_OVERRIDE:-catan-staged:$SLUG}"
CONTAINER="${CONTAINER_OVERRIDE:-catan-staged-$SLUG}"

if [[ "$NO_BUILD" -eq 0 ]]; then
  echo "qa-staged: building $IMAGE from $REPO_ROOT ($BRANCH @ $SHA)"
  # shellcheck disable=SC2086
  docker build -t "$IMAGE" $BUILD_ARGS "$REPO_ROOT"
else
  echo "qa-staged: --no-build, reusing $IMAGE"
fi

# Replace any previous container with this name.
docker rm -f "$CONTAINER" >/dev/null 2>&1 || true

# Find the first host port Docker will let us publish.
PORT="$BASE_PORT"
LIMIT=$((BASE_PORT + 50))
RUN_LOG="$(mktemp)"
trap 'rm -f "$RUN_LOG"' EXIT
while :; do
  if docker run -d --name "$CONTAINER" -p "$PORT:$INTERNAL_PORT" "$IMAGE" >/dev/null 2>"$RUN_LOG"; then
    break
  fi
  if grep -qiE 'port is already allocated|address already in use|failed to bind|bind: ' "$RUN_LOG"; then
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
    if [[ "$PORT" -ge "$LIMIT" ]]; then
      echo "qa-staged: no free port in $BASE_PORT..$LIMIT" >&2
      exit 1
    fi
    PORT=$((PORT + 1))
    continue
  fi
  echo "qa-staged: docker run failed:" >&2
  cat "$RUN_LOG" >&2
  exit 1
done

URL="http://127.0.0.1:$PORT/"

# Wait until the app answers on its internal port.
echo "qa-staged: waiting for $URL (up to ${TIMEOUT}s)"
deadline=$(( $(date +%s) + TIMEOUT ))
until docker exec "$CONTAINER" wget -qO- "http://127.0.0.1:$INTERNAL_PORT/" >/dev/null 2>&1; do
  if ! docker ps --format '{{.Names}}' | grep -qx "$CONTAINER"; then
    echo "qa-staged: container exited during startup; logs:" >&2
    docker logs --tail 40 "$CONTAINER" >&2 || true
    exit 1
  fi
  if [[ "$(date +%s)" -ge "$deadline" ]]; then
    echo "qa-staged: timed out after ${TIMEOUT}s; logs:" >&2
    docker logs --tail 40 "$CONTAINER" >&2 || true
    exit 1
  fi
  sleep 1
done

echo
echo "Staged and serving:"
echo "  URL:       $URL"
echo "  Image:     $IMAGE"
echo "  Container: $CONTAINER"
echo "  Branch:    $BRANCH @ $SHA"
echo "  Stop:      docker rm -f $CONTAINER"
