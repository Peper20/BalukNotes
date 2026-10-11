//! The sync API: vaults, files, versions, long polling.

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use notes_store::sync::{Changes, Conflict, Entry, MAX_FILE_SIZE, hash_hex};

use crate::common::Harness;

fn vault_uri(vault: &str, rest: &str) -> String {
    format!("/api/sync/vaults/{vault}{rest}")
}

#[tokio::test]
async fn needs_a_session() {
    let h = Harness::new(&["ivan"]);
    for (method, uri) in [
        ("GET", "/api/sync/vaults"),
        ("PUT", "/api/sync/vaults/notes"),
        ("GET", "/api/sync/vaults/notes/changes"),
        ("GET", "/api/sync/vaults/notes/files/a.typ"),
        ("PUT", "/api/sync/vaults/notes/files/a.typ"),
        ("DELETE", "/api/sync/vaults/notes/files/a.typ"),
    ] {
        let reply = h.send(method, uri, &[("x-base", "any")], b"x".to_vec()).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert_eq!(reply.error(), "sign-in required");
    }
    let bad = h.send("GET", "/api/sync/vaults", &[("authorization", "Bearer nope")], vec![]).await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn vaults_of_an_account() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    assert!(ivan.vaults().await.is_empty());
    let created = ivan.create_vault("notes").await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.json::<notes_hub::api::VaultInfo>().seq, 0);
    assert_eq!(ivan.create_vault("notes").await.status, StatusCode::OK, "an existing vault is not an error");
    ivan.create_vault("My%20notes").await;
    let names: Vec<_> = ivan.vaults().await.into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["My notes", "notes"]);
    assert!(h.dir.path().join("hub/ivan/notes/files").is_dir());

    for bad in ["_internal", ".hidden", "a%2Fb", "a%3Fb", "%20x"] {
        let reply = ivan.create_vault(bad).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{bad}");
        reply.error();
    }
    // An unknown vault is 404 and is not made by reading or writing.
    for reply in [
        ivan.get(&vault_uri("none", "/changes")).await,
        ivan.get(&vault_uri("none", "/files/a.typ")).await,
        ivan.put("none", "a.typ", "any", b"x").await,
        ivan.delete("none", "a.typ", "any").await,
    ] {
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        reply.error();
    }
    assert_eq!(ivan.vaults().await.len(), 2);
}

#[tokio::test]
async fn accounts_do_not_see_each_other() {
    let h = Harness::new(&["ivan", "anna"]);
    let ivan = h.account("ivan").await;
    let anna = h.account("anna").await;
    ivan.create_vault("notes").await;
    assert_eq!(ivan.put("notes", "secret.typ", "absent", b"ivan's").await.status, StatusCode::OK);
    assert!(anna.vaults().await.is_empty());
    assert_eq!(anna.get(&vault_uri("notes", "/files/secret.typ")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(anna.get(&vault_uri("notes", "/changes")).await.status, StatusCode::NOT_FOUND);
    // The same name is another vault.
    anna.create_vault("notes").await;
    assert_eq!(anna.get(&vault_uri("notes", "/files/secret.typ")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(anna.put("notes", "secret.typ", "absent", b"anna's").await.status, StatusCode::OK);
    assert_eq!(ivan.get(&vault_uri("notes", "/files/secret.typ")).await.body, b"ivan's");
    assert!(h.dir.path().join("hub/anna/notes/files/secret.typ").is_file());
}

#[tokio::test]
async fn write_read_changes_conflict_delete() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    ivan.create_vault("notes").await;

    let first = ivan.put("notes", "sub/a.typ", "absent", b"one").await;
    assert_eq!(first.status, StatusCode::OK);
    let first: Entry = first.json();
    assert_eq!((first.path.as_str(), first.seq, first.size), ("sub/a.typ", 1, 3));
    assert_eq!(first.hash.as_deref(), Some(hash_hex(b"one").as_str()));

    let read = ivan.get(&vault_uri("notes", "/files/sub/a.typ")).await;
    assert_eq!((read.status, read.body.as_slice()), (StatusCode::OK, &b"one"[..]));
    assert_eq!(read.header("content-type"), "application/octet-stream");
    assert_eq!(read.header("x-hash"), hash_hex(b"one"));
    assert_eq!(read.header("x-seq"), "1");

    // A stale base is a conflict that says what the server has.
    let stale = ivan.put("notes", "sub/a.typ", "absent", b"two").await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.json::<Conflict>().current, Some(first.clone()));
    let wrong = ivan.put("notes", "sub/a.typ", &hash_hex(b"other"), b"two").await;
    assert_eq!(wrong.status, StatusCode::CONFLICT);
    let new_file = ivan.put("notes", "b.typ", &hash_hex(b"other"), b"x").await;
    assert_eq!(new_file.json::<Conflict>().current, None);

    let hash = first.hash.clone().unwrap();
    let second: Entry = ivan.put("notes", "sub/a.typ", &hash, b"two").await.json();
    assert_eq!(second.seq, 2);
    let forced: Entry = ivan.put("notes", "sub/a.typ", "any", b"three").await.json();
    assert_eq!(forced.seq, 3);

    let changes: Changes = ivan.get(&vault_uri("notes", "/changes?after=0")).await.json();
    assert_eq!((changes.seq, changes.entries), (3, vec![forced.clone()]));
    let changes: Changes = ivan.get(&vault_uri("notes", "/changes?after=3")).await.json();
    assert!(changes.entries.is_empty());
    let changes: Changes = ivan.get(&vault_uri("notes", "/changes")).await.json();
    assert_eq!(changes.entries.len(), 1, "no `after` means from the start");

    let refused = ivan.delete("notes", "sub/a.typ", &hash).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.json::<Conflict>().current, Some(forced.clone()));
    let deleted = ivan.delete("notes", "sub/a.typ", forced.hash.as_deref().unwrap()).await;
    assert_eq!(deleted.status, StatusCode::OK);
    let tombstone: Entry = deleted.json();
    assert_eq!((tombstone.hash, tombstone.seq), (None, 4));
    let gone = ivan.get(&vault_uri("notes", "/files/sub/a.typ")).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    gone.error();
    assert_eq!(ivan.get(&vault_uri("notes", "/files/never.typ")).await.status, StatusCode::NOT_FOUND);
    // The file is in the plain tree of the vault until deleted: a hub is a folder of files.
    ivan.put("notes", "plain.typ", "absent", b"text").await;
    assert_eq!(std::fs::read(h.dir.path().join("hub/ivan/notes/files/plain.typ")).unwrap(), b"text");
}

#[tokio::test]
async fn bad_paths_and_bases() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    ivan.create_vault("notes").await;
    for path in [
        "../x.typ",
        "a/../x.typ",
        "%2e%2e/x.typ",
        "%2E%2E/%2e%2e/etc/passwd",
        "/abs.typ",
        "a%5Cb.typ",
        ".git/config",
        "a%00b.typ",
        "a//b.typ",
    ] {
        for reply in [
            ivan.put("notes", path, "any", b"x").await,
            ivan.delete("notes", path, "any").await,
            ivan.get(&vault_uri("notes", &format!("/files/{path}"))).await,
        ] {
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path}: {}", reply.text());
            reply.error();
        }
    }
    let uri = vault_uri("notes", "/files/a.typ");
    for method in ["PUT", "DELETE"] {
        let missing = ivan.call(method, &uri, &[], b"x").await;
        assert_eq!(missing.status, StatusCode::BAD_REQUEST, "{method}");
        assert!(missing.error().contains("x-base"));
        for bad in ["", "ANY", "abc", "latest"] {
            let reply = ivan.call(method, &uri, &[("x-base", bad)], b"x").await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{method} {bad:?}");
            reply.error();
        }
    }
    assert!(
        ivan.get(&vault_uri("notes", "/changes")).await.json::<Changes>().entries.is_empty(),
        "nothing was written"
    );
    // A file where a folder is, and the reverse.
    ivan.put("notes", "dir/a.typ", "absent", b"x").await;
    assert_eq!(ivan.put("notes", "dir", "absent", b"x").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(ivan.put("notes", "dir/a.typ/b.typ", "absent", b"x").await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn big_bodies_are_refused() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    ivan.create_vault("notes").await;
    let limit = usize::try_from(MAX_FILE_SIZE).unwrap();
    // Over the limit of the vault, under the one of the request.
    let reply = ivan.put("notes", "big.bin", "any", &vec![0; limit + 1]).await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    reply.error();
    // Over the limit of the request: not read at all.
    let reply = ivan.put("notes", "big.bin", "any", &vec![0; limit + 1024 * 1024]).await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    reply.error();
    assert_eq!(ivan.get(&vault_uri("notes", "/files/big.bin")).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn long_poll_returns_on_a_write() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    ivan.create_vault("notes").await;

    let waiting = {
        let hub = h.hub.clone();
        let token = h.login("ivan").await;
        tokio::spawn(async move {
            let app = hub.router();
            let started = Instant::now();
            let auth = format!("Bearer {token}");
            let reply = crate::common::send(
                &app,
                "GET",
                &vault_uri("notes", "/changes?after=0&wait=20"),
                &[("authorization", &auth)],
                vec![],
            )
            .await;
            (reply, started.elapsed())
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!waiting.is_finished(), "nothing to report yet");
    let entry: Entry = ivan.put("notes", "a.typ", "absent", b"x").await.json();
    let (reply, took) = tokio::time::timeout(Duration::from_secs(5), waiting).await.unwrap().unwrap();
    assert!(took < Duration::from_secs(5), "{took:?}");
    let changes: Changes = reply.json();
    assert_eq!(changes.entries, [entry]);

    // Changes that are already there are answered at once.
    let started = Instant::now();
    let now: Changes = ivan.get(&vault_uri("notes", "/changes?after=0&wait=20")).await.json();
    assert_eq!(now.entries.len(), 1);
    assert!(started.elapsed() < Duration::from_secs(5));

    // Nothing happens: an empty answer after the wait.
    let started = Instant::now();
    let idle: Changes = ivan.get(&vault_uri("notes", "/changes?after=1&wait=1")).await.json();
    assert!(idle.entries.is_empty() && idle.seq == 1);
    assert!(started.elapsed() >= Duration::from_millis(900), "{:?}", started.elapsed());
}

#[tokio::test]
async fn a_stop_answers_waiting_requests() {
    let h = Harness::new(&["ivan"]);
    let ivan = h.account("ivan").await;
    ivan.create_vault("notes").await;
    let token = h.login("ivan").await;
    let app = h.app.clone();
    let waiting = tokio::spawn(async move {
        let auth = format!("Bearer {token}");
        crate::common::send(
            &app,
            "GET",
            &vault_uri("notes", "/changes?after=0&wait=30"),
            &[("authorization", &auth)],
            vec![],
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    h.hub.close();
    let reply = tokio::time::timeout(Duration::from_secs(5), waiting).await.unwrap().unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.json::<Changes>().entries.is_empty());
}
