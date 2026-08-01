use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Authentication state for the application
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum AuthState {
    /// No authentication configured
    #[default]
    Unauthenticated,
    /// Authenticated via Personal Access Token
    PatAuthenticated { token: String },
    /// Authenticated via GitHub App OAuth
    GitHubAppAuthenticated {
        access_token: String,
        refresh_token: String,
        expires_at: DateTime<Utc>,
        user_login: String,
    },
}

impl AuthState {
    /// Returns the active token for API calls, regardless of auth method
    pub fn token(&self) -> Option<&str> {
        match self {
            AuthState::Unauthenticated => None,
            AuthState::PatAuthenticated { token } => Some(token),
            AuthState::GitHubAppAuthenticated { access_token, .. } => Some(access_token),
        }
    }
}
