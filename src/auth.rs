// ═════════════════════════════════════════════════════════════════════════
// Authentication & Session Management (Phase A)
// ═════════════════════════════════════════════════════════════════════════

use argon2::{hash_encoded, verify_encoded, Config};
use base64::Engine;
use rand::RngCore;
use sha2::{Sha256, Digest};
use std::sync::{Arc, Mutex};

use crate::db::{VaultDb, SharedVaultDb, UserRecord};
use crate::audit::AuditLog;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
}

impl UserInfo {
    /// Maps user role to vault AccessTier.
    /// admin → Secret, power_user → Confidential, user → Restricted, guest → Public
    pub fn access_tier(&self) -> crate::db::AccessTier {
        match self.role.as_str() {
            "admin" => crate::db::AccessTier::Secret,
            "power_user" => crate::db::AccessTier::Confidential,
            "user" => crate::db::AccessTier::Restricted,
            _ => crate::db::AccessTier::Public,
        }
    }
}

impl From<UserRecord> for UserInfo {
    fn from(u: UserRecord) -> Self {
        UserInfo {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            role: u.role,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub token: Option<String>,
    pub user: Option<UserInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeResponse {
    pub ok: bool,
    pub user: Option<UserInfo>,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Token Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a cryptographically random 32-byte raw token and its SHA-256 hash.
/// Raw token goes to the client (base64url encoded). Hash is stored in DB.
pub fn generate_session_token() -> (Vec<u8>, Vec<u8>) {
    let raw: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let hash = sha256_hash(&raw);
    (raw, hash)
}

/// SHA-256 hash of the given data.
pub fn sha256_hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Parse "Bearer <base64url-token>" from an Authorization header value.
/// Returns the raw token bytes on success.
pub fn extract_bearer_token(auth_header: &str) -> Option<Vec<u8>> {
    let prefix = "Bearer ";
    if auth_header.starts_with(prefix) {
        let encoded = &auth_header[prefix.len()..];
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .ok()
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Password Hashing (Argon2id)
// ─────────────────────────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
    let config = Config::default();
    hash_encoded(password.as_bytes(), &salt, &config)
        .map_err(|e| format!("Argon2id hashing failed: {}", e))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    verify_encoded(hash, password.as_bytes())
        .map_err(|e| format!("Password verification error: {}", e))
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Expiry Helpers
// ─────────────────────────────────────────────────────────────────────────────

const SESSION_ABSOLUTE_SECS: i64 = 24 * 60 * 60;
const SESSION_INACTIVITY_SECS: i64 = 8 * 60 * 60;

pub fn inactivity_expiry() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(SESSION_INACTIVITY_SECS))
        .to_rfc3339()
}

pub fn extend_session_expiry() -> String {
    inactivity_expiry()
}

// ─────────────────────────────────────────────────────────────────────────────
// Rate Limiting
// ─────────────────────────────────────────────────────────────────────────────

const MAX_LOGIN_ATTEMPTS: i64 = 5;
const RATE_LIMIT_WINDOW_SECS: i64 = 60;

pub fn check_rate_limit(db: &VaultDb, ip_address: &str) -> Result<(), String> {
    let now = chrono::Utc::now();
    let since = (now - chrono::Duration::seconds(RATE_LIMIT_WINDOW_SECS))
        .to_rfc3339();
    let count = db
        .count_recent_login_attempts(ip_address, &since)
        .map_err(|e| format!("DB error checking rate limit: {}", e))?;
    if count >= MAX_LOGIN_ATTEMPTS {
        return Err(format!(
            "Rate limit exceeded: {} attempts in {} seconds (max {})",
            count, RATE_LIMIT_WINDOW_SECS, MAX_LOGIN_ATTEMPTS
        ));
    }
    Ok(())
}

fn record_failed_login(
    db: &VaultDb,
    ip_address: &str,
    username: Option<&str>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.record_login_attempt(ip_address, &now, username)
        .map_err(|e| format!("DB error recording login attempt: {}", e))
}

// ─────────────────────────────────────────────────────────────────────────────
// AuthService
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AuthService {
    db: SharedVaultDb,
    audit_log: Arc<Mutex<AuditLog>>,
}

impl AuthService {
    pub fn new(db: SharedVaultDb, audit_log: Arc<Mutex<AuditLog>>) -> Self {
        Self { db, audit_log }
    }

    /// Attempt login. Returns (raw_token, UserInfo) on success.
    pub fn login(
        &self,
        username: &str,
        password: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(Vec<u8>, UserInfo), String> {
        let db = self.db.lock().map_err(|e| format!("DB lock poisoned: {}", e))?;

        // Rate limit check
        let ip = ip_address.unwrap_or("unknown");
        check_rate_limit(&db, ip)?;

        // Find user
        let user = db
            .get_user_by_username(username)
            .map_err(|e| format!("DB error: {}", e))?
            .ok_or_else(|| "Invalid username or password".to_string())?;

        if !user.is_active {
            return Err("Account is disabled".to_string());
        }

        // Verify password
        let valid = verify_password(password, &user.password_hash)
            .map_err(|e| format!("Password verification error: {}", e))?;
        if !valid {
            let _ = record_failed_login(&db, ip, Some(username));
            let _ = self.audit_log.lock().map(|mut log| {
                log.push_user("login_failed", serde_json::json!({
                    "username": username,
                    "ip": ip,
                    "reason": "invalid_password"
                }))
            });
            return Err("Invalid username or password".to_string());
        }

        // Create session
        let (raw_token, token_hash) = generate_session_token();
        let now = chrono::Utc::now().to_rfc3339();
        let expires_at = inactivity_expiry();

        db.create_session(&token_hash, user.id, &now, &expires_at, Some(ip), user_agent)
            .map_err(|e| format!("Failed to create session: {}", e))?;

        // Update last_login
        let _ = db.update_last_login(user.id, &now);

        // Purge old login attempts (don't fail login for this)
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let _ = db.purge_old_login_attempts(&cutoff);

        // Audit success
        let _ = self.audit_log.lock().map(|mut log| {
            log.push_user("login_success", serde_json::json!({
                "user_id": user.id,
                "username": username,
                "ip": ip
            }))
        });

        let user_info = UserInfo::from(user);
        Ok((raw_token, user_info))
    }

    /// Validate a raw token: hash it and look up a valid session.
    /// Also extends the inactivity window. Returns UserInfo on success.
    pub fn validate_token(&self, raw_token: &[u8]) -> Result<UserInfo, String> {
        let db = self.db.lock().map_err(|e| format!("DB lock poisoned: {}", e))?;

        let token_hash = sha256_hash(raw_token);
        let now = chrono::Utc::now().to_rfc3339();

        let session = db
            .get_valid_session(&token_hash, &now)
            .map_err(|e| format!("DB error looking up session: {}", e))?
            .ok_or("Invalid or expired session".to_string())?;

        // Touch session to extend inactivity window
        let new_expiry = extend_session_expiry();
        let _ = db.touch_session(&token_hash, &new_expiry);

        let user = db
            .get_user_by_id(session.user_id)
            .map_err(|e| format!("DB error looking up user: {}", e))?
            .ok_or("Session user not found".to_string())?;

        if !user.is_active {
            return Err("Account is disabled".to_string());
        }

        Ok(UserInfo::from(user))
    }

    /// Logout: delete the session from DB.
    pub fn logout(&self, raw_token: &[u8]) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("DB lock poisoned: {}", e))?;
        let token_hash = sha256_hash(raw_token);
        db.delete_session(&token_hash)
            .map_err(|e| format!("DB error during logout: {}", e))?;
        Ok(())
    }

    /// Create default admin user if no users exist.
    /// Reads password from AVALON_INITIAL_ADMIN_PASSWORD env var (fallback: "admin").
    pub fn ensure_admin_user(&self) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("DB lock poisoned: {}", e))?;

        let users = db.list_users().map_err(|e| format!("DB error: {}", e))?;
        if !users.is_empty() {
            return Ok(()); // already has users
        }

        let default_password = std::env::var("AVALON_INITIAL_ADMIN_PASSWORD")
            .unwrap_or_else(|_| "admin".to_string());
        let hash = hash_password(&default_password)
            .map_err(|e| format!("Failed to hash admin password: {}", e))?;
        let now = chrono::Utc::now().to_rfc3339();

        db.insert_user(
            "admin",
            Some("Administrator"),
            &hash,
            "admin",
            &now,
        )
        .map_err(|e| format!("Failed to create admin user: {}", e))?;

        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// require_auth helper — used inside protected handlers
// ─────────────────────────────────────────────────────────────────────────────

use actix_web::{HttpRequest, HttpResponse, web};

pub async fn require_auth(
    req: &HttpRequest,
    state: &web::Data<crate::AppState>,
) -> Result<UserInfo, HttpResponse> {
    let header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Missing Authorization header"
            }))
        })?;

    let token_bytes = extract_bearer_token(header).ok_or_else(|| {
        HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid Authorization header format. Use: Bearer <token>"
        }))
    })?;

    state
        .auth
        .validate_token(&token_bytes)
        .map_err(|e| HttpResponse::Unauthorized().json(serde_json::json!({ "error": e })))
}