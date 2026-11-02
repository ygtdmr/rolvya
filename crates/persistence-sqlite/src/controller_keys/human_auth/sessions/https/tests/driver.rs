//! Private browser automation driver. No fixture enrollment, clock, root or
//! control channel exists in production. Credentials stay in a mode-0600 file
//! outside uploaded browser artifacts; stdin is the test controller, not HTTP.
use super::*;
use std::io::BufRead;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "utility process executed by the real HTTPS Playwright suite"]
async fn browser_https_driver() {
    let dir = std::path::PathBuf::from(std::env::var_os("ROLVYA_BROWSER_FIXTURE_DIR").unwrap());
    let assets =
        std::path::PathBuf::from(std::env::var_os("ROLVYA_BROWSER_FIXTURE_ASSETS").unwrap());
    let factor = std::env::var_os("ROLVYA_BROWSER_FIXTURE_TOTP").is_some();
    let r = Running::from_seed(
        browser_seed(factor).await,
        Some(BrowserAssets::load(&assets).await.unwrap()),
    )
    .await;
    let code =
        r.seed.totp.as_ref().map(|seed| {
            super::super::super::tests::code(seed, r.seed.clock.load(Ordering::SeqCst)).0
        });
    let info = Zeroizing::new(serde_json::to_vec(&serde_json::json!({
        "url":r.base,"email":r.seed.email.as_str(),"password":std::str::from_utf8(&r.seed.credential.0).unwrap(),
        "viewerEmail":r.seed.viewer_email.as_str(),"viewerPassword":std::str::from_utf8(&r.seed.viewer_credential.0).unwrap(),
        "totp":code.as_deref(),"project":r.seed.project.as_str(),"fingerprint":r.seed.fingerprint
    })).unwrap());
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("info.tmp"))
        .unwrap();
    std::io::Write::write_all(&mut file, &info).unwrap();
    drop(file);
    std::fs::rename(dir.join("info.tmp"), dir.join("info.json")).unwrap();
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let input = tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines().take(100) {
            let Ok(line) = line else { break };
            if line.len() > 100 || send.blocking_send(line).is_err() {
                break;
            }
        }
    });
    let mut sequence = 0;
    while let Some(command) = receive.recv().await {
        match command.as_str() {
            "stop" => break,
            "emit" => {
                let mut tx = r
                    .seed
                    .store
                    .pool
                    .begin_with("BEGIN IMMEDIATE")
                    .await
                    .unwrap();
                sqlx::query("UPDATE task_runtime SET state='validating' WHERE project_id=?")
                    .bind(r.seed.project.as_str())
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO scheduler_events(project_id,request_id,kind,actor,recorded_at,payload) VALUES(?,?,'validate',?,?,'{\"fixture\":true}')").bind(r.seed.project.as_str()).bind(new_id().unwrap()).bind(r.seed.user.as_str()).bind(r.seed.clock.load(Ordering::SeqCst)).execute(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
            }
            "revoke" => {
                sqlx::query(
                    "UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?",
                )
                .bind(r.seed.user.as_str())
                .execute(&r.seed.store.pool)
                .await
                .unwrap();
            }
            "advance" => {
                r.seed.clock.fetch_add(3600, Ordering::SeqCst);
            }
            value if value.starts_with("mail:") => {
                let reference = &value[5..];
                assert!(reference.len() == 32 && reference.bytes().all(|b| b.is_ascii_hexdigit()));
                let token = Zeroizing::new(
                    (r.seed.mail)(reference).expect("private message not delivered"),
                );
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(dir.join("message"))
                    .unwrap();
                std::io::Write::write_all(&mut file, token.as_bytes()).unwrap();
            }
            _ => panic!("unsupported private fixture command"),
        }
        sequence += 1;
        tokio::fs::write(dir.join("ack"), sequence.to_string())
            .await
            .unwrap();
    }
    drop(receive);
    r.close().await;
    // The parent closes stdin after stop; no blocked reader survives the fixture.
    input.await.unwrap();
}
