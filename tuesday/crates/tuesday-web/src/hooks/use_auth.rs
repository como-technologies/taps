use crate::auth::{AuthState, TokenStorage, refresh_oauth_token};
use dioxus::prelude::*;

/// Hook for managing authentication state with localStorage persistence
/// and automatic token refresh for GitHub App tokens
pub fn use_auth() -> (Signal<AuthState>, Callback<AuthState>) {
    // Always initialize as Unauthenticated for consistent server/client rendering
    // This prevents hydration mismatches since server can't access localStorage
    let mut auth_state = use_signal(|| AuthState::Unauthenticated);

    // Load from localStorage after hydration (client-side only)
    use_effect(move || match TokenStorage::load() {
        Ok(state) if state != AuthState::Unauthenticated => {
            tracing::info!("Loaded auth state from localStorage");
            auth_state.set(state);
        }
        Ok(_) => {
            tracing::debug!("No auth state in localStorage");
        }
        Err(e) => {
            tracing::debug!("Failed to load auth state: {}", e);
        }
    });

    // Set up automatic token refresh for GitHub App tokens
    use_effect(move || {
        let state = auth_state();

        if let AuthState::GitHubAppAuthenticated {
            expires_at,
            refresh_token,
            user_login,
            ..
        } = &state
        {
            let now = chrono::Utc::now();
            let refresh_at = *expires_at - chrono::Duration::minutes(5);

            if refresh_at > now {
                // Schedule refresh
                let delay_ms = (refresh_at - now).num_milliseconds().max(0) as u64;
                let refresh_token = refresh_token.clone();
                let user_login = user_login.clone();

                tracing::info!("Scheduling token refresh in {} seconds", delay_ms / 1000);

                spawn(async move {
                    // Wait until 5 minutes before expiry
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(delay_ms as u32).await;
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }

                    // Attempt refresh
                    tracing::info!("Refreshing OAuth token");
                    match refresh_oauth_token(refresh_token.clone()).await {
                        Ok(new_token) => {
                            let expires_at = new_token.expires_at();
                            let new_state = AuthState::GitHubAppAuthenticated {
                                access_token: new_token.access_token,
                                refresh_token: new_token.refresh_token,
                                expires_at,
                                user_login,
                            };

                            if let Err(e) = TokenStorage::save(&new_state) {
                                tracing::warn!("Failed to save refreshed token: {}", e);
                            }

                            auth_state.set(new_state);
                            tracing::info!("Token refreshed successfully");
                        }
                        Err(e) => {
                            tracing::error!("Token refresh failed: {}", e);
                            // Don't log out yet - token is still valid for ~5 more minutes
                            // User will be logged out when they try to use the expired token
                        }
                    }
                });
            } else {
                // Token is about to expire or already expired
                tracing::warn!("Token is expiring soon or already expired");
            }
        }
    });

    // Callback to update auth state
    let set_auth = Callback::new(move |new_state: AuthState| {
        if let Err(e) = TokenStorage::save(&new_state) {
            tracing::warn!("Failed to save auth state: {}", e);
        }
        auth_state.set(new_state);
    });

    (auth_state, set_auth)
}
