# Calagopus Minecraft Tools

A Calagopus panel extension for Minecraft servers. It edits the server settings, manages the
whitelist, and installs and updates mods from Modrinth and CurseForge.

One extension, three pages on the server sidebar.

## What it does

**server.properties**
- Edit every key with a typed input. Booleans get a switch, numbers get a number field.
- Comments, key order and unknown keys survive a save.
- MOTD accepts `§` color codes. The file gets the correct `\u00a7` escapes.
- A filter box fits the 60+ keys on one screen. A raw tab edits the whole file as text.

**Whitelist**
- Add and remove players by name or UUID, with the skin head shown.
- Offline mode servers get offline UUIDs. Online mode servers get UUIDs from Mojang.
- A switch turns the whitelist on and off in `server.properties`.
- A running server gets `whitelist reload` over the console.

**Mods**
- Search Modrinth and CurseForge, filtered by the detected loader and Minecraft version.
- Install a mod into `/mods`, or into `/automodpack/host-modpack/main/mods` for client mods.
- Pick any compatible version, not only the newest one.
- Pin a mod to stop updates. Revert swaps back to the version an update replaced.
- Required dependencies install with the mod.
- Update one mod or all unpinned mods. Updates write to a temporary file and check the
  published hash before the rename, so the server never sees a half-written jar.
- Adopt mods that you installed before this extension. The node hashes each jar and the
  extension looks the hash up at both providers. Jars that nobody recognises stay on disk,
  and the result lists them.
- Auto update runs on a timer, and it waits until the server stops before it touches a jar.
  It can be switched off per server and for the whole panel.

## Requirements

- Calagopus panel 1.2.4 or newer.
- A Fabric, Forge or NeoForge server. The extension reads the loader and Minecraft version
  from the files on disk, and you can set both by hand.
- AutoModpack 4 for the client mod directory. Without it the extension skips the client
  target.
- A CurseForge API key for CurseForge search and installs. The key from the official
  CurseForge launcher works and is not personal. Modrinth needs no key.

Plain Forge uses the same code path as Fabric and NeoForge, but no test covers it yet.

## Install

1. Build or download `dev_smto_minecraft.c7s.zip`.
2. Copy it into the panel container:
   ```bash
   docker cp dev_smto_minecraft.c7s.zip calagopus-panel-web-1:/app/extensions/
   ```
3. Restart the panel container. The heavy image rebuilds the panel binary with the
   extension, then applies its migrations. Expect about two minutes of downtime.
4. Open **Admin**, then **Roles**, and grant `minecraft-properties`, `minecraft-whitelist`
   and `minecraft-mods` to the roles that should use the pages.
5. Open **Admin**, then **Minecraft Tools**, and paste the CurseForge key.

`panel-rs extensions update` alone does not install an update on the heavy image. It
refreshes the extracted source but not the stored zip, so a restart rebuilds the old
version. Replace the zip in `/app/extensions` first.

## Mod tracking

Each server keeps a manifest in `smto-modmanager.json` at the server root. One entry per
mod and per target, with provider, project id, version id, file name and hashes.

The manifest lives with the server data. A server backup or a move to another node keeps
it. Deleting it makes the extension forget its mods, but the jars stay and **Adopt existing
mods** rebuilds the list from the hashes.

## Development

The dev stack in `dev/` runs a panel, wings, Postgres, Valkey and a test server in Docker.
See `dev/README.md` for the commands and for the frontend build order.

Extension source is in `ext/dev_smto_minecraft`. The plan and open items are in `PLAN.md`.
