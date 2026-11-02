//! The HTTP interface. Everything is under `/api/v1`; signed-in requests carry
//! `Authorization: Bearer <token>`. Errors are `{"error": code, "message": text}`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::io::ReaderStream;

use crate::Config;
use crate::accounts::{Accounts, NoSuchUser, Taken, User};
use crate::store::{self, Limits, StoreError};

pub struct AppState {
    pub config: Config,
    pub accounts: Accounts,
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
}

impl AppState {
    pub fn new(config: Config, accounts: Accounts) -> Self {
        Self {
            config,
            accounts,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    fn files(&self, user: &User) -> PathBuf {
        self.config.data.join("files").join(user.id.to_string())
    }

    fn limits(&self) -> Limits {
        Limits {
            max_file: self.config.max_upload_bytes,
            quota: self.config.quota_bytes,
        }
    }

    /// How long a name must wait before trying again, if it must.
    fn locked_for(&self, name: &str) -> Option<Duration> {
        let attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        let (_, until) = attempts.get(&name.to_lowercase())?;
        until.checked_duration_since(Instant::now())
    }

    /// After a few wrong passwords each further one waits longer, up to 15 minutes.
    fn failed(&self, name: &str) {
        let mut attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        let entry = attempts
            .entry(name.to_lowercase())
            .or_insert((0, Instant::now()));
        entry.0 += 1;
        if entry.0 >= FREE_ATTEMPTS {
            let wait = 30u64 << (entry.0 - FREE_ATTEMPTS).min(5);
            entry.1 = Instant::now() + Duration::from_secs(wait.min(15 * 60));
        }
    }

    fn succeeded(&self, name: &str) {
        self.attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&name.to_lowercase());
    }
}

const FREE_ATTEMPTS: u32 = 5;

type Shared = Arc<AppState>;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .route("/api/v1/register", post(register))
        .route("/api/v1/login", post(login))
        .route("/api/v1/logout", post(logout))
        .route("/api/v1/password", post(change_password))
        .route("/api/v1/me", get(me))
        .route("/api/v1/list", get(list))
        .route("/api/v1/mkdir", post(mkdir))
        .route(
            "/api/v1/file",
            get(read)
                .put(write)
                .delete(remove)
                // Uploads are checked against the configured limit while they stream in.
                .layer(DefaultBodyLimit::disable()),
        )
        .with_state(Arc::new(state))
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
        }
    }

    fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("{e}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "the server ran into a problem",
        )
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "sign in again: the token is missing, wrong or revoked",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(json!({ "error": self.code, "message": self.message })),
        )
            .into_response();
        if let Some(secs) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(secs));
        }
        response
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::BadPath => Self::new(
                StatusCode::BAD_REQUEST,
                "bad_path",
                "that path isn't allowed",
            ),
            StoreError::NotFound => Self::new(StatusCode::NOT_FOUND, "not_found", "no such file"),
            StoreError::TooLarge => Self::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "the file is larger than this server accepts",
            ),
            StoreError::QuotaFull => Self::new(
                StatusCode::INSUFFICIENT_STORAGE,
                "quota",
                "this account's storage is full",
            ),
            StoreError::Io(e) => Self::internal(e),
        }
    }
}

/// The signed-in account of a request.
pub struct SignedIn {
    user: User,
    token: String,
}

impl FromRequestParts<Shared> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(ApiError::unauthorized)?
            .to_owned();
        let user = state
            .accounts
            .user_for_token(&token)
            .map_err(ApiError::internal)?
            .ok_or_else(ApiError::unauthorized)?;
        Ok(Self { user, token })
    }
}

/// Runs password hashing away from the request threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<anyhow::Result<T>, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(ApiError::internal)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    server: &'static str,
    version: &'static str,
    registration: bool,
    min_password: usize,
}

async fn info(State(state): State<Shared>) -> Json<Info> {
    Json(Info {
        server: "aireader",
        version: env!("CARGO_PKG_VERSION"),
        registration: state.config.open_registration,
        min_password: crate::accounts::MIN_PASSWORD,
    })
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct Session {
    token: String,
    username: String,
}

async fn register(
    State(state): State<Shared>,
    Json(form): Json<Credentials>,
) -> Result<(StatusCode, Json<Session>), ApiError> {
    if !state.config.open_registration {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "registration_closed",
            "this server doesn't take new accounts; ask its owner for one",
        ));
    }
    let username = form.username.trim().to_owned();
    let worker = state.clone();
    let user = match blocking(move || worker.accounts.add(&username, &form.password)).await? {
        Ok(user) => user,
        Err(e) if e.is::<Taken>() => {
            return Err(ApiError::new(StatusCode::CONFLICT, "taken", e.to_string()));
        }
        Err(e) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid",
                e.to_string(),
            ));
        }
    };
    let token = state
        .accounts
        .issue_token(&user)
        .map_err(ApiError::internal)?;
    tracing::info!("new account {}", user.name);
    Ok((
        StatusCode::CREATED,
        Json(Session {
            token,
            username: user.name,
        }),
    ))
}

async fn login(
    State(state): State<Shared>,
    Json(form): Json<Credentials>,
) -> Result<Json<Session>, ApiError> {
    let username = form.username.trim().to_owned();
    if let Some(wait) = state.locked_for(&username) {
        let mut e = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "locked",
            "too many wrong passwords; try again in a while",
        );
        e.retry_after = Some(wait.as_secs().max(1));
        return Err(e);
    }
    let worker = state.clone();
    let name = username.clone();
    let user = blocking(move || worker.accounts.sign_in(&name, &form.password))
        .await?
        .map_err(ApiError::internal)?;
    let Some(user) = user else {
        state.failed(&username);
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "wrong_password",
            "wrong name or password",
        ));
    };
    state.succeeded(&username);
    let token = state
        .accounts
        .issue_token(&user)
        .map_err(ApiError::internal)?;
    Ok(Json(Session {
        token,
        username: user.name,
    }))
}

async fn logout(State(state): State<Shared>, signed_in: SignedIn) -> Result<StatusCode, ApiError> {
    state
        .accounts
        .revoke_token(&signed_in.token)
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct NewPassword {
    old: String,
    new: String,
}

/// Changes the password; every device signs in again, this one gets a fresh token.
async fn change_password(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Json(form): Json<NewPassword>,
) -> Result<Json<Session>, ApiError> {
    let worker = state.clone();
    let name = signed_in.user.name.clone();
    let result = blocking(move || {
        if worker.accounts.sign_in(&name, &form.old)?.is_none() {
            anyhow::bail!(WrongPassword);
        }
        worker.accounts.set_password(&name, &form.new)
    })
    .await?;
    match result {
        Ok(()) => {}
        Err(e) if e.is::<WrongPassword>() => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "wrong_password",
                "the current password is wrong",
            ));
        }
        Err(e) if e.is::<NoSuchUser>() => return Err(ApiError::unauthorized()),
        Err(e) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid",
                e.to_string(),
            ));
        }
    }
    let token = state
        .accounts
        .issue_token(&signed_in.user)
        .map_err(ApiError::internal)?;
    Ok(Json(Session {
        token,
        username: signed_in.user.name,
    }))
}

#[derive(Debug)]
struct WrongPassword;

impl std::fmt::Display for WrongPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("wrong password")
    }
}

impl std::error::Error for WrongPassword {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Me {
    username: String,
    used_bytes: u64,
    quota_bytes: u64,
}

async fn me(State(state): State<Shared>, signed_in: SignedIn) -> Result<Json<Me>, ApiError> {
    let root = state.files(&signed_in.user);
    let used = tokio::task::spawn_blocking(move || store::usage(&root))
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(Me {
        username: signed_in.user.name,
        used_bytes: used,
        quota_bytes: state.config.quota_bytes,
    }))
}

#[derive(Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
}

async fn list(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Query(q): Query<PathQuery>,
) -> Result<Json<Vec<store::Entry>>, ApiError> {
    Ok(Json(
        store::list(&state.files(&signed_in.user), &q.path).await?,
    ))
}

async fn mkdir(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Query(q): Query<PathQuery>,
) -> Result<StatusCode, ApiError> {
    store::mkdir(&state.files(&signed_in.user), &q.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn read(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Query(q): Query<PathQuery>,
) -> Result<Response, ApiError> {
    let path = store::resolve(&state.files(&signed_in.user), &q.path)?;
    if q.path.is_empty() || path.is_dir() {
        return Err(StoreError::BadPath.into());
    }
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(StoreError::from)?;
    let len = file.metadata().await.map_err(StoreError::from)?.len();
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            ),
            (header::CONTENT_LENGTH, HeaderValue::from(len)),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}

async fn write(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Query(q): Query<PathQuery>,
    body: Body,
) -> Result<StatusCode, ApiError> {
    store::write(
        &state.files(&signed_in.user),
        &q.path,
        body.into_data_stream(),
        &state.limits(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(state): State<Shared>,
    signed_in: SignedIn,
    Query(q): Query<PathQuery>,
) -> Result<StatusCode, ApiError> {
    store::remove(&state.files(&signed_in.user), &q.path).await?;
    Ok(StatusCode::NO_CONTENT)
}
