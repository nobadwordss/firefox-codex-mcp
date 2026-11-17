set -eu
directory=/usr/libexec/firefox-codex-mcp
umask 022
generation=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
test "${#generation}" -eq 64
digest=$(sha256sum "$directory/firefox-codex-mcp")
digest=${digest%% *}
test "${#digest}" -eq 64
test "$digest" = '@EXECUTABLE_SHA256@'
temporary=$(mktemp "$directory/.installation.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
cat > "$temporary" <<EOF
{"schema":1,"version":"@VERSION@","generation":"$generation","executable_sha256":"$digest"}
EOF
chmod 0644 "$temporary"
mv -f "$temporary" "$directory/installation.json"
trap - EXIT HUP INT TERM
