//! Settings (schema and values) and design themes. Shared by all vaults:
//! `/api/settings`; for one vault: `/api/vaults/{vault}/settings`, the shared
//! ones with the vault's own on top (`PUT` with `null` makes one shared again).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use notes_core::settings::SettingsProblem;
use notes_core::themes::Theme;
use serde_json::{Map, Value};

use crate::AppState;
use crate::api::{SettingsResponse, VaultSettingsResponse};
use crate::assets::css;
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/vaults/{vault}/settings", get(get_vault_settings).put(put_vault_settings))
        .route("/api/themes", get(themes))
        .route("/api/themes.css", get(themes_css))
}

async fn get_settings(State(s): State<AppState>) -> Json<SettingsResponse> {
    Json(SettingsResponse { schema: s.settings.schema().clone(), values: s.settings.values() })
}

async fn put_settings(State(s): State<AppState>, Json(patch): Json<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let (settings, vaults) = (s.settings.clone(), s.vaults.clone());
    let values = blocking(move || {
        let values = settings.update(&patch)?;
        vaults.apply_device();
        Ok(values)
    })
    .await?;
    Ok(Json(Value::Object(values)))
}

fn vault_response(s: &AppState, own: Map<String, Value>, problem: Option<SettingsProblem>) -> VaultSettingsResponse {
    let shared = s.settings.values();
    let mut values = shared.clone();
    values.extend(own.clone());
    VaultSettingsResponse { schema: s.settings.schema().clone(), values, shared, own, problem }
}

async fn get_vault_settings(
    State(s): State<AppState>,
    Path(vault): Path<String>,
) -> ApiResult<Json<VaultSettingsResponse>> {
    let vault = s.vault(vault).await?;
    Ok(Json(vault_response(&s, vault.settings.own(), vault.settings.problem().cloned())))
}

async fn put_vault_settings(
    State(s): State<AppState>,
    Path(vault): Path<String>,
    Json(patch): Json<Map<String, Value>>,
) -> ApiResult<Json<VaultSettingsResponse>> {
    let (vault, settings) = (s.vault(vault).await?, s.settings.clone());
    let problem = vault.settings.problem().cloned();
    let own = blocking(move || vault.settings.update(settings.schema(), &patch)).await?;
    Ok(Json(vault_response(&s, own, problem)))
}

async fn themes(State(s): State<AppState>) -> Json<Vec<Theme>> {
    Json(s.vaults.library().themes().themes().to_vec())
}

async fn themes_css(State(s): State<AppState>) -> axum::response::Response {
    css(s.vaults.library().themes().css().to_owned())
}
