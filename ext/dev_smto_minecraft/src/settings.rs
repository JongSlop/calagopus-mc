//! Extension settings that are not per server. Stored in the panel database, with
//! the CurseForge key encrypted at rest.

use shared::State;

pub async fn curseforge_key(state: &State) -> anyhow::Result<Option<String>> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT curseforge_api_key FROM dev_smto_minecraft_settings WHERE id = 1",
    )
    .fetch_one(state.database.write())
    .await?;

    match stored {
        Some(value) if !value.is_empty() => Ok(Some(
            state.database.decrypt_base64(&value).await?.to_string(),
        )),
        _ => Ok(None),
    }
}

pub async fn set_curseforge_key(state: &State, key: Option<&str>) -> anyhow::Result<()> {
    let encrypted = match key.filter(|key| !key.trim().is_empty()) {
        Some(key) => Some(
            state
                .database
                .encrypt_base64(compact_str::CompactString::from(key.trim()))
                .await?,
        ),
        None => None,
    };

    sqlx::query(
        "INSERT INTO dev_smto_minecraft_settings (id, curseforge_api_key) VALUES (1, $1)
         ON CONFLICT (id) DO UPDATE SET curseforge_api_key = EXCLUDED.curseforge_api_key",
    )
    .bind(encrypted)
    .execute(state.database.write())
    .await?;

    Ok(())
}

/// Panel-wide kill switch. Per server toggles still exist, this one stops the
/// background task before it even looks at a server.
pub async fn auto_updates_enabled(state: &State) -> anyhow::Result<bool> {
    let enabled: bool = sqlx::query_scalar(
        "SELECT auto_updates_enabled FROM dev_smto_minecraft_settings WHERE id = 1",
    )
    .fetch_one(state.database.write())
    .await?;

    Ok(enabled)
}

pub async fn set_auto_updates_enabled(state: &State, enabled: bool) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO dev_smto_minecraft_settings (id, auto_updates_enabled) VALUES (1, $1)
         ON CONFLICT (id) DO UPDATE SET auto_updates_enabled = EXCLUDED.auto_updates_enabled",
    )
    .bind(enabled)
    .execute(state.database.write())
    .await?;

    Ok(())
}
