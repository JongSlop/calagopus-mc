#!/bin/bash
# Minecraft: Java (Fabric) installer. Runs inside the installer container; the server
# directory is /mnt/server. Kept dependency free: no jq, no apt.
fail() {
    echo -e "\n[ERROR] $1\n"
    if [[ -n "${INSTALL_STATUS_FILE}" ]]; then echo "$1" > "${INSTALL_STATUS_FILE}"; fi
    exit 1
}

mkdir -p /mnt/server
cd /mnt/server || fail "cannot enter /mnt/server"

# strip whitespace so the pretty-printed meta JSON can be read with grep
flat() { tr -d ' \n\r\t' | sed 's/},{/}\n{/g'; }

MC="${MINECRAFT_VERSION:-1.21.1}"
[[ "$MC" == latest ]] && MC=1.21.1

INSTALLER=$(curl -fsSL --retry 3 https://meta.fabricmc.net/v2/versions/installer \
    | flat | grep -oE '"version":"[^"]+"' | head -1 | cut -d'"' -f4)
[[ -n "$INSTALLER" ]] || fail "cannot reach the Fabric meta API"

LOADER="${FABRIC_LOADER}"
if [[ -z "$LOADER" ]]; then
    loaders=$(curl -fsSL --retry 3 "https://meta.fabricmc.net/v2/versions/loader/${MC}") || fail "cannot list Fabric loaders for ${MC}"
    # newest loader first; prefer the newest stable one
    while read -r entry; do
        [[ "$entry" == *'"version":"'* ]] || continue
        if [[ -z "$LOADER" || "$entry" == *'"stable":true'* ]]; then
            LOADER=$(sed -E 's/^.*"loader":\{"separator":"[^"]*","build":[0-9]+,"maven":"[^"]*","version":"([^"]+)".*$/\1/' <<< "$entry")
            [[ "$entry" == *'"stable":true'* ]] && break
        fi
    done < <(flat <<< "$loaders" | grep -oE '"loader":\{[^}]*\}')
fi
[[ -n "$LOADER" ]] || fail "no Fabric loader for Minecraft ${MC}"

echo "Installing Fabric ${LOADER} for Minecraft ${MC}..."
curl -fsSL --retry 3 -o "${SERVER_JARFILE}.download" \
    "https://meta.fabricmc.net/v2/versions/loader/${MC}/${LOADER}/${INSTALLER}/server/jar" \
    || fail "cannot download the Fabric server launcher"
mv -f "${SERVER_JARFILE}.download" "${SERVER_JARFILE}"

echo "eula=true" > eula.txt
cat > server.properties <<PROPS
server-port=25565
motd=A Calagopus dev server
online-mode=false
white-list=false
difficulty=normal
gamemode=survival
max-players=20
view-distance=10
pvp=true
level-name=world
PROPS

echo "${MC} (Fabric ${LOADER}) installed."
