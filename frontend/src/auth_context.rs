use dioxus::prelude::*;
use shared::AdminUserDto;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    Loading,
    Unauthenticated,
    Authenticated(AdminUserDto),
}

#[derive(Clone, Copy)]
pub struct AuthContext {
    pub state: Signal<AuthState>,
}

pub fn use_init_auth_provider() -> AuthContext {
    let mut state = use_signal(|| AuthState::Loading);

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            match crate::api::fetch_me().await {
                Ok(session) if session.authenticated => {
                    if let Some(user) = session.user {
                        state.set(AuthState::Authenticated(user));
                    } else {
                        state.set(AuthState::Unauthenticated);
                    }
                }
                _ => state.set(AuthState::Unauthenticated),
            }
        });
    });

    use_context_provider(|| AuthContext { state })
}

pub fn use_auth() -> AuthContext {
    use_context::<AuthContext>()
}
