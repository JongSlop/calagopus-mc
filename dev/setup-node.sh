#!/usr/bin/env bash
# Dev-panel setup: location, node, wings pairing.
# Run from dev/:  ./setup-node.sh <pairing-code>
set -euo pipefail

CODE="${1:?usage: setup-node.sh <pairing-code>}"
PANEL=http://127.0.0.1:8000
JAR="$(mktemp)"

curl -sf -c "$JAR" -H 'content-type: application/json' \
  -d '{"user":"admin","password":"devpassword123"}' \
  "$PANEL/api/auth/login" > /dev/null

api() {
  curl -sf -b "$JAR" -H 'content-type: application/json' "$@"
}

LOC=$(api -d '{"name":"Local","description":"dev"}' "$PANEL/api/admin/locations" | jq -r .location.uuid)
echo "location: $LOC"

docker exec dev-web-1 panel-rs nodes create \
  --location-uuid "$LOC" --name local --url http://wings:8080 \
  --public-url http://localhost:8080 --sftp-port 2022 \
  --memory 8589934592 --disk 107374182400 \
  --deployment-enabled false --maintenance-enabled false --json > "$JAR.node.json"

NODE=$(jq -r .uuid "$JAR.node.json")
echo "node: $NODE"

api -d "{\"pairing_code\":\"$CODE\"}" "$PANEL/api/admin/nodes/$NODE/pair"
echo "paired"
