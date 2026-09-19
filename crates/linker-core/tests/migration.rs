use linker_core::state::StateDb;
use rusqlite::{params, Connection};
use std::fs;
use std::path::Path;

fn legacy(directory: &Path) {
    fs::create_dir_all(directory.join("manifests")).unwrap();
    fs::create_dir_all(directory.join("rules")).unwrap();
    let conn = Connection::open(directory.join("state.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
         CREATE TABLE items(id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, item_type TEXT NOT NULL DEFAULT 'directory',
             local_path TEXT NOT NULL UNIQUE, cloud_path TEXT NOT NULL, rule_path TEXT NOT NULL,
             status TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, last_sync_at INTEGER, last_error TEXT);
         CREATE TABLE exclude_rules(id TEXT PRIMARY KEY, item_id TEXT NOT NULL, pattern TEXT NOT NULL, created_at INTEGER NOT NULL,
             UNIQUE(item_id, pattern), FOREIGN KEY(item_id) REFERENCES items(id));
         CREATE TABLE file_states(id TEXT PRIMARY KEY,item_id TEXT NOT NULL,relative_path TEXT NOT NULL,
             local_hash TEXT,local_mtime INTEGER,local_size INTEGER,cloud_hash TEXT,cloud_mtime INTEGER,cloud_size INTEGER,
             last_synced_hash TEXT,last_synced_at INTEGER,deleted INTEGER NOT NULL DEFAULT 0,
             UNIQUE(item_id, relative_path),FOREIGN KEY(item_id) REFERENCES items(id));"
    ).unwrap();
    for i in 0..6 {
        let name = format!("item{i}");
        conn.execute(
            "INSERT INTO items VALUES (?1,?2,'directory',?3,?4,?5,'active',100,200,200,NULL)",
            params![
                format!("id{i}"),
                name,
                format!("/source/{name}"),
                format!("/target/{name}"),
                directory
                    .join(format!("rules/{name}.ignore"))
                    .to_str()
                    .unwrap()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exclude_rules VALUES (?1,?2,'tmp/',100)",
            params![format!("r{i}"), format!("id{i}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_states VALUES (?1,?2,'a','abc',100,3,'abc',100,3,'abc',100,0)",
            params![format!("f{i}"), format!("id{i}")],
        )
        .unwrap();
        fs::write(directory.join(format!("rules/{name}.ignore")), "tmp/\n").unwrap();
        fs::write(
            directory.join(format!("manifests/{name}.json")),
            serde_json::to_vec(&serde_json::json!({
                "schema_version":1,"id":format!("id{i}"),"name":name,"type":"directory",
                "source_path":format!("/source/{name}"),"target_path":format!("/target/{name}"),
                "rule_path":directory.join(format!("rules/{name}.ignore")),
                "created_at":"2026-08-21T00:00:00Z","updated_at":"2026-08-21T00:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn read_only_preview_rejects_legacy_state_without_migrating() {
    let tmp = tempfile::tempdir().unwrap();
    legacy(tmp.path());
    let path = tmp.path().join("state.sqlite");
    let before = fs::read(&path).unwrap();
    let error = StateDb::open_read_only(&path)
        .err()
        .expect("legacy preview rejected");
    assert!(error.to_string().contains("requires upgraded Linker state"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!tmp.path().join("backups").exists());
    assert!(!tmp.path().join("locks").exists());
}

#[test]
fn interrupted_backups_are_rebuilt_before_retiring_old_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    legacy(tmp.path());
    let backup = tmp.path().join("backups/gitignore-v2");
    fs::create_dir_all(backup.join("manifests")).unwrap();
    fs::create_dir_all(backup.join("rules")).unwrap();
    fs::write(backup.join("state.sqlite"), "").unwrap();
    fs::write(backup.join("manifests/item0.json"), "{partial").unwrap();
    fs::write(backup.join("rules/item0.ignore"), "t").unwrap();
    StateDb::open(&tmp.path().join("state.sqlite")).unwrap();
    let saved = Connection::open(backup.join("state.sqlite")).unwrap();
    assert_eq!(
        saved
            .query_row("SELECT count(*) FROM exclude_rules", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        6
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(backup.join("manifests/item0.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(
        fs::read_to_string(backup.join("rules/item0.ignore")).unwrap(),
        "tmp/\n"
    );
}

#[test]
fn migration_preserves_six_associations_baselines_and_archives_rules() {
    let tmp = tempfile::tempdir().unwrap();
    legacy(tmp.path());
    for _ in 0..2 {
        let db = StateDb::open(&tmp.path().join("state.sqlite")).unwrap();
        let items = db.list_items().unwrap();
        assert_eq!(items.len(), 6);
        for item in items {
            assert_eq!(item.last_sync_at, Some(200));
            assert_eq!(
                db.list_file_states(&item.id).unwrap()[0]
                    .local_hash
                    .as_deref(),
                Some("abc")
            );
            let manifest: serde_json::Value = serde_json::from_slice(
                &fs::read(tmp.path().join(format!("manifests/{}.json", item.name))).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest["schema_version"], 2);
            assert!(manifest.get("rule_path").is_none());
            assert_eq!(manifest["created_at"], "2026-08-21T00:00:00Z");
        }
    }
    assert!(!tmp.path().join("rules").exists());
    let backup = tmp.path().join("backups/gitignore-v2");
    let old = Connection::open(backup.join("state.sqlite")).unwrap();
    assert_eq!(
        old.query_row("SELECT count(*) FROM exclude_rules", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        fs::read_to_string(backup.join("rules/item0.ignore")).unwrap(),
        "tmp/\n"
    );
    let current = Connection::open(tmp.path().join("state.sqlite")).unwrap();
    assert!(current.prepare("SELECT rule_path FROM items").is_err());
    assert!(current.prepare("SELECT * FROM exclude_rules").is_err());
}

#[test]
fn filesystem_migration_can_finish_after_database_was_committed() {
    let tmp = tempfile::tempdir().unwrap();
    legacy(tmp.path());
    StateDb::open(&tmp.path().join("state.sqlite")).unwrap();
    let backup = tmp.path().join("backups/gitignore-v2");
    fs::remove_file(backup.join("done")).unwrap();
    fs::copy(
        backup.join("manifests/item0.json"),
        tmp.path().join("manifests/item0.json"),
    )
    .unwrap();
    fs::create_dir(tmp.path().join("rules")).unwrap();
    fs::copy(
        backup.join("rules/item0.ignore"),
        tmp.path().join("rules/item0.ignore"),
    )
    .unwrap();
    StateDb::open(&tmp.path().join("state.sqlite")).unwrap();
    assert!(backup.join("done").exists());
    assert!(!tmp.path().join("rules").exists());
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(tmp.path().join("manifests/item0.json")).unwrap())
            .unwrap();
    assert_eq!(value["schema_version"], 2);
}

#[test]
fn migration_never_deletes_external_rule_path_or_unknown_snapshots() {
    let tmp = tempfile::tempdir().unwrap();
    let support = tmp.path().join("support");
    legacy(&support);
    let outside = tmp.path().join("outside.ignore");
    fs::write(&outside, "precious").unwrap();
    let conn = Connection::open(support.join("state.sqlite")).unwrap();
    conn.execute(
        "UPDATE items SET rule_path=?1 WHERE name='item0'",
        [outside.to_str().unwrap()],
    )
    .unwrap();
    fs::write(support.join("rules/unowned.txt"), "keep").unwrap();
    StateDb::open(&support.join("state.sqlite")).unwrap();
    assert_eq!(fs::read_to_string(outside).unwrap(), "precious");
    assert_eq!(
        fs::read_to_string(support.join("rules/unowned.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn linked_rules_directory_is_rejected_before_migration_or_external_changes() {
    let tmp = tempfile::tempdir().unwrap();
    legacy(tmp.path());
    fs::rename(tmp.path().join("rules"), tmp.path().join("outside")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("outside"), tmp.path().join("rules")).unwrap();
    assert!(StateDb::open(&tmp.path().join("state.sqlite")).is_err());
    assert!(tmp.path().join("outside/item0.ignore").exists());
    let conn = Connection::open(tmp.path().join("state.sqlite")).unwrap();
    assert!(conn.prepare("SELECT rule_path FROM items").is_ok());
}
