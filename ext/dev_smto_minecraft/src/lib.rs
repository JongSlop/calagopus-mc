use shared::{
    State,
    extensions::{
        Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder,
        background_tasks::BackgroundTaskBuilder,
    },
    permissions::PermissionGroup,
};

pub mod mods;
pub mod settings;
pub mod properties;
pub mod server_file;
pub mod whitelist;

mod routes;

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder
            .add_client_server_api_router(|routes| {
                routes.nest("/dev.smto.minecraft", routes::server::router(&state))
            })
            .add_admin_api_router(|routes| {
                routes.nest("/dev.smto.minecraft", routes::admin::router(&state))
            })
    }

    async fn initialize_background_tasks(
        &mut self,
        _state: State,
        builder: BackgroundTaskBuilder,
    ) -> BackgroundTaskBuilder {
        // Wakes up hourly; each server's manifest decides whether it is due yet. Only
        // unpinned mods, only while the server is offline.
        let cron = std::str::FromStr::from_str("0 0 * * * *").expect("valid cron expression");

        builder
            .add_cron_task(
                "dev_smto_minecraft_mod_auto_update",
                cron,
                |state| async move { mods::manager::auto_update_pass(&state).await },
            )
            .await;

        builder
    }

    async fn initialize_permissions(
        &mut self,
        _state: State,
        builder: ExtensionPermissionsBuilder,
    ) -> ExtensionPermissionsBuilder {
        builder.add_server_permission_group(
            "minecraft-properties",
            PermissionGroup {
                description: "Permissions for editing the server.properties file.",
                permissions: indexmap::IndexMap::from([
                    (
                        "read",
                        "Allows viewing the server.properties file of this server.",
                    ),
                    (
                        "update",
                        "Allows changing the server.properties file of this server.",
                    ),
                ]),
            },
        )
        .add_server_permission_group(
            "minecraft-mods",
            PermissionGroup {
                description: "Permissions for browsing and managing server mods.",
                permissions: indexmap::IndexMap::from([
                    ("read", "Allows browsing installed and available mods."),
                    ("update", "Allows installing, removing, pinning and updating mods."),
                ]),
            },
        )
        .add_server_permission_group(
            "minecraft-whitelist",
            PermissionGroup {
                description: "Permissions for editing the whitelist.",
                permissions: indexmap::IndexMap::from([
                    ("read", "Allows viewing the whitelist of this server."),
                    ("update", "Allows adding and removing whitelisted players."),
                ]),
            },
        )
    }
}
