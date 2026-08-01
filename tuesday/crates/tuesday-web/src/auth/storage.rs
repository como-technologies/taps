use super::state::AuthState;

// localStorage only exists in the browser; the key is wasm-only by cfg.
#[cfg(target_arch = "wasm32")]
const AUTH_STORAGE_KEY: &str = "tuesday_auth_state";

/// LocalStorage wrapper for persisting auth state
pub struct TokenStorage;

impl TokenStorage {
    /// Save auth state to localStorage
    pub fn save(state: &AuthState) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            let json =
                serde_json::to_string(state).map_err(|e| format!("Serialization error: {}", e))?;

            storage
                .set_item(AUTH_STORAGE_KEY, &json)
                .map_err(|_| "Failed to save to localStorage")?;

            tracing::debug!("Auth state saved to localStorage");
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = state;
            tracing::debug!("localStorage not available on server");
            Ok(())
        }
    }

    /// Load auth state from localStorage
    pub fn load() -> Result<AuthState, String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            let json = storage
                .get_item(AUTH_STORAGE_KEY)
                .map_err(|_| "Failed to read from localStorage")?;

            match json {
                Some(data) => {
                    let state = serde_json::from_str(&data)
                        .map_err(|e| format!("Deserialization error: {}", e))?;
                    tracing::debug!("Auth state loaded from localStorage");
                    Ok(state)
                }
                None => {
                    tracing::debug!("No auth state in localStorage");
                    Ok(AuthState::Unauthenticated)
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            tracing::debug!("localStorage not available on server");
            Ok(AuthState::Unauthenticated)
        }
    }

    /// Clear auth state from localStorage
    pub fn clear() -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            storage
                .remove_item(AUTH_STORAGE_KEY)
                .map_err(|_| "Failed to clear localStorage")?;

            tracing::debug!("Auth state cleared from localStorage");
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            tracing::debug!("localStorage not available on server");
            Ok(())
        }
    }
}

/// Store OAuth state parameter in sessionStorage for CSRF protection
pub fn store_oauth_state(state: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        let window = window().ok_or("No window object")?;
        let storage = window
            .session_storage()
            .map_err(|_| "Failed to access sessionStorage")?
            .ok_or("sessionStorage not available")?;

        storage
            .set_item("oauth_state", state)
            .map_err(|_| "Failed to save OAuth state")?;

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = state;
        Err("sessionStorage not available on server".to_string())
    }
}

/// Get stored OAuth state parameter from sessionStorage
pub fn get_oauth_state() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        let window = window()?;
        let storage = window.session_storage().ok()??;
        storage.get_item("oauth_state").ok()?
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Clear OAuth state parameter from sessionStorage
pub fn clear_oauth_state() {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        if let Some(window) = window() {
            if let Ok(Some(storage)) = window.session_storage() {
                let _ = storage.remove_item("oauth_state");
            }
        }
    }
}
