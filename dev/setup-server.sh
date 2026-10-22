#!/usr/bin/env bash
# Creates a dev Minecraft server on the local node.
# Run from dev/:  ./setup-server.sh [name]
set -euo pipefail

NAME="${1:-fabric-test}"
PANEL=http://127.0.0.1:8000
JAR="$(mktemp)"

curl -sf -c "$JAR" -H 'content-type: application/json' \
  -d '{"user":"admin","password":"devpassword123"}' \
  "$PANEL/api/auth/login" > /dev/null

api() { curl -s -b "$JAR" -H 'content-type: application/json' "$@"; }

NODE=$(docker exec dev-db-1 psql -U panel -d panel -tAc "select uuid from nodes where name='local'")
OWNER=$(docker exec dev-db-1 psql -U panel -d panel -tAc "select uuid from users where username='admin'")

# allocation on the panel's own view of the node: the docker network address is fine here,
# only the port matters for the container binding
ALLOC=$(api "$PANEL/api/admin/nodes/$NODE/allocations?per_page=100" | jq -r '.allocations.data[0].uuid // empty')
if [ -z "$ALLOC" ]; then
  api -d '{"ip":"0.0.0.0","ports":[25565]}' "$PANEL/api/admin/nodes/$NODE/allocations" > /dev/null
  ALLOC=$(api "$PANEL/api/admin/nodes/$NODE/allocations?per_page=100" | jq -r '.allocations.data[0].uuid')
fi

API=$(api "$PANEL/api/admin/nests?per_page=100" \
  | jq -r '.nests.data[] | select(.name=="Minecraft") | .uuid')
EGG=$(docker exec dev-db-1 psql -U panel -d panel -tAc "select uuid from nest_eggs where name like 'Minecraft: Java (Fabric)'")

VARS=$(api "$PANEL/api/admin/nests/$API/eggs/$EGG/variables" \
  | jq -c '[.variables[] | {env_variable: .env_variable, value: (.default_value // "")}]')

api -d "$(jq -n \
  --arg node "$NODE" --arg owner "$OWNER" --arg egg "$EGG" --arg alloc "$ALLOC" \
  --arg name "$NAME" --argjson vars "$VARS" \
  '{
    node_uuid: $node, owner_uuid: $owner, egg_uuid: $egg,
    allocation_uuid: $alloc, allocation_uuids: [],
    start_on_completion: false, skip_installer: false,
    name: $name, description: "dev test server",
    limits: {cpu: 100, memory: 2147483648, memory_overhead: 0, swap: 0, disk: 8589934592},
    pinned_cpus: [],
    startup: "java -Xms128M -XX:MaxRAMPercentage=95.0 -Duser.home=/home/container -jar {{SERVER_JARFILE}} nogui",
    image: "ghcr.io/caloptreyx/java:21",
    labels: {},
    hugepages_passthrough_enabled: false,
    kvm_passthrough_enabled: false,
    feature_limits: {allocations: 0, databases: 0, backups: 0, schedules: 0},
    variables: $vars
  }')" "$PANEL/api/admin/servers"
