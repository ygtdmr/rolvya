use super::*;
use crate::controller_keys::{
    human_auth::sessions::https::BrowserAssets, worker_auth::https::tls::ServerIdentity,
};
use crate::resilience::common;
use std::io::BufRead;
pub(in crate::controller_keys) async fn operational_browser_driver(dir: &Path, assets: &Path) {
    let mut f = Fixture::new().await;
    f.initialize().await;
    let user = f.enroll().await;
    let plan = common::plan();
    let project = plan.key.project_id.clone();
    f.op.store
        .create_project(&project, &common::owner())
        .await
        .unwrap();
    let validated = rolvya_plan_validator::validate(
        &plan,
        &common::SyntheticPolicy,
        rolvya_plan_validator::Limits::default(),
    )
    .unwrap();
    let revision =
        f.op.store
            .save_draft(&validated, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
    f.op.store
        .activate(
            &revision,
            &common::request(),
            &common::owner(),
            &common::OwnerPolicy,
            &common::SyntheticPolicy,
            rolvya_plan_validator::Limits::default(),
        )
        .await
        .unwrap();
    let binding =
        f.op.prepare_binding(&project, &user, "owner")
            .await
            .unwrap();
    for entry in std::fs::read_dir(&f.config.assets).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    for entry in std::fs::read_dir(assets).unwrap() {
        let entry = entry.unwrap();
        write_private(
            &f.config.assets.join(entry.file_name()),
            &std::fs::read(entry.path()).unwrap(),
        );
    }
    f.config.asset_sha256 = service::asset_digest(&f.config.assets).unwrap();
    f.op.close().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    f.config.canonical_origin = base.clone();
    f.config.listen = listener.local_addr().unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let identity = ServerIdentity::fixture(
        vec![certificate.cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
    );
    let op = HostOperator::open(f.config.clone()).await.unwrap();
    let store = op.store.clone();
    let service = ControllerService::fixture(
        op,
        identity,
        BrowserAssets::load(&f.config.assets).await.unwrap(),
    )
    .await
    .unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(service.serve_listener(listener, async {
        let _ = stopped.await;
    }));
    let info=Zeroizing::new(serde_json::to_vec(&serde_json::json!({"url":base,"email":"owner@example.test","password":"offline-unique-human-password","viewerEmail":"unenrolled@example.test","viewerPassword":"no-enrolled-viewer-credential","totp":null,"project":project.as_str(),"fingerprint":revision.fingerprint(),"bindingApproval":binding.id})).unwrap());
    write_private(&dir.join("info.tmp"), &info);
    std::fs::rename(dir.join("info.tmp"), dir.join("info.json")).unwrap();
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let input = tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines().take(20) {
            let Ok(line) = line else { break };
            if line.len() > 64 || send.blocking_send(line).is_err() {
                break;
            }
        }
    });
    let mut sequence = 0;
    while let Some(command) = receive.recv().await {
        match command.as_str() {
            "stop" => break,
            "emit" => {
                let mut tx = store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
                sqlx::query("UPDATE task_runtime SET state='validating' WHERE project_id=?")
                    .bind(project.as_str())
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO scheduler_events(project_id,request_id,kind,actor,recorded_at,payload) VALUES(?,?,'validate',?,?,'{}')").bind(project.as_str()).bind(random_id().unwrap()).bind(user.as_str()).bind(now().unwrap()).execute(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
            }
            _ => panic!("unsupported private operational command"),
        };
        sequence += 1;
        tokio::fs::write(dir.join("ack"), sequence.to_string())
            .await
            .unwrap();
    }
    drop(receive);
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
    input.await.unwrap();
}
