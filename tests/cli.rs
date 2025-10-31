use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn adr_dir(tmp: &Path) -> PathBuf {
    tmp.join("docs").join("adr")
}

fn read(path: impl Into<PathBuf>) -> String {
    fs::read_to_string(path.into()).expect("read file")
}

#[test]
fn new_creates_proposed_and_index() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = assert_cmd::Command::cargo_bin("radr").unwrap();
    cmd.current_dir(tmp.path()).arg("new").arg("First ADR");
    cmd.assert().success();

    let adr0 = adr_dir(tmp.path()).join("0001-first-adr.md");
    assert!(adr0.exists());
    let content = read(&adr0);
    assert!(content.contains("Status: Proposed"));
    assert!(content.contains("Date:"));

    let index = adr_dir(tmp.path()).join("index.md");
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
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose DB"])
        .assert()
        .success();

    // accept by id
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["accept", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Accepted ADR 0001"));

    let adr1 = adr_dir(tmp.path()).join("0001-choose-db.md");
    let c1 = read(&adr1);
    assert!(c1.contains("Status: Accepted"));
    assert!(c1.contains(&format!("Date: {}", today)));

    // new second and accept by title
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Use Queue"])
        .assert()
        .success();

    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["accept", "Use Queue"])
        .assert()
        .success();

    let adr2 = adr_dir(tmp.path()).join("0002-use-queue.md");
    let c2 = read(&adr2);
    assert!(c2.contains("Status: Accepted"));
}

#[test]
fn supersede_marks_old_and_new_proposed_and_updates_index() {
    let tmp = tempfile::tempdir().unwrap();

    // create first
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose X"])
        .assert()
        .success();

    // supersede
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Y"])
        .assert()
        .success();

    let old = adr_dir(tmp.path()).join("0001-choose-x.md");
    let new_adr = adr_dir(tmp.path()).join("0002-choose-y.md");
    assert!(old.exists());
    assert!(new_adr.exists());

    let old_c = read(&old);
    assert!(old_c.contains("Status: Superseded by 0002"));
    assert!(old_c.contains("Superseded-by: 0002"));
    let s_pos = old_c.find("Status: Superseded by 0002").unwrap();
    let sb_pos = old_c.find("Superseded-by: 0002").unwrap();
    assert!(s_pos < sb_pos);

    let new_c = read(&new_adr);
    assert!(new_c.contains("Supersedes: [0001](0001-choose-x.md)"));
    assert!(new_c.contains("Status: Proposed"));

    let index = adr_dir(tmp.path()).join("index.md");
    let idx = read(&index);
    assert!(idx.contains("0001: Choose X"));
    assert!(idx.contains("0002: Choose Y"));
}

#[test]
fn list_outputs_lines_and_regenerates_index() {
    let tmp = tempfile::tempdir().unwrap();

    // create two
    for title in ["One", "Two"] {
        assert_cmd::Command::cargo_bin("radr")
            .unwrap()
            .current_dir(tmp.path())
            .args(["new", title])
            .assert()
            .success();
    }

    // list
    let mut cmd = assert_cmd::Command::cargo_bin("radr").unwrap();
    cmd.current_dir(tmp.path()).arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0001 | One"))
        .stdout(predicate::str::contains("0002 | Two"));

    // index exists
    let index = adr_dir(tmp.path()).join("index.md");
    assert!(index.exists());
}

#[test]
fn config_flag_changes_adr_dir_and_index_name() {
    let tmp = tempfile::tempdir().unwrap();
    // Write YAML config overriding defaults
    let cfg = tmp.path().join("radr.yaml");
    std::fs::write(&cfg, b"adr_dir: adrs\nindex_name: ADRS.md\n").unwrap();

    // Use --config to pick up YAML
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["--config", cfg.to_str().unwrap(), "new", "From Config"])
        .assert()
        .success();

    let adr = tmp.path().join("adrs").join("0001-from-config.md");
    assert!(adr.exists());
    let index = tmp.path().join("adrs").join("ADRS.md");
    assert!(index.exists());
}

#[test]
fn env_config_overrides_local_files() {
    let tmp = tempfile::tempdir().unwrap();
    // local toml
    std::fs::write(
        tmp.path().join("radr.toml"),
        b"adr_dir='local'\nindex_name='LOCAL.md'\n",
    )
    .unwrap();
    // env yaml
    let env_yaml = tmp.path().join("radr.yaml");
    std::fs::write(&env_yaml, b"adr_dir: env\nindex_name: ENV.md\n").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("radr").unwrap();
    cmd.current_dir(tmp.path())
        .env("RADR_CONFIG", env_yaml)
        .args(["new", "From Env"])
        .assert()
        .success();

    let adr = tmp.path().join("env").join("0001-from-env.md");
    assert!(adr.exists());
    let idx = tmp.path().join("env").join("ENV.md");
    assert!(idx.exists());
}

#[test]
fn mdx_new_creates_front_matter_and_index() {
    let tmp = tempfile::tempdir().unwrap();
    // MDX + front matter via TOML config
    let cfg = tmp.path().join("radr.toml");
    std::fs::write(
        &cfg,
        b"adr_dir='adrs'\nindex_name='INDEX.md'\nformat='mdx'\nfront_matter=true\n",
    )
    .unwrap();

    // create new
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "MDX Test"])
        .assert()
        .success();

    // file exists with .mdx extension and front matter
    let adr = tmp.path().join("adrs").join("0001-mdx-test.mdx");
    assert!(adr.exists());
    let c = read(&adr);
    assert!(c.starts_with("---\n"));
    assert!(c.contains("title:"));
    // After front matter, ensure classic fields exist
    assert!(c.contains("Status: Proposed"));
    assert!(c.contains("Date:"));
    assert!(c.contains("## Context"));

    // index exists and includes entry
    let index = tmp.path().join("adrs").join("INDEX.md");
    assert!(index.exists());
    let idx = read(&index);
    assert!(idx.contains("0001: MDX Test"));
    assert!(idx.contains("Status: Proposed"));
}

#[test]
fn mdx_accept_updates_front_matter() {
    let tmp = tempfile::tempdir().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    // MDX + front matter
    std::fs::write(
        tmp.path().join("radr.toml"),
        b"adr_dir='adrs'\nformat='mdx'\nfront_matter=true\n",
    )
    .unwrap();

    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Accept Me"])
        .assert()
        .success();

    // accept should update classic fields after front matter
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["accept", "1"])
        .assert()
        .success();

    let adr = tmp.path().join("adrs").join("0001-accept-me.mdx");
    let c = read(&adr);
    assert!(c.contains("Status: Accepted"));
    assert!(c.contains(&format!("Date: {}", today)));
}

#[test]
fn mdx_supersede_updates_front_matter_and_index() {
    let tmp = tempfile::tempdir().unwrap();
    // MDX + front matter
    std::fs::write(
        tmp.path().join("radr.toml"),
        b"adr_dir='adrs'\nindex_name='INDEX.md'\nformat='mdx'\nfront_matter=true\n",
    )
    .unwrap();

    // create first
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose X"])
        .assert()
        .success();

    // supersede
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Y"])
        .assert()
        .success();

    let old = tmp.path().join("adrs").join("0001-choose-x.mdx");
    let new_adr = tmp.path().join("adrs").join("0002-choose-y.mdx");
    assert!(old.exists());
    assert!(new_adr.exists());

    let old_c = read(&old);
    assert!(old_c.contains("Status: Superseded by 0002"));
    assert!(old_c.contains("Superseded-by: 0002"));

    let new_c = read(&new_adr);
    assert!(new_c.contains("Supersedes: [0001]("));
    assert!(new_c.contains("Status: Proposed"));

    let index = tmp.path().join("adrs").join("INDEX.md");
    let idx = read(&index);
    assert!(idx.contains("0001: Choose X"));
    assert!(idx.contains("0002: Choose Y"));
}

#[test]
fn accept_nonexistent_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["accept", "9999"])
        .assert()
        .failure();
}

#[test]
fn supersede_nonexistent_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "9999", "Y"])
        .assert()
        .failure();
}

#[test]
fn supersede_already_superseded_shows_message_and_fails() {
    let tmp = tempfile::tempdir().unwrap();

    // create first
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose X"])
        .assert()
        .success();

    // supersede once (1 -> 2)
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Y"])
        .assert()
        .success();

    // try to supersede ADR 1 again; should fail with a helpful message
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Z"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "0001: Choose X is already superseded by 0002: Choose Y",
        ));

    // Ensure no ADR 3 was created by the failed attempt
    assert!(!adr_dir(tmp.path()).join("0003-choose-z.md").exists());
}

#[test]
fn supersede_already_superseded_force_allows_and_updates() {
    let tmp = tempfile::tempdir().unwrap();

    // create first
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Choose X"])
        .assert()
        .success();

    // supersede once (1 -> 2)
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Y"])
        .assert()
        .success();

    // supersede ADR 1 again with --force; should succeed and create ADR 3
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["supersede", "1", "Choose Z", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Created ADR 0003 superseding 0001",
        ));

    // Check ADR 3 exists
    assert!(adr_dir(tmp.path()).join("0003-choose-z.md").exists());

    // Old ADR 1 should now show superseded by 0003
    let old = adr_dir(tmp.path()).join("0001-choose-x.md");
    let c = read(&old);
    assert!(c.contains("Status: Superseded by 0003"));
}

#[test]
fn reject_by_id_and_title_updates_status_and_date() {
    let tmp = tempfile::tempdir().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // new
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Discard Me"])
        .assert()
        .success();

    // reject by id
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["reject", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rejected ADR 0001"));

    let adr1 = adr_dir(tmp.path()).join("0001-discard-me.md");
    let c1 = read(&adr1);
    assert!(c1.contains("Status: Rejected"));
    assert!(c1.contains(&format!("Date: {}", today)));

    // new second and reject by title (case-insensitive)
    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "Reject This Too"])
        .assert()
        .success();

    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["reject", "reject this too"])
        .assert()
        .success();

    let adr2 = adr_dir(tmp.path()).join("0002-reject-this-too.md");
    let c2 = read(&adr2);
    assert!(c2.contains("Status: Rejected"));
}

#[test]
fn template_via_config_is_applied() {
    let tmp = tempfile::tempdir().unwrap();
    // Write template and config
    let tpl = tmp.path().join("tpl.md");
    std::fs::write(
        &tpl,
        "# ADR {{NUMBER}}: {{TITLE}}\n\nDate: {{DATE}}\nStatus: {{STATUS}}\n\nTEMPLATE\n",
    )
    .unwrap();
    let cfg = tmp.path().join("radr.toml");
    std::fs::write(
        &cfg,
        format!(
            "adr_dir='adrs'\nindex_name='index.md'\ntemplate='{}'\n",
            tpl.display()
        ),
    )
    .unwrap();

    assert_cmd::Command::cargo_bin("radr")
        .unwrap()
        .current_dir(tmp.path())
        .args(["new", "From Template"])
        .assert()
        .success();

    let adr = tmp.path().join("adrs").join("0001-from-template.md");
    let c = read(&adr);
    assert!(c.contains("TEMPLATE"));
    assert!(c.contains("Status: Proposed"));
}
