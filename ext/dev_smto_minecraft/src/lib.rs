use shared::{
    State,
    extensions::{Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder},
    permissions::PermissionGroup,
};

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
        builder.add_client_server_api_router(|routes| {
            routes.nest("/dev.smto.minecraft", routes::server::router(&state))
        })
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
