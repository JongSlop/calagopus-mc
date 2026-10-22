# Dev stack

Local Calagopus panel + wings with the dev extension compiled from `../ext`.

## Start

```bash
docker compose up -d
# first start builds the panel from source (about 10 min); later starts are instant
docker compose logs -f web
```

Panel: http://127.0.0.1:8000 (admin / devpassword123)
Wings: http://127.0.0.1:8080, SFTP on 2022, server port 25565

## Build the extension

```bash
docker compose exec web sh -c 'cd /app/repo && cargo build --profile dev --bin panel-rs'
docker compose restart web
```

`panel-rs extensions resync` runs automatically on start and picks up the mounted
sources under `../ext/dev_smto_minecraft`.

Unit tests for the extension crate:

```bash
docker compose exec web sh -c 'cd /app/repo && cargo test -p dev_smto_minecraft'
```

## Test server

`./setup-server.sh [name]` creates a Fabric server on the local node, port 25565.
Server files land in `wings/data/volumes/<server uuid>/`.

The egg lives in `eggs/minecraft-fabric.yml`. To push changes to it after editing:

```bash
docker compose exec web sh -c 'panel-rs nests mass-import /tmp/eggs --nest Minecraft'
```

## Why images differ from production

`ghcr.io/caloptreyx/*` images are amd64 only and cannot run on an arm64 Mac, so the
egg uses `ghcr.io/pterodactyl/yolks:java_21` and `ghcr.io/pterodactyl/installers:debian`.
The installers image has no jq and runs as non-root, so the install script avoids both.

## Reset

```bash
docker compose down -v && rm -rf postgres wings/data build/binaries
```
