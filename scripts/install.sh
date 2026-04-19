#!/bin/sh
set -e
# Legacy installer location — delegates to root install.sh
# See: https://github.com/IbrohimMid/VAC
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$SCRIPT_DIR/../install.sh" "$@"
