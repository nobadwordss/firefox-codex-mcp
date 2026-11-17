#!/bin/sh
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "$#" -gt 0 ]; then
    exec "$package_dir/firefox-codex-mcp" "$@"
fi
if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    exec "$package_dir/firefox-codex-mcp" --manage
fi
printf '%s\n' 'No graphical session detected. Install directly: ./install.sh --install' 'Print the setup URL without opening a browser: ./install.sh --manage-no-open'
