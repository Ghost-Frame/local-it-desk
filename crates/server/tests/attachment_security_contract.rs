//! Security contract for attachment upload, authorization, storage, and download.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, HttpBody, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use deadpool_sqlite::Pool;
use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::middleware::SESSION_COOKIE_NAME;
use local_it_desk_server::auth::session;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// Browser authentication material for one named test account.
struct BrowserSession {
    /// Opaque session cookie request pair.
    cookie: String,
    /// Per-session CSRF request proof.
    csrf: String,
}

/// Captured response status, headers, and raw bytes.
struct TestResponse {
    /// HTTP response status.
    status: StatusCode,
    /// Complete response headers.
    headers: HeaderMap,
    /// Bounded response bytes.
    body: Vec<u8>,
    /// Upper response-body size hint captured before the body is consumed.
    body_size_hint_upper: Option<u64>,
}

/// Isolated attachment test application and its persisted fixture identities.
struct TestContext {
    /// Application router under test.
    app: Router,
    /// SQLite pool used for storage assertions.
    pool: Pool,
    /// Temporary runtime root retained for the test lifetime.
    _temp: TempDir,
    /// Administrator and technician identity.
    administrator: BrowserSession,
    /// Owning requester identity.
    requester: BrowserSession,
    /// Unrelated requester identity.
    stranger: BrowserSession,
    /// Ticket that owns attachment fixtures.
    ticket_id: Uuid,
    /// Requester-visible comment fixture.
    public_comment_id: Uuid,
    /// Staff-only note fixture.
    internal_comment_id: Uuid,
}

/// Builds one migrated app with configurable per-file and per-ticket limits.
async fn test_app(max_upload_bytes: u64, max_ticket_upload_bytes: u64) -> TestContext {
    let temp = tempfile::tempdir().expect("temporary directory");
    let mut config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    config.max_upload_bytes = max_upload_bytes;
    config.max_ticket_upload_bytes = max_ticket_upload_bytes;
    config
        .prepare_runtime_directories()
        .expect("runtime directories");
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let (administrator, requester, stranger, ticket_id, public_comment_id, internal_comment_id) =
        connection
            .interact(|connection| {
                db::migrations::run_migrations(connection)?;
                let administrator = create_account(connection, "desk.admin", Role::Administrator)?;
                let requester = create_account(connection, "casey", Role::Requester)?;
                let stranger = create_account(connection, "morgan", Role::Requester)?;
                let category_id = Uuid::new_v4();
                let ticket_id = Uuid::new_v4();
                let public_comment_id = Uuid::new_v4();
                let internal_comment_id = Uuid::new_v4();
                let now = "2026-01-01T00:00:00.000Z";
                connection.execute(
                    "INSERT INTO categories (
                         id, name, description, is_active, sort_order, created_at, updated_at
                     ) VALUES (?1, 'Classroom technology', NULL, 1, 0, ?2, ?2)",
                    rusqlite::params![category_id.to_string(), now],
                )?;
                connection.execute(
                    "INSERT INTO tickets (
                         id, number, title, description, requester_id, assignee_id,
                         category_id, status, priority, created_at, updated_at
                     ) VALUES (?1, 1, 'Projector issue', 'The image is distorted.',
                               ?2, ?3, ?4, 'open', 'normal', ?5, ?5)",
                    rusqlite::params![
                        ticket_id.to_string(),
                        requester.to_string(),
                        administrator.to_string(),
                        category_id.to_string(),
                        now,
                    ],
                )?;
                for (id, visibility) in [
                    (public_comment_id, "public"),
                    (internal_comment_id, "internal"),
                ] {
                    connection.execute(
                        "INSERT INTO ticket_comments (
                             id, ticket_id, author_id, body, visibility, created_at, updated_at
                         ) VALUES (?1, ?2, ?3, 'Fixture comment', ?4, ?5, ?5)",
                        rusqlite::params![
                            id.to_string(),
                            ticket_id.to_string(),
                            administrator.to_string(),
                            visibility,
                            now,
                        ],
                    )?;
                }
                Ok::<_, local_it_desk_server::error::AppError>((
                    administrator,
                    requester,
                    stranger,
                    ticket_id,
                    public_comment_id,
                    internal_comment_id,
                ))
            })
            .await
            .expect("fixture interaction")
            .expect("fixture result");
    let administrator = issue_session(&pool, administrator).await;
    let requester = issue_session(&pool, requester).await;
    let stranger = issue_session(&pool, stranger).await;
    TestContext {
        app: routes::build_router(config, pool.clone()),
        pool,
        _temp: temp,
        administrator,
        requester,
        stranger,
        ticket_id,
        public_comment_id,
        internal_comment_id,
    }
}

/// Creates one active account that can immediately use product routes.
fn create_account(
    connection: &rusqlite::Connection,
    username: &str,
    role: Role,
) -> local_it_desk_server::error::AppResult<Uuid> {
    Ok(user::create(
        connection,
        &NewUser {
            username,
            display_name: username,
            email: None,
            password: "fixture password value",
            role,
            must_change_password: false,
        },
    )?
    .id)
}

/// Issues one server-side browser session for a fixture account.
async fn issue_session(pool: &Pool, user_id: Uuid) -> BrowserSession {
    let issued = db::interact(pool, move |connection| {
        session::create(connection, user_id, 14)
    })
    .await
    .expect("session");
    BrowserSession {
        cookie: format!("{SESSION_COOKIE_NAME}={}", issued.token),
        csrf: issued.csrf_token,
    }
}

/// Returns a deterministic valid one-pixel PNG fixture.
fn png_bytes() -> Vec<u8> {
    STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("static PNG")
}

/// Encodes one attachment request as a multipart form body.
fn multipart_body(
    parent_kind: &str,
    parent_id: Uuid,
    filename: &str,
    bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = "local-it-desk-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"parent_kind\"\r\n\r\n{parent_kind}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"parent_id\"\r\n\r\n{parent_id}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

/// Sends one authenticated request and captures its bounded raw response.
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
    session: &BrowserSession,
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", &session.cookie)
        .header("x-csrf-token", &session.csrf)
        .extension(ConnectInfo(
            "192.0.2.61:42000"
                .parse::<SocketAddr>()
                .expect("static peer"),
        ));
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body_size_hint_upper = response.body().size_hint().upper();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("bounded body")
        .to_vec();
    TestResponse {
        status,
        headers,
        body,
        body_size_hint_upper,
    }
}

/// Uploads one attachment through the public multipart contract.
async fn upload(
    context: &TestContext,
    session: &BrowserSession,
    parent_kind: &str,
    parent_id: Uuid,
    filename: &str,
    bytes: &[u8],
) -> TestResponse {
    let (boundary, body) = multipart_body(parent_kind, parent_id, filename, bytes);
    send(
        &context.app,
        "POST",
        "/api/attachments",
        Some(&format!("multipart/form-data; boundary={boundary}")),
        body,
        session,
    )
    .await
}

/// Parses one response body as JSON.
fn json_body(response: &TestResponse) -> Value {
    serde_json::from_slice(&response.body).expect("JSON response")
}

/// Confirms safe randomized storage, checksums, download headers, and ownership.
#[tokio::test]
async fn stores_randomized_verified_files_and_enforces_download_ownership() {
    let context = test_app(1024 * 1024, 4 * 1024 * 1024).await;
    let png = png_bytes();
    let uploaded = upload(
        &context,
        &context.requester,
        "ticket",
        context.ticket_id,
        "projector.png",
        &png,
    )
    .await;
    assert_eq!(uploaded.status, StatusCode::CREATED);
    let attachment_id = json_body(&uploaded)["id"]
        .as_str()
        .expect("attachment id")
        .to_owned();

    let (stored_name, checksum, size) = db::interact(&context.pool, move |connection| {
        connection
            .query_row(
                "SELECT stored_name, sha256, size_bytes FROM attachments WHERE id = ?1",
                [&attachment_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                    ))
                },
            )
            .map_err(Into::into)
    })
    .await
    .expect("attachment metadata");
    assert!(!stored_name.contains("projector"));
    assert_eq!(checksum.len(), 64);
    assert_eq!(size, png.len() as u64);

    let owner_download = send(
        &context.app,
        "GET",
        &format!(
            "/api/attachments/{}",
            json_body(&uploaded)["id"].as_str().expect("id")
        ),
        None,
        Vec::new(),
        &context.requester,
    )
    .await;
    assert_eq!(owner_download.status, StatusCode::OK);
    assert_eq!(owner_download.body, png);
    assert_eq!(owner_download.body_size_hint_upper, None);
    assert_eq!(
        owner_download.headers["content-length"],
        owner_download.body.len().to_string()
    );
    assert_eq!(owner_download.headers["x-content-type-options"], "nosniff");
    assert!(
        owner_download.headers["content-disposition"]
            .to_str()
            .expect("disposition")
            .starts_with("attachment;")
    );
    assert!(
        owner_download.headers["cache-control"]
            .to_str()
            .expect("cache policy")
            .contains("private")
    );

    let stranger_download = send(
        &context.app,
        "GET",
        &format!(
            "/api/attachments/{}",
            json_body(&uploaded)["id"].as_str().expect("id")
        ),
        None,
        Vec::new(),
        &context.stranger,
    )
    .await;
    assert_eq!(stranger_download.status, StatusCode::NOT_FOUND);
}

/// Confirms active, executable, mismatched, and traversal-shaped uploads are rejected.
#[tokio::test]
async fn rejects_unsafe_content_names_and_extension_mismatches() {
    let context = test_app(1024 * 1024, 4 * 1024 * 1024).await;
    for (filename, bytes, expected) in [
        (
            "payload.svg",
            b"<svg><script>alert(1)</script></svg>".to_vec(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            "payload.exe",
            b"MZ executable".to_vec(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        ("wrong.jpg", png_bytes(), StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("../projector.png", png_bytes(), StatusCode::BAD_REQUEST),
    ] {
        let response = upload(
            &context,
            &context.requester,
            "ticket",
            context.ticket_id,
            filename,
            &bytes,
        )
        .await;
        assert_eq!(response.status, expected, "{filename}");
    }
    let entries = std::fs::read_dir(context.app_state_upload_dir()).expect("upload directory");
    assert_eq!(entries.count(), 0);
}

/// Confirms internal-note files remain staff-only in list and download routes.
#[tokio::test]
async fn internal_note_attachments_never_cross_the_requester_boundary() {
    let context = test_app(1024 * 1024, 4 * 1024 * 1024).await;
    let public_upload = upload(
        &context,
        &context.requester,
        "public_comment",
        context.public_comment_id,
        "requester.png",
        &png_bytes(),
    )
    .await;
    assert_eq!(public_upload.status, StatusCode::CREATED);
    let uploaded = upload(
        &context,
        &context.administrator,
        "internal_note",
        context.internal_comment_id,
        "controller.png",
        &png_bytes(),
    )
    .await;
    assert_eq!(uploaded.status, StatusCode::CREATED);
    let attachment_id = json_body(&uploaded)["id"]
        .as_str()
        .expect("attachment id")
        .to_owned();

    let requester_list = send(
        &context.app,
        "GET",
        &format!("/api/tickets/{}/attachments", context.ticket_id),
        None,
        Vec::new(),
        &context.requester,
    )
    .await;
    let staff_list = send(
        &context.app,
        "GET",
        &format!("/api/tickets/{}/attachments", context.ticket_id),
        None,
        Vec::new(),
        &context.administrator,
    )
    .await;
    assert_eq!(
        json_body(&requester_list).as_array().expect("list").len(),
        1
    );
    assert_eq!(json_body(&staff_list).as_array().expect("list").len(), 2);
    assert_eq!(
        send(
            &context.app,
            "GET",
            &format!("/api/attachments/{attachment_id}"),
            None,
            Vec::new(),
            &context.requester,
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

/// Confirms per-file and aggregate limits leave no partial files or metadata.
#[tokio::test]
async fn size_failures_and_database_failures_clean_partial_storage() {
    let png = png_bytes();
    let small_context = test_app((png.len() - 1) as u64, 4 * 1024 * 1024).await;
    let oversized = upload(
        &small_context,
        &small_context.requester,
        "ticket",
        small_context.ticket_id,
        "large.png",
        &png,
    )
    .await;
    assert_eq!(oversized.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        std::fs::read_dir(small_context.app_state_upload_dir())
            .expect("upload directory")
            .count(),
        0
    );

    let aggregate_context = test_app(1024 * 1024, png.len() as u64 + 1).await;
    assert_eq!(
        upload(
            &aggregate_context,
            &aggregate_context.requester,
            "ticket",
            aggregate_context.ticket_id,
            "first.png",
            &png,
        )
        .await
        .status,
        StatusCode::CREATED
    );
    assert_eq!(
        upload(
            &aggregate_context,
            &aggregate_context.requester,
            "ticket",
            aggregate_context.ticket_id,
            "second.png",
            &png,
        )
        .await
        .status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let persisted = db::interact(&aggregate_context.pool, |connection| {
        connection
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| {
                row.get::<_, u64>(0)
            })
            .map_err(Into::into)
    })
    .await
    .expect("attachment count");
    assert_eq!(persisted, 1);
    assert_eq!(
        std::fs::read_dir(aggregate_context.app_state_upload_dir())
            .expect("upload directory")
            .count(),
        1
    );

    let failed_context = test_app(1024 * 1024, 4 * 1024 * 1024).await;
    db::interact(&failed_context.pool, |connection| {
        connection.execute_batch(
            "CREATE TRIGGER reject_attachment_fixture
             BEFORE INSERT ON attachments
             BEGIN
                 SELECT RAISE(ABORT, 'forced attachment insert failure');
             END;",
        )?;
        Ok(())
    })
    .await
    .expect("failure trigger");
    let failed = upload(
        &failed_context,
        &failed_context.requester,
        "ticket",
        failed_context.ticket_id,
        "database-failure.png",
        &png,
    )
    .await;
    assert_eq!(failed.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        std::fs::read_dir(failed_context.app_state_upload_dir())
            .expect("upload directory")
            .count(),
        0
    );
    let failed_rows = db::interact(&failed_context.pool, |connection| {
        connection
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| {
                row.get::<_, u64>(0)
            })
            .map_err(Into::into)
    })
    .await
    .expect("failed attachment count");
    assert_eq!(failed_rows, 0);
}

/// Test-only runtime path access retained by the temporary fixture.
impl TestContext {
    /// Returns the attachment directory beneath this fixture's runtime root.
    fn app_state_upload_dir(&self) -> std::path::PathBuf {
        self._temp.path().join("uploads")
    }
}
