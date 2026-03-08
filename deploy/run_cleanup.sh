#!/usr/bin/env bash
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="${SCRIPT_DIR}"

cd "${APP_ROOT}"
set -a
. "${APP_ROOT}/.env"
set +a

exec "${APP_ROOT}/image_resize" cleanup
