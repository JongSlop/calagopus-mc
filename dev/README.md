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

The frontend is embedded into the `panel-rs` binary at compile time, so a change to
`frontend/` needs both steps, in this order, or the panel keeps serving the old UI:

```bash
docker compose exec web sh -c 'cd /app/repo/frontend && pnpm build'
docker compose exec web sh -c 'cd /app/repo && cargo build --profile dev --bin panel-rs'
docker compose restart web
```

Restarting alone never picks up frontend changes, and a browser reload cannot either.

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

### Startup uses -Xmx, not MaxRAMPercentage

Wings runs in a container here and reports a nonsense memory limit to the game
container, so `-XX:MaxRAMPercentage=95.0` makes the JVM ask for a petabyte heap
and it exits at once with `Could not reserve enough space`. The dev egg therefore
starts the server with `-Xmx2G`. Production hosts report the real limit and can
keep using the percentage.

## Why images differ from production

`ghcr.io/caloptreyx/*` images are amd64 only and cannot run on an arm64 Mac, so the
egg uses `ghcr.io/pterodactyl/yolks:java_21` and `ghcr.io/pterodactyl/installers:debian`.
The installers image has no jq and runs as non-root, so the install script avoids both.

## Reset

```bash
docker compose down -v && rm -rf postgres wings/data build/binaries
```

## Install on the production instance (s1)

The production panel is the stock `ghcr.io/calagopus/panel:heavy` image, so the
extension is installed as an archive, not from a source mount:

1. `docker cp dev_smto_minecraft.c7s.zip calagopus-panel-web-1:/app/extensions/`
   (that directory is the mounted `build/extensions`, so the zip lands on the host).
2. `docker restart calagopus-panel-web-1`.

The heavy supervisor unpacks every zip in `/app/extensions`, resyncs the extension
list, rebuilds the panel binary with all of them and starts it. The rebuild takes
about two minutes; the panel is down for that long. Migrations are applied on
startup and land in `build/extension-migrations/<crate>` on the host.

`panel-rs extensions add` alone is not enough on this setup: it unpacks the source
into the container but does not store the zip in the extension directory, so the
next restart drops the extension again.

Check the result with `docker exec calagopus-panel-web-1 panel-rs extensions list`
and look for `status: applied`.
