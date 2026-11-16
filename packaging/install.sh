#!/bin/sh
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "$#" -gt 0 ]; then
    exec "$package_dir/firefox-codex-mcp" "$@"
fi
if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    exec "$package_dir/firefox-codex-mcp" --manage
fi
printf '%s\n' '그래픽 환경이 없습니다. 직접 설치: ./install.sh --install [프로필 경로]' '관리 주소만 열기: ./install.sh --manage-no-open'
