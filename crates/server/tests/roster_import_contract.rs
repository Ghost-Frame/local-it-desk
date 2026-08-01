//! Contract tests for bounded preview and atomic staff roster import.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use deadpool_sqlite::Pool;
use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::middleware::SESSION_COOKIE_NAME;
use local_it_desk_server::auth::session;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::error::AppError;
use local_it_desk_server::models::roster;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

/// Test browser material for an authenticated administrator request.
struct AdminSession {
    /// Opaque session cookie request pair.
    cookie: String,
    /// In-memory CSRF secret for state-changing requests.
    csrf: String,
}

/// Captured roster endpoint response fields.
struct TestResponse {
    /// HTTP status returned by the route.
    status: StatusCode,
    /// Parsed JSON response body.
    body: Value,
    /// Optional response cache policy.
    cache_control: Option<String>,
}

/// Builds one migrated application with an administrator and active session.
async fn test_app() -> (Router, Pool, AdminSession, TempDir) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    connection
        .interact(|connection| db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction")
        .expect("migration result");
    let administrator = db::interact(&pool, |connection| {
        user::create(
            connection,
            &NewUser {
                username: "teacher.admin",
                display_name: "Teacher Administrator",
                email: None,
                password: "correct horse battery staple",
                role: Role::Administrator,
                must_change_password: false,
            },
        )
    })
    .await
    .expect("administrator");
    let issued = db::interact(&pool, move |connection| {
        session::create(connection, administrator.id, 14)
    })
    .await
    .expect("administrator session");
    let browser = AdminSession {
        cookie: format!("{SESSION_COOKIE_NAME}={}", issued.token),
        csrf: issued.csrf_token,
    };
    (
        routes::build_router(config, pool.clone()),
        pool,
        browser,
        temp,
    )
}

/// Sends one raw CSV request with administrator browser credentials.
async fn send_csv(app: &Router, path: &str, csv: &str, session: &AdminSession) -> TestResponse {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "text/csv; charset=utf-8")
                .header("cookie", &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .body(Body::from(csv.to_string()))
                .expect("CSV request"),
        )
        .await
        .expect("router response");
    let status = response.status();
    let cache_control = response
        .headers()
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), 128 * 1024)
        .await
        .expect("response body");
    let body = serde_json::from_slice(&bytes).expect("JSON response");
    TestResponse {
        status,
        body,
        cache_control,
    }
}

/// Confirms valid rows normalize fields while preserving Unicode display names.
#[test]
fn parser_accepts_the_exact_contract_and_normalizes_rows() {
    let csv = b"username,display_name,role,email\n Teacher.One ,Ren\xC3\xA9e O'Connor,requester, STAFF@EXAMPLE.INVALID \nhelper.tech,Helper Tech,technician,\n";
    let preview = roster::parse(csv, 4096, 10).expect("valid CSV");

    assert!(preview.valid);
    assert!(preview.errors.is_empty());
    assert_eq!(preview.rows.len(), 2);
    assert_eq!(preview.rows[0].row_number, 2);
    assert_eq!(preview.rows[0].username, "teacher.one");
    assert_eq!(preview.rows[0].display_name, "Renée O'Connor");
    assert_eq!(
        preview.rows[0].email.as_deref(),
        Some("staff@example.invalid")
    );
    assert_eq!(preview.rows[1].email, None);
}

/// Confirms duplicate identities, roles, and formula prefixes produce safe row errors.
#[test]
fn parser_rejects_duplicates_unknown_roles_and_formula_prefixes() {
    let csv = b"username,display_name,role,email\nTeacher.One,First Teacher,requester,\nteacher.one,Second Teacher,unknown,\nformula.user,=HYPERLINK(1),requester,\n";
    let preview = roster::parse(csv, 4096, 10).expect("structured invalid preview");

    assert!(!preview.valid);
    assert!(preview.errors.iter().any(|error| {
        error.row_number == Some(3) && error.field.as_deref() == Some("username")
    }));
    assert!(
        preview
            .errors
            .iter()
            .any(|error| { error.row_number == Some(3) && error.field.as_deref() == Some("role") })
    );
    assert!(preview.errors.iter().any(|error| {
        error.row_number == Some(4) && error.field.as_deref() == Some("display_name")
    }));
    assert!(
        !preview
            .errors
            .iter()
            .any(|error| error.message.contains("HYPERLINK"))
    );
}

/// Confirms headers, column counts, byte limits, and row limits fail closed.
#[test]
fn parser_enforces_structure_and_configurable_limits() {
    let empty = roster::parse(b"username,display_name,role,email\n\n", 4096, 10)
        .expect("empty roster preview");
    assert!(!empty.valid);

    let wrong_header = roster::parse(b"name,display_name,role,email\na,b,requester,\n", 4096, 10)
        .expect("header preview");
    assert!(!wrong_header.valid);
    assert_eq!(wrong_header.errors[0].row_number, Some(1));

    let wrong_columns = roster::parse(
        b"username,display_name,role,email\none,One User,requester\n",
        4096,
        10,
    )
    .expect("column preview");
    assert!(!wrong_columns.valid);

    assert!(matches!(
        roster::parse(b"username,display_name,role,email\n", 8, 10),
        Err(AppError::PayloadTooLarge)
    ));
    let too_many_rows = roster::parse(
        b"username,display_name,role,email\none,One User,requester,\ntwo,Two User,requester,\n",
        4096,
        1,
    )
    .expect("row limit preview");
    assert!(!too_many_rows.valid);
    assert!(
        too_many_rows
            .errors
            .iter()
            .any(|error| { error.row_number == Some(3) && error.message.contains("row limit") })
    );
}

/// Confirms preview is non-mutating and apply returns one-time onboarding material.
#[tokio::test]
async fn preview_then_apply_is_explicit_and_plaintext_is_not_persisted() {
    let (app, pool, administrator, _temp) = test_app().await;
    let csv = "username,display_name,role,email\nstaff.one,Staff One,requester,staff.one@example.invalid\nhelper.tech,Helper Tech,technician,\n";
    let preview = send_csv(&app, "/api/admin/users/import/preview", csv, &administrator).await;
    assert_eq!(preview.status, StatusCode::OK);
    assert_eq!(preview.body["valid"], true);
    assert_eq!(preview.body["rows"].as_array().map(Vec::len), Some(2));
    let count_after_preview = db::interact(&pool, |connection| {
        connection
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, u64>(0))
            .map_err(Into::into)
    })
    .await
    .expect("preview count");
    assert_eq!(count_after_preview, 1);

    let applied = send_csv(&app, "/api/admin/users/import/apply", csv, &administrator).await;
    assert_eq!(applied.status, StatusCode::CREATED);
    assert_eq!(applied.cache_control.as_deref(), Some("no-store"));
    assert_eq!(applied.body["created"].as_array().map(Vec::len), Some(2));
    let temporary_passwords = applied.body["created"]
        .as_array()
        .expect("created accounts")
        .iter()
        .map(|entry| {
            entry["temporary_password"]
                .as_str()
                .expect("temporary password")
                .to_string()
        })
        .collect::<Vec<_>>();
    let leaked = db::interact(&pool, move |connection| {
        let mut total = 0_u64;
        for password in temporary_passwords {
            total += connection.query_row(
                "SELECT COUNT(*) FROM users WHERE password_hash = ?1",
                [&password],
                |row| row.get::<_, u64>(0),
            )?;
            total += connection.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE summary LIKE '%' || ?1 || '%'",
                [&password],
                |row| row.get::<_, u64>(0),
            )?;
        }
        Ok(total)
    })
    .await
    .expect("plaintext scan");
    assert_eq!(leaked, 0);
}

/// Confirms any invalid or conflicting row prevents every account insertion.
#[tokio::test]
async fn apply_is_all_or_none_for_validation_and_database_conflicts() {
    let (app, pool, administrator, _temp) = test_app().await;
    let invalid = "username,display_name,role,email\ngood.user,Good User,requester,\nbad.user,Bad User,not-a-role,\n";
    let response = send_csv(
        &app,
        "/api/admin/users/import/apply",
        invalid,
        &administrator,
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    let conflict = "username,display_name,role,email\nnew.user,New User,requester,\nteacher.admin,Duplicate Administrator,administrator,\n";
    let response = send_csv(
        &app,
        "/api/admin/users/import/apply",
        conflict,
        &administrator,
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let usernames = db::interact(&pool, |connection| {
        let mut statement = connection.prepare("SELECT username FROM users ORDER BY username")?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(values)
    })
    .await
    .expect("usernames");
    assert_eq!(usernames, vec!["teacher.admin"]);
}
