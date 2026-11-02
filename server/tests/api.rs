//! The API over real HTTP, as the app uses it.

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use aireader_server::Config;

struct Server {
    base: String,
    client: Client,
    _dir: tempfile::TempDir,
}

async fn start(open_registration: bool, quota_bytes: u64) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let app = aireader_server::app(Config {
        data: dir.path().to_path_buf(),
        open_registration,
        quota_bytes,
        max_upload_bytes: 1 << 20,
    })
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Server {
        base: format!("http://{addr}/api/v1"),
        client: Client::new(),
        _dir: dir,
    }
}

impl Server {
    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        let mut req = self.client.post(format!("{}{path}", self.base)).json(&body);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        let res = req.send().await.unwrap();
        let status = res.status();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    async fn register(&self, name: &str, password: &str) -> String {
        let (status, body) = self
            .post(
                "/register",
                None,
                json!({ "username": name, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["token"].as_str().unwrap().to_owned()
    }

    async fn put(&self, token: &str, path: &str, bytes: &'static [u8]) -> StatusCode {
        self.client
            .put(format!("{}/file", self.base))
            .query(&[("path", path)])
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn get(&self, token: &str, path: &str) -> (StatusCode, Vec<u8>) {
        let res = self
            .client
            .get(format!("{}/file", self.base))
            .query(&[("path", path)])
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (res.status(), res.bytes().await.unwrap().to_vec())
    }

    async fn list(&self, token: &str, path: &str) -> (StatusCode, Value) {
        let res = self
            .client
            .get(format!("{}/list", self.base))
            .query(&[("path", path)])
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (res.status(), res.json().await.unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn registration_is_closed_unless_opened() {
    let server = start(false, 0).await;
    let info: Value = server
        .client
        .get(format!("{}/info", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["server"], "aireader");
    assert_eq!(info["registration"], false);
    let (status, body) = server
        .post(
            "/register",
            None,
            json!({ "username": "eve", "password": "long enough" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "registration_closed");
}

#[tokio::test]
async fn files_round_trip_and_stay_private() {
    let server = start(true, 0).await;
    let alice = server.register("alice", "correct horse").await;
    let bob = server.register("bob", "battery staple").await;

    // What the sync code does: list a missing folder, make folders, write, read, list.
    assert_eq!(server.list(&alice, "devices").await.1, json!([]));
    let (status, _) = server
        .post("/mkdir?path=devices%2Fab12", Some(&alice), json!(null))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        server
            .put(&alice, "devices/ab12/library.json", b"{\"v\":1}")
            .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server.get(&alice, "devices/ab12/library.json").await,
        (StatusCode::OK, b"{\"v\":1}".to_vec())
    );
    let (_, listed) = server.list(&alice, "devices/ab12").await;
    assert_eq!(listed[0]["name"], "library.json");
    assert_eq!(listed[0]["dir"], false);
    let first_tag = listed[0]["tag"].clone();
    server
        .put(&alice, "devices/ab12/library.json", b"{\"v\":22}")
        .await;
    assert_ne!(
        server.list(&alice, "devices/ab12").await.1[0]["tag"],
        first_tag
    );

    // Another account sees none of it, and nobody gets out of their folder.
    assert_eq!(server.list(&bob, "devices").await.1, json!([]));
    assert_eq!(
        server.get(&bob, "devices/ab12/library.json").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        server.get(&bob, "../1/devices/ab12/library.json").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server.put(&bob, "../1/x", b"x").await,
        StatusCode::BAD_REQUEST
    );

    // Removing twice is fine; without a token nothing works.
    let delete = |token: &str| {
        server
            .client
            .delete(format!("{}/file", server.base))
            .query(&[("path", "devices/ab12/library.json")])
            .bearer_auth(token.to_owned())
            .send()
    };
    assert_eq!(
        delete(&alice).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        delete(&alice).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server.list("not-a-token", "").await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn wrong_passwords_slow_down_and_tokens_can_be_revoked() {
    let server = start(true, 0).await;
    server.register("alice", "correct horse").await;
    let wrong = json!({ "username": "alice", "password": "wrong horse" });
    for _ in 0..5 {
        assert_eq!(
            server.post("/login", None, wrong.clone()).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    // Locked for a while now, even with the right password.
    let right = json!({ "username": "Alice", "password": "correct horse" });
    let (status, body) = server.post("/login", None, right).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");

    let other = start(true, 0).await;
    let first = other.register("carol", "correct horse").await;
    let (status, session) = other
        .post(
            "/login",
            None,
            json!({ "username": "carol", "password": "correct horse" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let second = session["token"].as_str().unwrap().to_owned();

    // Logging out ends that token only.
    assert_eq!(
        other.post("/logout", Some(&second), json!(null)).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(other.list(&second, "").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(other.list(&first, "").await.0, StatusCode::OK);

    // A new password signs every device out and hands this one a new token.
    let (status, _) = other
        .post(
            "/password",
            Some(&first),
            json!({ "old": "nope nope", "new": "brand new pass" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, session) = other
        .post(
            "/password",
            Some(&first),
            json!({ "old": "correct horse", "new": "brand new pass" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(other.list(&first, "").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        other.list(session["token"].as_str().unwrap(), "").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn storage_limits_are_reported() {
    let server = start(true, 10).await;
    let token = server.register("alice", "correct horse").await;
    assert_eq!(
        server.put(&token, "a.json", b"0123456789").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server.put(&token, "b.json", b"x").await,
        StatusCode::INSUFFICIENT_STORAGE
    );
    let me: Value = server
        .client
        .get(format!("{}/me", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["usedBytes"], 10);
    assert_eq!(me["quotaBytes"], 10);
}
