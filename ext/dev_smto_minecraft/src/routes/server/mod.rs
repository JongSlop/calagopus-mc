use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod properties;
mod whitelist;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/properties", properties::router(state))
        .nest("/whitelist", whitelist::router(state))
        .with_state(state.clone())
}
