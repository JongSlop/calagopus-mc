# Calagopus Minecraft Extension: Plan

Status: dev environment up, M0 done, M1 in progress.

## Findings

- Panel on s1: `ghcr.io/calagopus/panel:heavy`, version 1.2.4. Extensions are compiled into the panel binary. Installing one rebuilds the binary in the container.
- Compose file: `/root/containers/calagopus-panel/compose.yml`. Mounted dirs: `build/extensions` (the `.c7s.zip` files), `build/extension-migrations`, `build/binaries`, `build/translations`.
- Installed now: `xyz.stellarstudios.mcmods` v1.2.1 (third-party mod browser with Modrinth and CurseForge). It has no pinning, no auto-update and no background task. License unknown.
- Leftover migration dirs: `com_subwaystudios_serverproperties` (one table, `ext_server_properties_settings`), plus empty dirs for `cal_stef_nbteditor`, `dev_emipy_mcplayers`.
- Extension layout: `Metadata.toml`, `backend/` (Rust, `Cargo.toml`, `src/lib.rs`), `frontend/` (React, `src/index.ts`), optional `migrations/`.
- Package name format: `<tld>.<author>.<name>`, for example `net.gtzk.minecraft`. The crate name uses underscores (`net_gtzk_minecraft`).
- Backend: axum routes. Server-scoped routes go through `add_client_server_api_router`. File access goes through `wings_api` (list, read, write, pull, fingerprints).
- Background tasks only run on the primary instance (`APP_PRIMARY=true`, already set).
- Do not read or reuse code from the installed `xyz.stellarstudios.mcmods` (licensing, see Decisions).

## Dev setup (on this Mac)

Running now. See `dev/README.md` for commands. Panel 1.2.6 heavy, wings, Postgres 17,
Valkey 8, one Fabric 1.21.1 test server on port 25565. The heavy image's supervisor
rebuilds its own cached binary and ignores manually applied extensions, so the dev
compose overrides the entrypoint and runs `target/debug/panel-rs` built from source.

`dev/compose.yml`: panel heavy image (1.2.6), Postgres 17, Valkey 8. Panel at http://127.0.0.1:8000.

The extension source lives on the host at `ext/dev_smto_minecraft` and is bind-mounted into the container three times:

- `ext/dev_smto_minecraft` → `/app/repo/backend-extensions/dev_smto_minecraft`
- `ext/.../migrations` → `/app/repo/database/extension-migrations/dev_smto_minecraft`
- `ext/.../frontend` → `/app/repo/frontend/extensions/dev_smto_minecraft`

Without mounts, edits vanish on container recreate. Keep `migrations` as a real directory, not a symlink: `panel-rs extensions export` refuses symlinks that escape the extension dir.

Commands (host):

```bash
cd dev
docker compose up -d
docker exec dev-web-1 sh -c 'cd /app/repo && panel-rs extensions export dev.smto.minecraft'
docker cp dev-web-1:/app/repo/exported-extensions/dev_smto_minecraft.c7s.zip .
docker exec dev-web-1 sh -c 'cd /app/repo && panel-rs extensions apply --profile dev'
```

## Decisions

1. One extension with three feature modules (`properties`, `whitelist`, `mods`). One rebuild per release, one `panel_version` to track.
2. Package name: `dev.smto.minecraft` (crate `dev_smto_minecraft`).
3. Clean-room mod manager. `xyz.stellarstudios.mcmods` is a paid, all-rights-reserved product by a third party. Do NOT copy, port or decompile its code or structure. Use only official docs: Calagopus extension docs, Modrinth API, CurseForge API, wings API. Its zip stays untouched on s1. Do not remove it for now. Route and sidebar prefixes must not clash with it (it uses `/minecraft/mods`).
4. Loaders: Fabric, NeoForge and plain Forge from the start (Forge should cost little extra: mostly a loader tag in queries and detection).
5. CurseForge API key: the user supplies their own, stored in extension settings. Never reuse the key from the old extension.
6. AutoModpack 4.0.5 runs on every server. Client-side mods are installable too, but only into `/automodpack/host-modpack/main/mods`. Server-side mods go to `/mods` as usual. Details:
   - Install target is chosen per mod: mods the server loads go to `/mods`, mods only the client needs go to the AutoModpack host dir.
   - Corrected: `/mods` is **not** server-only. AutoModpack syncs it to clients on join. Only the host-modpack dir is client-only, and a mod that must never reach players needs an exclusion in the AutoModpack config (rare).
   - Do not filter out Modrinth `client_side: required` + `server_side: unsupported` mods any more. Route them to the AutoModpack dir instead.
   - The lockfile records the target dir per file, so pinning and auto-update work for both dirs.
   - Check how AutoModpack 4.0.5 treats `host-modpack/main` (it may regenerate its modpack hash on change, so avoid half-written files: download to a temp name, verify, then rename).
7. Per-feature on/off switch and egg allowlist in the extension settings.
7. Do not test on prod. Dev setup is a Docker container on the user's Mac (arm64, Docker 29.8, Compose v5.5).

## Milestones

### M0: Pipeline spike (done)
- Dev env in Docker on the Mac: panel (built from `calagopus/panel` source or the heavy image), Postgres 16+, Redis 7+, plus wings and a test Minecraft server.
- `panel-rs extensions init dev.smto.minecraft`.
- Hello world: one migration, one server route, one sidebar page.
- `panel-rs extensions export` to a `.c7s.zip`, then install it on a staging panel.
- Exit: the zip installs on a clean 1.2.4 heavy container.

Done in dev: extension listed, applied, route reachable at
`/api/client/servers/{server}/dev.smto.minecraft/properties`, reads and writes the real
`server.properties` over wings. Still open: install the exported zip on a clean panel.

### M1: server.properties editor
- Backend: done. Read and write over wings, parser keeps comments, key order, unknown keys and Java escapes. Permission group `minecraft-properties` (`read`, `update`). Activity log on save.
- Frontend: table of key/value rows, add and remove, save sends only the diff.
- Open: typed inputs (bool switch, number), raw-text fallback, restart note.

### M2: Whitelist editor
- Read and write `whitelist.json`. Show player heads, add by name, remove, bulk import.
- Name to UUID through the Mojang API, with caching and rate limit handling. Offline-mode servers use a different UUID scheme (detect via `online-mode`).
- Server running: use console commands (`whitelist add|remove|reload`). Server stopped: write the file.

### M3: Mod manager
- M3a: Browse and install. Modrinth + CurseForge. Filter by MC version and loader. Hide client-only mods (Modrinth `server_side: unsupported`). Resolve required dependencies.
- M3b: State. Per-server DB table (lockfile): file, provider, project id, version id, hash, pinned flag. Detect unmanaged jars through hash and fingerprint lookup. Never touch unmanaged jars.
- M3c: Pinning. Pin a version, block updates for it.
- M3d: Auto-update. Cron background task on the primary instance. Per-server opt-in. Skip pinned mods. Stay within the same MC version and loader. Keep the old jar for rollback. Verify the hash after download. Apply only while the server is stopped, or queue until the next stop.
- Loader and MC version detection from server files, not only egg variables. 1.20.1 vs 1.21.1 NeoForge and Forge tagging differs on Modrinth.
- CurseForge: API key in extension settings. Handle `allowModDistribution=false` (no download URL).

## Risks

| Risk | Plan |
|------|------|
| Prod downtime on install (image rebuild) | Staging first. Backup `compose.yml` and `postgres/` before the first install. |
| Panel upgrade breaks `panel_version` match | Pin the panel image tag. Rebuild and re-test on upgrade. |
| Server overwrites file edits while running | Console commands for whitelist. Warn for properties. |
| Two mod managers clash (routes, sidebar) | Unique route prefix. Remove the old one after M3. |
| CurseForge rate limits and blocked downloads | Cache responses. Show a clear "manual download needed" state. |
| Wrong loader or version pulls a crashing mod | Strict loader/version filters. Test on 1.20.1 and 1.21.1 for Fabric and NeoForge. |

## Status

- M0 done: dev stack in `dev/`, extension installs into a panel built from source,
  `panel-rs extensions export dev.smto.minecraft` produces the install zip.
- M1 done: `server.properties` editor with typed inputs (switch, numbers, text) and
  `` § `` colour codes written as `\u00a7` escapes.
- M2 done: whitelist editor, Mojang lookup on online-mode servers, offline UUIDs
  otherwise, `whitelist reload` sent to a running server.
- Adoption done: jars already on disk are hashed by wings (sha512 for Modrinth, CurseForge
  fingerprint for CF, sha1 to name the CF file) and recorded in the manifest. Unrecognised
  jars stay unmanaged and are listed for the user. Runs from the button, never automatically.
- Detection fixed: `.fabric` directory and `versions/<mc>` are used as hints, because the
  start jar is often renamed to `server.jar`. Install now refuses to run without a loader
  and Minecraft version instead of picking whatever file is newest.
- M3 done: Modrinth and CurseForge search, install to `/mods` or the AutoModpack
  host modpack directory with sha512 verification and atomic rename, required
  dependencies installed one level deep, pinning, single and bulk updates, and a
  six-hour cron task that only touches jars while a server is offline.
- Open: verify on a real host (the macOS dev box cannot keep the game container
  running), CurseForge key from the user, Forge (non-Neo) end-to-end check.

## Open questions

- Own CurseForge API key: user will provide it.
- Mod needed on both sides (client and server required): install into both dirs?
