use crate::{
    config::App,
    helper::{Error, Result, bad},
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn denied() -> Error {
    Error(StatusCode::UNAUTHORIZED, "Authentication required".into())
}
fn secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        #[cfg(test)]
        {
            return "test-only-secret-at-least-thirty-two-bytes".into();
        }
        #[cfg(not(test))]
        {
            panic!("JWT_SECRET is required")
        }
    })
}
pub(crate) fn validate_config() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if secret().len() < 32 {
        return Err("JWT_SECRET must contain at least 32 bytes".into());
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub sub: i64,
    pub sid: String,
    jti: String,
    exp: i64,
    iat: i64,
    iss: String,
    aud: String,
    kind: String,
}
fn token(uid: i64, sid: &str, kind: &str, expiry: i64) -> Result<(String, String)> {
    let jti = Uuid::new_v4().to_string();
    let c = Claims {
        sub: uid,
        sid: sid.into(),
        jti: jti.clone(),
        exp: expiry,
        iat: now(),
        iss: "stockroom".into(),
        aud: "stockroom-web".into(),
        kind: kind.into(),
    };
    let t = encode(
        &Header::new(Algorithm::HS256),
        &c,
        &EncodingKey::from_secret(secret().as_bytes()),
    )
    .map_err(|_| bad("Token creation failed"))?;
    Ok((t, jti))
}
fn read(headers: &HeaderMap, name: &str, kind: &str) -> Result<Claims> {
    let raw = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|c| c.trim().strip_prefix(&format!("{name}=")))
        })
        .ok_or_else(denied)?;
    let mut v = Validation::new(Algorithm::HS256);
    v.set_issuer(&["stockroom"]);
    v.set_audience(&["stockroom-web"]);
    v.leeway = 0;
    let c = decode::<Claims>(raw, &DecodingKey::from_secret(secret().as_bytes()), &v)
        .map_err(|_| denied())?
        .claims;
    if c.kind != kind {
        return Err(denied());
    }
    Ok(c)
}
fn cookies(access: &str, refresh: &str, expiry: i64) -> Response {
    let mut r = Json(json!({"ok":true})).into_response();
    let secure = if std::env::var("COOKIE_SECURE").as_deref() == Ok("false") {
        ""
    } else {
        "; Secure"
    };
    for (name, value, path, age) in [
        (
            "access_token",
            access,
            "/api",
            if access.is_empty() { 0 } else { 900 },
        ),
        (
            "refresh_token",
            refresh,
            "/api/auth",
            if refresh.is_empty() {
                0
            } else {
                (expiry - now()).max(0)
            },
        ),
    ] {
        r.headers_mut().append(
            header::SET_COOKIE,
            format!(
                "{name}={value}; Path={path}; HttpOnly; SameSite=Strict; Max-Age={age}{secure}"
            )
            .parse()
            .unwrap(),
        );
    }
    r.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    r
}
#[derive(Deserialize)]
pub(crate) struct Login {
    username: String,
    password: String,
}
pub(crate) async fn login(State(a): State<App>, Json(input): Json<Login>) -> Result<Response> {
    if input.username.len() > 100 || input.password.len() > 1024 {
        return Err(denied());
    }
    let username = input.username.trim().to_lowercase();
    sqlx::query("INSERT IGNORE INTO login_attempts(username) VALUES(?)")
        .bind(&username)
        .execute(&a.db)
        .await?;
    let mut tx = a.db.begin().await?;
    let (failures, locked): (i32, i64) = sqlx::query_as(
        "SELECT failures,locked_until FROM login_attempts WHERE username=? FOR UPDATE",
    )
    .bind(&username)
    .fetch_one(&mut *tx)
    .await?;
    if locked > now() {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again later".into(),
        ));
    }
    let user: Option<(i64, String)> = sqlx::query_as(
        "SELECT id,password_hash FROM users WHERE username=? AND active=TRUE FOR UPDATE",
    )
    .bind(&username)
    .fetch_optional(&mut *tx)
    .await?;
    let hash=user.as_ref().map(|u|u.1.clone()).unwrap_or_else(|| "$argon2id$v=19$m=19456,t=2,p=1$c29tZXJhbmRvbXNhbHQ$uRpGJ8iqU1KmQ7CPGvdz4sDIoK5qyl4DFzT3g1yHfBU".into());
    let password = input.password;
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|h| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &h)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    if !valid || user.is_none() {
        let attempts = if locked > 0 && locked <= now() {
            1
        } else {
            failures.saturating_add(1)
        };
        sqlx::query("UPDATE login_attempts SET failures=?,locked_until=? WHERE username=?")
            .bind(attempts)
            .bind(if attempts >= 5 { now() + 900 } else { 0 })
            .bind(&username)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(denied());
    }
    sqlx::query("UPDATE login_attempts SET failures=0,locked_until=0 WHERE username=?")
        .bind(&username)
        .execute(&mut *tx)
        .await?;
    let uid = user.unwrap().0;
    let sid = Uuid::new_v4().to_string();
    let exp = now() + 604800;
    let (refresh, jti) = token(uid, &sid, "refresh", exp)?;
    let (access, _) = token(uid, &sid, "access", now() + 900)?;
    sqlx::query("INSERT INTO auth_sessions(id,user_id,expires_at) VALUES(?,?,?)")
        .bind(&sid)
        .bind(uid)
        .bind(exp)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO refresh_tokens(jti,session_id) VALUES(?,?)")
        .bind(jti)
        .bind(&sid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(cookies(&access, &refresh, exp))
}
pub(crate) async fn refresh(State(a): State<App>, headers: HeaderMap) -> Result<Response> {
    let c = read(&headers, "refresh_token", "refresh")?;
    let mut tx = a.db.begin().await?;
    let session: Option<(i64, i64, bool)> = sqlx::query_as(
        "SELECT user_id,expires_at,revoked FROM auth_sessions WHERE id=? FOR UPDATE",
    )
    .bind(&c.sid)
    .fetch_optional(&mut *tx)
    .await?;
    let (uid, exp, revoked) = session.ok_or_else(denied)?;
    if revoked || exp <= now() || uid != c.sub {
        return Err(denied());
    }
    let active: (bool,) = sqlx::query_as("SELECT active FROM users WHERE id=?")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?;
    if !active.0 {
        return Err(denied());
    }
    let used: Option<(bool,)> =
        sqlx::query_as("SELECT used FROM refresh_tokens WHERE jti=? AND session_id=?")
            .bind(&c.jti)
            .bind(&c.sid)
            .fetch_optional(&mut *tx)
            .await?;
    if used != Some((false,)) {
        sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE id=?")
            .bind(&c.sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "Refresh token reuse detected; session revoked".into(),
        ));
    }
    sqlx::query("UPDATE refresh_tokens SET used=TRUE WHERE jti=?")
        .bind(&c.jti)
        .execute(&mut *tx)
        .await?;
    let (refresh, jti) = token(uid, &c.sid, "refresh", exp)?;
    let (access, _) = token(uid, &c.sid, "access", now() + 900)?;
    sqlx::query("INSERT INTO refresh_tokens(jti,session_id) VALUES(?,?)")
        .bind(jti)
        .bind(&c.sid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(cookies(&access, &refresh, exp))
}
pub(crate) async fn logout(State(a): State<App>, h: HeaderMap) -> Result<Response> {
    if let Ok(c) =
        read(&h, "refresh_token", "refresh").or_else(|_| read(&h, "access_token", "access"))
    {
        sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE id=? AND user_id=?")
            .bind(c.sid)
            .bind(c.sub)
            .execute(&a.db)
            .await?;
    }
    Ok(cookies("", "", 0))
}
pub(crate) async fn logout_all(
    State(a): State<App>,
    axum::Extension(c): axum::Extension<Claims>,
) -> Result<Response> {
    sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE user_id=?")
        .bind(c.sub)
        .execute(&a.db)
        .await?;
    Ok(cookies("", "", 0))
}
pub(crate) async fn allowed(a: &App, uid: i64, permission: &str) -> Result<bool> {
    Ok(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM users u JOIN role_permissions rp ON rp.role_id=u.role_id WHERE u.id=? AND u.active=TRUE AND rp.permission=?").bind(uid).bind(permission).fetch_one(&a.db).await?>0)
}
pub(crate) async fn me(
    State(a): State<App>,
    axum::Extension(c): axum::Extension<Claims>,
) -> Result<Json<Value>> {
    let (username, role): (String, String) = sqlx::query_as(
        "SELECT u.username,r.name FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?",
    )
    .bind(c.sub)
    .fetch_one(&a.db)
    .await?;
    let permissions:Vec<String>=sqlx::query_scalar("SELECT permission FROM role_permissions WHERE role_id=(SELECT role_id FROM users WHERE id=?) ORDER BY permission").bind(c.sub).fetch_all(&a.db).await?;
    Ok(Json(
        json!({"id":c.sub,"username":username,"role":role,"permissions":permissions}),
    ))
}
pub(crate) async fn guard(State(a): State<App>, mut req: Request, next: Next) -> Result<Response> {
    let path = req.uri().path().to_string();
    let write = !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    if write {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let expected = std::env::var("FRONTEND_ORIGIN").unwrap_or("http://localhost:5173".into());
        if origin.is_some_and(|v| v != expected)
            || req
                .headers()
                .get("sec-fetch-site")
                .is_some_and(|v| v == "cross-site")
        {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Untrusted request origin".into(),
            ));
        }
        // JSON/custom headers prevent simple cross-origin form submissions, including login CSRF.
        if req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| !v.starts_with("application/json"))
        {
            return Err(bad("Content-Type application/json required"));
        }
    }
    if !path.starts_with("/api/")
        || path == "/api/health"
        || ["/api/auth/login", "/api/auth/refresh", "/api/auth/logout"].contains(&path.as_str())
    {
        return Ok(private(next.run(req).await));
    }
    let c = read(req.headers(), "access_token", "access")?;
    let valid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.id=? AND s.user_id=? AND s.revoked=FALSE AND s.expires_at>? AND u.active=TRUE").bind(&c.sid).bind(c.sub).bind(now()).fetch_one(&a.db).await?;
    if valid == 0 {
        return Err(denied());
    }
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or("");
    let permission = match route {
        "/api/auth/me" | "/api/auth/password" | "/api/auth/logout-all" => "profile.read",
        "/api/roles" | "/api/roles/{id}" | "/api/users" | "/api/users/{id}/role" => "roles.manage",
        "/api/payments" | "/api/payments/{id}" => {
            if write {
                "payments.write"
            } else {
                "payments.read"
            }
        }
        "/api/products" | "/api/products/{id}" => {
            if write {
                "products.write"
            } else {
                "products.read"
            }
        }
        "/api/void-requests/{id}/approve" | "/api/void-requests/{id}/reject" => "invoices.void",
        "/api/void-requests" | "/api/invoices/{id}/cancel" | "/api/sales-invoices/{id}/cancel" => {
            "invoices.request_void"
        }
        "/api/delivery-cancellation-requests/{id}/approve"
        | "/api/delivery-cancellation-requests/{id}/reject"
        | "/api/delivery-orders/{id}/approve" => "delivery.approve",
        "/api/delivery-cancellation-requests" => "delivery.read",
        "/api/delivery-orders" | "/api/delivery-orders/{id}" => {
            if write {
                "delivery.write"
            } else {
                "delivery.read"
            }
        }
        "/api/delivery-orders/{id}/status" | "/api/delivery-orders/{id}/cancel-request" => {
            "delivery.write"
        }
        "/api/search/reindex" => "search.manage",
        "/api/documents"
        | "/api/purchase-orders"
        | "/api/purchase-invoices"
        | "/api/sales-invoices" => {
            if write {
                "invoices.write"
            } else {
                "invoices.read"
            }
        }
        "/api/documents/preview" | "/api/invoices/{id}" => "invoices.write",
        "/api/documents/{id}" | "/api/forecast" | "/api/search" => "invoices.read",
        _ => return Err(Error(StatusCode::NOT_FOUND, "Endpoint not found".into())),
    };
    if !allowed(&a, c.sub, permission).await? {
        return Err(Error(
            StatusCode::FORBIDDEN,
            format!("Missing permission: {permission}"),
        ));
    }
    req.extensions_mut().insert(c.clone());
    if !write || path.starts_with("/api/auth/") || path.ends_with("/preview") {
        return Ok(private(next.run(req).await));
    }
    let key = req
        .headers()
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| bad("Idempotency-Key required"))?
        .to_string();
    if key.is_empty() || key.len() > 128 || !key.is_ascii() {
        return Err(bad("Invalid Idempotency-Key"));
    }
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, 2 * 1024 * 1024)
        .await
        .map_err(|_| bad("Request body too large"))?;
    let mut digest = Sha256::new();
    digest.update(parts.method.as_str().as_bytes());
    digest.update([0]);
    digest.update(parts.uri.to_string().as_bytes());
    digest.update([0]);
    digest.update(&bytes);
    let fingerprint = format!("{:x}", digest.finalize());
    let inserted=sqlx::query("INSERT IGNORE INTO idempotency_requests(user_id,request_key,fingerprint,created_at) VALUES(?,?,?,?)").bind(c.sub).bind(&key).bind(&fingerprint).bind(now()).execute(&a.db).await?.rows_affected();
    if inserted == 0 {
        let(fp,status,body):(String,Option<i32>,Option<Vec<u8>>)=sqlx::query_as("SELECT fingerprint,status_code,response FROM idempotency_requests WHERE user_id=? AND request_key=?").bind(c.sub).bind(&key).fetch_one(&a.db).await?;
        if fp != fingerprint {
            return Err(Error(
                StatusCode::CONFLICT,
                "Idempotency key reused with different request".into(),
            ));
        }
        if let (Some(s), Some(b)) = (status, body) {
            return Ok((
                StatusCode::from_u16(s as u16).unwrap(),
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::CACHE_CONTROL, "no-store"),
                    (
                        axum::http::HeaderName::from_static("idempotency-replayed"),
                        "true",
                    ),
                ],
                b,
            )
                .into_response());
        }
        return Err(Error(
            StatusCode::CONFLICT,
            "Request pending; reconcile before retrying with a new key".into(),
        ));
    }
    let response = next
        .run(Request::from_parts(parts, Body::from(bytes)))
        .await;
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, 16 * 1024 * 1024)
        .await
        .map_err(|_| bad("Response too large"))?;
    sqlx::query("UPDATE idempotency_requests SET status_code=?,response=? WHERE user_id=? AND request_key=?").bind(parts.status.as_u16() as i32).bind(bytes.as_ref()).bind(c.sub).bind(key).execute(&a.db).await?;
    Ok(private(Response::from_parts(parts, Body::from(bytes))))
}
pub(crate) async fn cleanup(a: &App) -> Result<()> {
    sqlx::query("DELETE FROM auth_sessions WHERE expires_at<=?")
        .bind(now())
        .execute(&a.db)
        .await?;
    sqlx::query("DELETE FROM idempotency_requests WHERE status_code IS NOT NULL AND created_at<?")
        .bind(now() - 86400)
        .execute(&a.db)
        .await?;
    sqlx::query("DELETE FROM login_attempts WHERE locked_until>0 AND locked_until<?")
        .bind(now() - 86400)
        .execute(&a.db)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct RoleInput {
    name: String,
    permissions: Vec<String>,
}
pub(crate) async fn roles(State(a): State<App>) -> Result<Json<Value>> {
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id,name FROM roles ORDER BY id")
        .fetch_all(&a.db)
        .await?;
    let mut result = Vec::new();
    for (id, name) in rows {
        let permissions: Vec<String> = sqlx::query_scalar(
            "SELECT permission FROM role_permissions WHERE role_id=? ORDER BY permission",
        )
        .bind(id)
        .fetch_all(&a.db)
        .await?;
        result.push(json!({"id":id,"name":name,"permissions":permissions}));
    }
    let permissions: Vec<String> = sqlx::query_scalar("SELECT name FROM permissions ORDER BY name")
        .fetch_all(&a.db)
        .await?;
    Ok(Json(json!({"roles":result,"permissions":permissions})))
}
pub(crate) async fn save_role(
    State(a): State<App>,
    Json(d): Json<RoleInput>,
) -> Result<Json<Value>> {
    if d.name.trim().is_empty()
        || d.name.len() > 50
        || ["Admin", "Staff", "Finance"].contains(&d.name.as_str())
    {
        return Err(bad(
            "Use a new custom role name; built-in roles are protected",
        ));
    }
    let mut tx = a.db.begin().await?;
    let id = sqlx::query("INSERT INTO roles(name) VALUES(?)")
        .bind(d.name.trim())
        .execute(&mut *tx)
        .await?
        .last_insert_id() as i64;
    for p in std::collections::HashSet::<String>::from_iter(d.permissions) {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE name=?")
            .bind(&p)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            return Err(bad("Unknown permission"));
        }
        sqlx::query("INSERT INTO role_permissions(role_id,permission) VALUES(?,?)")
            .bind(id)
            .bind(p)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Deserialize)]
pub(crate) struct UserRole {
    role_id: i64,
    active: bool,
}
pub(crate) async fn user_role(
    State(a): State<App>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::Extension(c): axum::Extension<Claims>,
    Json(d): Json<UserRole>,
) -> Result<Json<Value>> {
    if id == c.sub {
        return Err(bad("Cannot change your own role or active state"));
    }
    let mut tx = a.db.begin().await?;
    sqlx::query("UPDATE users SET role_id=?,active=? WHERE id=?")
        .bind(d.role_id)
        .bind(d.active)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE user_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}

pub(crate) async fn users(State(a): State<App>) -> Result<Json<Value>> {
    let rows: Vec<(i64, String, i64, bool)> =
        sqlx::query_as("SELECT id,username,role_id,active FROM users ORDER BY id")
            .fetch_all(&a.db)
            .await?;
    Ok(Json(json!(rows.into_iter().map(|(id,username,role_id,active)|json!({"id":id,"username":username,"role_id":role_id,"active":active})).collect::<Vec<_>>())))
}
pub(crate) async fn update_role(
    State(a): State<App>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(d): Json<RoleInput>,
) -> Result<Json<Value>> {
    if d.name.trim().is_empty() || d.name.len() > 50 {
        return Err(bad("Valid role name required"));
    }
    let mut tx = a.db.begin().await?;
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM roles WHERE id=? FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let name = name.ok_or_else(|| bad("Role not found"))?;
    if name == "Admin"
        || (name != d.name.trim()
            && (["Staff", "Finance"].contains(&name.as_str())
                || ["Admin", "Staff", "Finance"].contains(&d.name.trim())))
    {
        return Err(bad("Built-in roles are protected; create a custom role"));
    }
    sqlx::query("UPDATE roles SET name=? WHERE id=?")
        .bind(d.name.trim())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM role_permissions WHERE role_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for p in std::collections::HashSet::<String>::from_iter(d.permissions) {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE name=?")
            .bind(&p)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            return Err(bad("Unknown permission"));
        }
        sqlx::query("INSERT INTO role_permissions(role_id,permission) VALUES(?,?)")
            .bind(id)
            .bind(p)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE user_id IN (SELECT id FROM users WHERE role_id=?)").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Deserialize)]
pub(crate) struct NewUser {
    username: String,
    password: String,
    role_id: i64,
}
pub(crate) async fn create_user(
    State(a): State<App>,
    Json(d): Json<NewUser>,
) -> Result<Json<Value>> {
    if d.username.trim().is_empty()
        || d.username.len() > 100
        || d.password.len() < 10
        || d.password.len() > 1024
    {
        return Err(bad("Username required; password must have 10-1024 bytes"));
    }
    let hash = tokio::task::spawn_blocking(move || {
        use argon2::{
            PasswordHasher,
            password_hash::{SaltString, rand_core::OsRng},
        };
        Argon2::default()
            .hash_password(d.password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
            .map_err(|_| bad("Password hashing failed"))
    })
    .await
    .map_err(|_| bad("Password hashing failed"))??;
    let id = sqlx::query("INSERT INTO users(username,password_hash,role_id) VALUES(?,?,?)")
        .bind(d.username.trim().to_lowercase())
        .bind(hash)
        .bind(d.role_id)
        .execute(&a.db)
        .await?
        .last_insert_id();
    Ok(Json(json!({"id":id})))
}

#[cfg(test)]
mod jwt_tests {
    use super::*;
    #[test]
    fn rejects_expired_wrong_kind_and_tampered_tokens() {
        let sid = Uuid::new_v4().to_string();
        let (expired, _) = token(1, &sid, "access", now() - 1).unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("access_token={expired}").parse().unwrap(),
        );
        assert!(read(&h, "access_token", "access").is_err());
        let (refresh, _) = token(1, &sid, "refresh", now() + 900).unwrap();
        h.insert(
            header::COOKIE,
            format!("access_token={refresh}").parse().unwrap(),
        );
        assert!(read(&h, "access_token", "access").is_err());
        let (access, _) = token(1, &sid, "access", now() + 900).unwrap();
        h.insert(
            header::COOKIE,
            format!("access_token={access}").parse().unwrap(),
        );
        assert!(read(&h, "access_token", "access").is_ok());
        let mut bytes = access.into_bytes();
        let last = bytes.len() - 2;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        h.insert(
            header::COOKIE,
            format!("access_token={}", String::from_utf8(bytes).unwrap())
                .parse()
                .unwrap(),
        );
        assert!(read(&h, "access_token", "access").is_err());
    }
}

#[derive(Deserialize)]
pub(crate) struct PasswordChange {
    current_password: String,
    new_password: String,
}
pub(crate) async fn change_password(
    State(a): State<App>,
    axum::Extension(c): axum::Extension<Claims>,
    Json(d): Json<PasswordChange>,
) -> Result<Response> {
    if d.current_password.len() > 1024 || d.new_password.len() < 10 || d.new_password.len() > 1024 {
        return Err(bad("New password must contain 10-1024 bytes"));
    }
    let old: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=?")
        .bind(c.sub)
        .fetch_one(&a.db)
        .await?;
    let old_verify = old.clone();
    let next = tokio::task::spawn_blocking(move || {
        use argon2::{
            PasswordHasher,
            password_hash::{SaltString, rand_core::OsRng},
        };
        let hash = PasswordHash::new(&old_verify).map_err(|_| denied())?;
        Argon2::default()
            .verify_password(d.current_password.as_bytes(), &hash)
            .map_err(|_| denied())?;
        Argon2::default()
            .hash_password(d.new_password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
            .map_err(|_| bad("Password hashing failed"))
    })
    .await
    .map_err(|_| bad("Password hashing failed"))??;
    let mut tx = a.db.begin().await?;
    if sqlx::query("UPDATE users SET password_hash=? WHERE id=? AND password_hash=?")
        .bind(next)
        .bind(c.sub)
        .bind(old)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(bad("Password changed concurrently; sign in again"));
    }
    sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE user_id=?")
        .bind(c.sub)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(cookies("", "", 0))
}

fn private(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
