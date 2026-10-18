//! End-to-end HTTP tests: the whole surface the app and the browser UI use.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use little_log_server::build_router;
use little_log_server::state::{AppState, Config};
use serde_json::{json, Value};
use tower::ServiceExt;

/* --------------------------------------------------------------- test rig */

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestServer {
    app: Router,
    dir: PathBuf,
}

impl TestServer {
    fn new() -> Self {
        let unique = format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(format!("little-log-api-{unique}"));
        let web = dir.join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("index.html"), "<!doctype html><ll-app></ll-app>").unwrap();
        std::fs::write(web.join("app.js"), "console.log('little log')").unwrap();

        let state = AppState::new(Config {
            session_secret: "test-secret-that-is-long-enough".to_string(),
            cookie_secure: false,
            data_dir: dir.clone(),
            web_dir: web,
        })
        .unwrap();

        Self {
            app: build_router(state),
            dir,
        }
    }

    async fn call(&self, request: Request<Body>) -> Response {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or("").to_string());
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();

        Response {
            status,
            content_type,
            cookie,
            set_cookie,
            body: bytes.to_vec(),
        }
    }

    /// Register the first account and return its session cookie.
    async fn sign_up_owner(&self) -> String {
        let response = self
            .call(post_json(
                "/api/auth/register",
                json!({ "name": "Kwabena", "email": "kwabena@example.com", "password": "hunter2hunter2" }),
                None,
            ))
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        response.cookie.expect("a session cookie")
    }

    /// Register an invited account and return its session cookie.
    async fn sign_up_invited(&self, owner: &str, role: &str, email: &str) -> String {
        let invite = self
            .call(post_json(
                "/api/invites",
                json!({ "role": role }),
                Some(owner),
            ))
            .await;
        assert_eq!(invite.status, StatusCode::OK, "{}", invite.text());
        let token = invite.json()["token"].as_str().unwrap().to_string();

        let response = self
            .call(post_json(
                "/api/auth/register",
                json!({
                    "name": "Invitee",
                    "email": email,
                    "password": "hunter2hunter2",
                    "invite": token,
                }),
                None,
            ))
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        response.cookie.expect("a session cookie")
    }

    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

struct Response {
    status: StatusCode,
    content_type: String,
    /// `ll_session=…` from `Set-Cookie`, ready to send back.
    cookie: Option<String>,
    /// The raw `Set-Cookie` header, for attribute assertions.
    set_cookie: Option<String>,
    body: Vec<u8>,
}

impl Response {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

fn post_json(uri: &str, body: Value, cookie: Option<&str>) -> Request<Body> {
    with_cookie(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json"),
        cookie,
    )
    .body(Body::from(body.to_string()))
    .unwrap()
}

fn put_json(uri: &str, body: Value, cookie: Option<&str>) -> Request<Body> {
    with_cookie(
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json"),
        cookie,
    )
    .body(Body::from(body.to_string()))
    .unwrap()
}

fn put_bytes(uri: &str, bytes: Vec<u8>, cookie: Option<&str>) -> Request<Body> {
    with_cookie(
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header(header::CONTENT_TYPE, "image/png"),
        cookie,
    )
    .body(Body::from(bytes))
    .unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    with_cookie(Request::builder().method("GET").uri(uri), cookie)
        .body(Body::empty())
        .unwrap()
}

fn delete(uri: &str, cookie: Option<&str>) -> Request<Body> {
    with_cookie(Request::builder().method("DELETE").uri(uri), cookie)
        .body(Body::empty())
        .unwrap()
}

fn with_cookie(
    builder: axum::http::request::Builder,
    cookie: Option<&str>,
) -> axum::http::request::Builder {
    match cookie {
        Some(cookie) => builder.header(header::COOKIE, cookie),
        None => builder,
    }
}

fn an_entry(kind: &str) -> Value {
    json!({
        "id": little_log_core::model::new_id(),
        "kind": kind,
        "occurred_at": little_log_core::validate::now_millis(),
        "ended_at": null,
        "feed_kind": if kind == "feed" { json!("bottle") } else { Value::Null },
        "amount": 90,
        "amount_unit": "ml",
        "diaper_type": if kind == "diaper" { json!("pee") } else { Value::Null },
        "created_at": little_log_core::validate::now_millis(),
        "updated_at": little_log_core::validate::now_millis(),
        "deleted": 0
    })
}

/* ------------------------------------------------------------------- auth */

#[tokio::test]
async fn status_reports_needs_setup_until_the_first_account() {
    let server = TestServer::new();

    let before = server.call(get("/api/auth/status", None)).await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(before.json()["needs_setup"], json!(true));

    server.sign_up_owner().await;

    let after = server.call(get("/api/auth/status", None)).await;
    assert_eq!(after.json()["needs_setup"], json!(false));
    server.cleanup();
}

#[tokio::test]
async fn the_first_account_becomes_the_owner_with_an_httponly_cookie() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;

    let me = server.call(get("/api/auth/me", Some(&cookie))).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["user"]["role"], json!("owner"));
    assert_eq!(me.json()["baby"]["name"], json!("Baby"));
    // The password hash never leaves the server.
    assert!(me.json()["user"].get("password_hash").is_none());

    let set_cookie = server
        .call(post_json(
            "/api/auth/login",
            json!({ "email": "kwabena@example.com", "password": "hunter2hunter2" }),
            None,
        ))
        .await
        .set_cookie
        .unwrap();
    assert!(set_cookie.contains("ll_session="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("Path=/"));
    server.cleanup();
}

#[tokio::test]
async fn a_second_account_needs_an_invite() {
    let server = TestServer::new();
    server.sign_up_owner().await;

    let response = server
        .call(post_json(
            "/api/auth/register",
            json!({ "name": "Stranger", "email": "stranger@example.com", "password": "hunter2hunter2" }),
            None,
        ))
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert!(response.text().contains("invite"));
    server.cleanup();
}

#[tokio::test]
async fn an_invited_caregiver_joins_with_that_role_and_links_are_single_use() {
    let server = TestServer::new();
    let owner = server.sign_up_owner().await;

    let invite = server
        .call(post_json(
            "/api/invites",
            json!({ "role": "caregiver" }),
            Some(&owner),
        ))
        .await;
    let token = invite.json()["token"].as_str().unwrap().to_string();
    assert_eq!(invite.json()["role"], json!("caregiver"));

    let joined = server
        .call(post_json(
            "/api/auth/register",
            json!({ "name": "Grandma", "email": "grandma@example.com", "password": "hunter2hunter2", "invite": token }),
            None,
        ))
        .await;
    assert_eq!(joined.status, StatusCode::OK, "{}", joined.text());
    assert_eq!(joined.json()["user"]["role"], json!("caregiver"));

    // The same link cannot be used twice.
    let reused = server
        .call(post_json(
            "/api/auth/register",
            json!({ "name": "Someone", "email": "someone@example.com", "password": "hunter2hunter2", "invite": token }),
            None,
        ))
        .await;
    assert_eq!(reused.status, StatusCode::BAD_REQUEST);
    assert!(reused.text().contains("already been used"));

    let co_parent = server
        .sign_up_invited(&owner, "co_parent", "partner@example.com")
        .await;
    let me = server.call(get("/api/auth/me", Some(&co_parent))).await;
    assert_eq!(me.json()["user"]["role"], json!("co_parent"));
    server.cleanup();
}

#[tokio::test]
async fn signing_in_and_out_manages_the_session() {
    let server = TestServer::new();
    server.sign_up_owner().await;

    let wrong = server
        .call(post_json(
            "/api/auth/login",
            json!({ "email": "kwabena@example.com", "password": "not-the-password" }),
            None,
        ))
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json()["error"], json!("email or password is wrong"));

    let unknown = server
        .call(post_json(
            "/api/auth/login",
            json!({ "email": "nobody@example.com", "password": "hunter2hunter2" }),
            None,
        ))
        .await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);

    let login = server
        .call(post_json(
            "/api/auth/login",
            json!({ "email": "Kwabena@Example.com  ", "password": "hunter2hunter2" }),
            None,
        ))
        .await;
    assert_eq!(
        login.status,
        StatusCode::OK,
        "emails are trimmed and lower-cased"
    );
    let cookie = login.cookie.unwrap();

    assert_eq!(
        server.call(get("/api/auth/me", Some(&cookie))).await.status,
        StatusCode::OK
    );

    let logout = server
        .call(post_json("/api/auth/logout", json!({}), Some(&cookie)))
        .await;
    assert_eq!(logout.status, StatusCode::OK);

    assert_eq!(
        server.call(get("/api/auth/me", Some(&cookie))).await.status,
        StatusCode::UNAUTHORIZED,
        "the session is revoked server-side"
    );
    server.cleanup();
}

#[tokio::test]
async fn unauthenticated_requests_are_401_with_a_json_error() {
    let server = TestServer::new();

    for uri in [
        "/api/entries",
        "/api/baby",
        "/api/caregivers",
        "/api/export.csv",
    ] {
        let response = server.call(get(uri, None)).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert!(response.json()["error"].is_string(), "{uri}");
    }

    let post = server
        .call(post_json("/api/entries", an_entry("feed"), None))
        .await;
    assert_eq!(post.status, StatusCode::UNAUTHORIZED);
    server.cleanup();
}

#[tokio::test]
async fn failed_sign_ins_are_throttled() {
    let server = TestServer::new();
    server.sign_up_owner().await;

    let mut throttled = false;
    for _ in 0..12 {
        let response = server
            .call(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "203.0.113.7")
                    .body(Body::from(
                        json!({ "email": "kwabena@example.com", "password": "nope" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        if response.status == StatusCode::TOO_MANY_REQUESTS {
            throttled = true;
            break;
        }
    }
    assert!(throttled, "repeated failures must be rate limited");
    server.cleanup();
}

/* ---------------------------------------------------------------- entries */

#[tokio::test]
async fn entries_round_trip_newest_first() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;

    let now = little_log_core::validate::now_millis();
    let mut older = an_entry("feed");
    older["occurred_at"] = json!(now - 120_000);
    let created = server
        .call(post_json("/api/entries", older, Some(&cookie)))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    assert!(
        created.json()["baby_id"].is_string(),
        "the server fills the baby"
    );
    assert!(created.json()["caregiver_id"].is_string());

    let mut newer = an_entry("diaper");
    newer["occurred_at"] = json!(now - 60_000);
    server
        .call(post_json("/api/entries", newer, Some(&cookie)))
        .await;

    let list = server.call(get("/api/entries", Some(&cookie))).await;
    let entries = list.json();
    assert_eq!(entries.as_array().unwrap().len(), 2);
    assert_eq!(entries[0]["kind"], json!("diaper"), "newest first");
    server.cleanup();
}

#[tokio::test]
async fn a_caregiver_can_add_and_edit_their_own_rows_but_not_other_peoples() {
    let server = TestServer::new();
    let owner = server.sign_up_owner().await;
    let caregiver = server
        .sign_up_invited(&owner, "caregiver", "grandma@example.com")
        .await;

    let owners_entry = server
        .call(post_json("/api/entries", an_entry("feed"), Some(&owner)))
        .await
        .json();
    let id = owners_entry["id"].as_str().unwrap().to_string();

    // Editing someone else's entry is refused…
    let mut edited = owners_entry.clone();
    edited["amount"] = json!(130);
    edited["updated_at"] = json!(little_log_core::validate::now_millis() + 1000);
    let refused = server
        .call(post_json("/api/entries", edited, Some(&caregiver)))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.text().contains("cannot change"));

    // …and so is deleting it.
    let refused_delete = server
        .call(delete(&format!("/api/entries/{id}"), Some(&caregiver)))
        .await;
    assert_eq!(refused_delete.status, StatusCode::FORBIDDEN);

    // But they can add their own and edit it afterwards.
    let own = server
        .call(post_json(
            "/api/entries",
            an_entry("diaper"),
            Some(&caregiver),
        ))
        .await
        .json();
    let mut own_edited = own.clone();
    own_edited["diaper_type"] = json!("both");
    own_edited["updated_at"] = json!(little_log_core::validate::now_millis() + 1000);
    let updated = server
        .call(post_json("/api/entries", own_edited, Some(&caregiver)))
        .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.text());
    assert_eq!(updated.json()["diaper_type"], json!("both"));

    // A caregiver cannot invite people either.
    let invite = server
        .call(post_json(
            "/api/invites",
            json!({ "role": "caregiver" }),
            Some(&caregiver),
        ))
        .await;
    assert_eq!(invite.status, StatusCode::FORBIDDEN);
    server.cleanup();
}

#[tokio::test]
async fn a_co_parent_can_edit_anyone_and_delete_entries() {
    let server = TestServer::new();
    let owner = server.sign_up_owner().await;
    let co_parent = server
        .sign_up_invited(&owner, "co_parent", "partner@example.com")
        .await;

    let entry = server
        .call(post_json("/api/entries", an_entry("feed"), Some(&owner)))
        .await
        .json();
    let id = entry["id"].as_str().unwrap().to_string();

    let mut edited = entry.clone();
    edited["note"] = json!("edited by the co-parent");
    edited["updated_at"] = json!(little_log_core::validate::now_millis() + 1000);
    let updated = server
        .call(post_json("/api/entries", edited, Some(&co_parent)))
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.json()["note"], json!("edited by the co-parent"));

    let deleted = server
        .call(delete(&format!("/api/entries/{id}"), Some(&co_parent)))
        .await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(deleted.json()["deleted"], json!(1));

    let list = server.call(get("/api/entries", Some(&owner))).await;
    assert_eq!(
        list.json().as_array().unwrap().len(),
        0,
        "tombstones are hidden"
    );
    server.cleanup();
}

#[tokio::test]
async fn delete_everything_is_owner_only() {
    let server = TestServer::new();
    let owner = server.sign_up_owner().await;
    let caregiver = server
        .sign_up_invited(&owner, "caregiver", "grandma@example.com")
        .await;

    server
        .call(post_json("/api/entries", an_entry("feed"), Some(&owner)))
        .await;

    let refused = server.call(delete("/api/data", Some(&caregiver))).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let done = server.call(delete("/api/data", Some(&owner))).await;
    assert_eq!(done.status, StatusCode::NO_CONTENT);

    let list = server.call(get("/api/entries", Some(&owner))).await;
    assert_eq!(list.json().as_array().unwrap().len(), 0);

    // The tombstones are still there for other devices.
    let sync = server.call(get("/api/sync", Some(&owner))).await;
    assert_eq!(sync.json()["entries"].as_array().unwrap().len(), 1);
    assert_eq!(sync.json()["entries"][0]["deleted"], json!(1));
    server.cleanup();
}

#[tokio::test]
async fn csv_export_uses_the_prototype_columns() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;

    let mut sleep = an_entry("sleep");
    sleep["feed_kind"] = Value::Null;
    sleep["amount"] = Value::Null;
    sleep["amount_unit"] = Value::Null;
    sleep["diaper_type"] = Value::Null;
    sleep["occurred_at"] = json!(1_700_000_000_000i64);
    sleep["ended_at"] = json!(1_700_000_000_000i64 + 80 * 60_000);
    server
        .call(post_json("/api/entries", sleep, Some(&cookie)))
        .await;

    let csv = server.call(get("/api/export.csv", Some(&cookie))).await;
    assert_eq!(csv.status, StatusCode::OK);
    assert!(csv.content_type.starts_with("text/csv"));
    let body = csv.text();
    let mut lines = body.lines();
    assert_eq!(
        lines.next().unwrap(),
        "\"time\",\"type\",\"what\",\"details\",\"notes\""
    );
    let row = lines.next().unwrap();
    assert!(row.contains("\"sleep\""));
    assert!(row.contains("\"1h 20m\""), "row was {row}");
    server.cleanup();
}

/* ------------------------------------------------------------------- sync */

#[tokio::test]
async fn sync_push_is_idempotent_and_pull_only_returns_changes() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;
    let entry = an_entry("feed");

    let first = server
        .call(post_json(
            "/api/sync",
            json!({ "entries": [entry] }),
            Some(&cookie),
        ))
        .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(first.json()["applied"], json!(1));
    assert!(first.json()["cursor"].is_string());

    // Push the identical row again: nothing changes.
    let again = server
        .call(post_json(
            "/api/sync",
            json!({ "entries": [entry] }),
            Some(&cookie),
        ))
        .await;
    assert_eq!(again.json()["applied"], json!(0));
    assert_eq!(again.json()["skipped"], json!(1));

    let cursor = first.json()["cursor"].as_str().unwrap().to_string();
    let pull = server
        .call(get(&format!("/api/sync?since={cursor}"), Some(&cookie)))
        .await;
    assert_eq!(pull.status, StatusCode::OK);
    assert_eq!(pull.json()["entries"].as_array().unwrap().len(), 0);
    assert_eq!(pull.json()["baby"]["name"], json!("Baby"));

    // A newer edit shows up.
    let mut edited = entry.clone();
    edited["note"] = json!("a bit grizzly");
    edited["updated_at"] = json!(little_log_core::validate::now_millis() + 5000);
    server
        .call(post_json(
            "/api/sync",
            json!({ "entries": [edited] }),
            Some(&cookie),
        ))
        .await;

    let pull = server
        .call(get(&format!("/api/sync?since={cursor}"), Some(&cookie)))
        .await;
    let entries = pull.json()["entries"].clone();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["note"], json!("a bit grizzly"));

    let bad_cursor = server
        .call(get("/api/sync?since=nonsense", Some(&cookie)))
        .await;
    assert_eq!(bad_cursor.status, StatusCode::BAD_REQUEST);
    server.cleanup();
}

#[tokio::test]
async fn a_stale_push_never_wins() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;

    let mut entry = an_entry("feed");
    entry["note"] = json!("fresh");
    entry["updated_at"] = json!(1_800_000_000_000i64);
    server
        .call(post_json(
            "/api/sync",
            json!({ "entries": [entry.clone()] }),
            Some(&cookie),
        ))
        .await;

    let mut stale = entry.clone();
    stale["note"] = json!("stale");
    stale["updated_at"] = json!(1_700_000_000_000i64);
    let pushed = server
        .call(post_json(
            "/api/sync",
            json!({ "entries": [stale] }),
            Some(&cookie),
        ))
        .await;
    assert_eq!(pushed.json()["applied"], json!(0));
    assert_eq!(pushed.json()["skipped"], json!(1));

    let list = server.call(get("/api/entries", Some(&cookie))).await;
    assert_eq!(list.json()[0]["note"], json!("fresh"));
    server.cleanup();
}

/* ------------------------------------------------------------------ baby */

#[tokio::test]
async fn the_baby_profile_can_be_edited_by_a_co_parent_only() {
    let server = TestServer::new();
    let owner = server.sign_up_owner().await;
    let caregiver = server
        .sign_up_invited(&owner, "caregiver", "grandma@example.com")
        .await;

    let updated = server
        .call(put_json(
            "/api/baby",
            json!({ "name": "Ada", "born": "2026-03-02", "theme": "dark" }),
            Some(&owner),
        ))
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.json()["name"], json!("Ada"));

    let refused = server
        .call(put_json(
            "/api/baby",
            json!({ "name": "Nope", "born": "", "theme": "auto" }),
            Some(&caregiver),
        ))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let baby = server.call(get("/api/baby", Some(&caregiver))).await;
    assert_eq!(baby.json()["name"], json!("Ada"), "but they can read it");

    let caregivers = server.call(get("/api/caregivers", Some(&owner))).await;
    assert_eq!(caregivers.json().as_array().unwrap().len(), 2);
    server.cleanup();
}

/* ---------------------------------------------------------------- photos */

#[tokio::test]
async fn photos_are_resized_on_upload_and_served_behind_the_cookie() {
    let server = TestServer::new();
    let cookie = server.sign_up_owner().await;
    let entry_id = little_log_core::model::new_id();

    let png = big_png(2000, 1200);
    let upload = server
        .call(put_bytes(
            &format!("/api/photos/{entry_id}"),
            png,
            Some(&cookie),
        ))
        .await;
    assert_eq!(upload.status, StatusCode::OK, "{}", upload.text());
    assert_eq!(upload.json()["photo"], json!(format!("{entry_id}.jpg")));

    let anonymous = server
        .call(get(&format!("/api/photos/{entry_id}"), None))
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    let fetch = server
        .call(get(&format!("/api/photos/{entry_id}"), Some(&cookie)))
        .await;
    assert_eq!(fetch.status, StatusCode::OK);
    assert_eq!(fetch.content_type, "image/jpeg");
    assert_eq!(&fetch.body[..2], &[0xff, 0xd8], "re-encoded as JPEG");

    let missing = server
        .call(get(
            &format!("/api/photos/{}", little_log_core::model::new_id()),
            Some(&cookie),
        ))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let bad_id = server
        .call(put_bytes(
            "/api/photos/not-a-uuid",
            vec![1, 2, 3],
            Some(&cookie),
        ))
        .await;
    assert_eq!(bad_id.status, StatusCode::BAD_REQUEST);
    server.cleanup();
}

/* -------------------------------------------------------------- the web UI */

#[tokio::test]
async fn the_health_probe_answers_without_a_session() {
    let server = TestServer::new();

    let health = server.call(get("/healthz", None)).await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(health.json()["status"], json!("ok"));
    assert_eq!(health.json()["entries"], json!(0));
    server.cleanup();
}

#[tokio::test]
async fn the_web_app_is_served_with_a_spa_fallback() {
    let server = TestServer::new();

    let index = server.call(get("/", None)).await;
    assert_eq!(index.status, StatusCode::OK);
    assert!(index.text().contains("<ll-app>"));

    let asset = server.call(get("/app.js", None)).await;
    assert_eq!(asset.status, StatusCode::OK);
    assert!(asset.text().contains("little log"));

    // Deep links fall back to index.html so the SPA router can handle them.
    let deep = server.call(get("/?invite=abc", None)).await;
    assert_eq!(deep.status, StatusCode::OK);
    assert!(deep.text().contains("<ll-app>"));
    server.cleanup();
}

/// A real PNG, big enough to need downscaling.
fn big_png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 160])
    });
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}
