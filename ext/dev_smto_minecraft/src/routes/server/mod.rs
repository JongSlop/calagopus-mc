use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod properties;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/properties", properties::router(state))
        .with_state(state.clone())
}
