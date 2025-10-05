use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

fn adr_dir(tmp: &PathBuf) -> PathBuf {
    tmp.join("docs").join("adr")
}

fn read(path: impl Into<PathBuf>) -> String {
    fs::read_to_string(path.into()).expect("read file")
}

#[test]
fn new_creates_proposed_and_index() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = assert_cmd::Command::cargo_bin("radr").unwrap();
    cmd.current_dir(tmp.path())
        .arg("new")
        .arg("First ADR");
    cmd.assert().success();

    let adr0 = adr_dir(&tmp.path().to_path_buf()).join("0001-first-adr.md");
    assert!(adr0.exists());
    let content = read(&adr0);
    assert!(content.contains("Status: Proposed"));
    assert!(content.contains("Date:"));

    let index = adr_dir(&tmp.path().to_path_buf()).join("index.md");
    assert!(index.exists());
    let idx = read(&index);
    assert!(idx.contains("0001: First ADR"));
    assert!(idx.contains("Status: Proposed"));
}

#[test]
fn accept_by_id_and_title_updates_status_and_date() {
    let tmp = tempfile::tempdir().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // new
    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose DB"])
        .assert()
        .success();

    // accept by id
    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["accept", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Accepted ADR 0001"));

    let adr1 = adr_dir(&tmp.path().to_path_buf()).join("0001-choose-db.md");
    let c1 = read(&adr1);
    assert!(c1.contains("Status: Accepted"));
    assert!(c1.contains(&format!("Date: {}", today)));

    // new second and accept by title
    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["new", "Use Queue"])
        .assert()
        .success();

    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["accept", "Use Queue"])
        .assert()
        .success();

    let adr2 = adr_dir(&tmp.path().to_path_buf()).join("0002-use-queue.md");
    let c2 = read(&adr2);
    assert!(c2.contains("Status: Accepted"));
}

#[test]
fn supersede_marks_old_and_new_proposed_and_updates_index() {
    let tmp = tempfile::tempdir().unwrap();

    // create first
    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose X"])
        .assert()
        .success();

    // supersede
    assert_cmd::Command::cargo_bin("radr").unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Y"])
        .assert()
        .success();

    let old = adr_dir(&tmp.path().to_path_buf()).join("0001-choose-x.md");
    let new_adr = adr_dir(&tmp.path().to_path_buf()).join("0002-choose-y.md");
    assert!(old.exists());
    assert!(new_adr.exists());

    let old_c = read(&old);
    assert!(old_c.contains("Status: Superseded by 0002"));
    assert!(old_c.contains("Superseded-by: 0002"));

    let new_c = read(&new_adr);
    assert!(new_c.contains("Supersedes: 0001"));
    assert!(new_c.contains("Status: Proposed"));

    let index = adr_dir(&tmp.path().to_path_buf()).join("index.md");
    let idx = read(&index);
    assert!(idx.contains("0001: Choose X"));
    assert!(idx.contains("0002: Choose Y"));
}

#[test]
fn list_outputs_lines_and_regenerates_index() {
    let tmp = tempfile::tempdir().unwrap();

    // create two
    for title in ["One", "Two"] {
        assert_cmd::Command::cargo_bin("radr").unwrap()
            .current_dir(tmp.path())
            .args(["new", title])
            .assert()
            .success();
    }

    // list
    let mut cmd = assert_cmd::Command::cargo_bin("radr").unwrap();
    cmd.current_dir(tmp.path()).arg("list");
    cmd.assert().success().stdout(predicate::str::contains("0001 | One")).stdout(predicate::str::contains("0002 | Two"));

    // index exists
    let index = adr_dir(&tmp.path().to_path_buf()).join("index.md");
    assert!(index.exists());
}

