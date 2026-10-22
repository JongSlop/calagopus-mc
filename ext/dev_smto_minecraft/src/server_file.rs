//! Reading and writing files of a server through wings.

use shared::models::server::GetServer;
use tokio::io::AsyncReadExt;

pub async fn read(state: &shared::State, server: &GetServer, file: &str) -> anyhow::Result<String> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let mut reader = api
        .get_servers_server_files_contents(
            server.uuid,
            &wings_api::servers_server_files_contents::get::Query {
                file: Some(file.into()),
                ..Default::default()
            },
        )
        .await?;

    let mut contents = String::new();
    reader.read_to_string(&mut contents).await?;

    Ok(contents)
}

/// Returns `None` when the file does not exist.
pub async fn read_optional(
    state: &shared::State,
    server: &GetServer,
    file: &str,
) -> anyhow::Result<Option<String>> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let mut reader = match api
        .get_servers_server_files_contents(
            server.uuid,
            &wings_api::servers_server_files_contents::get::Query {
                file: Some(file.into()),
                ..Default::default()
            },
        )
        .await
    {
        Ok(reader) => reader,
        Err(wings_api::client::ApiHttpError::Http(axum::http::StatusCode::NOT_FOUND, _)) => {
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    let mut contents = String::new();
    reader.read_to_string(&mut contents).await?;

    Ok(Some(contents))
}

pub async fn write(state: &shared::State, server: &GetServer, file: &str, contents: String) -> anyhow::Result<()> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    api.post_servers_server_files_write(
        server.uuid,
        wings_api::client::AsyncRequestReader::new(std::io::Cursor::new(contents.into_bytes())),
        &wings_api::servers_server_files_write::post::Query {
            file: Some(file.into()),
            ..Default::default()
        },
    )
    .await?;

    Ok(())
}

/// Sends console commands, ignoring failures when the server is not running.
pub async fn send_commands(
    state: &shared::State,
    server: &GetServer,
    commands: Vec<String>,
) -> anyhow::Result<()> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    api.post_servers_server_commands(
        server.uuid,
        &wings_api::servers_server_commands::post::RequestBody {
            commands: commands.into_iter().map(Into::into).collect(),
        },
    )
    .await?;

    Ok(())
}

async fn is_running(state: &shared::State, server: &GetServer) -> anyhow::Result<bool> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let info = api.get_servers_server(server.uuid).await?;
    Ok(matches!(info.state, wings_api::ServerState::Running))
}

/// Reloads the whitelist on a running server. A stopped server reads the file on
/// the next start, so nothing has to happen there.
pub async fn reload_whitelist(state: &shared::State, server: &GetServer) -> anyhow::Result<()> {
    if is_running(state, server).await.unwrap_or(false) {
        send_commands(state, server, vec!["whitelist reload".to_string()]).await?;
    }

    Ok(())
}
