use std::process::{Child, Stdio};

use tracing::info;

use crate::{Config, ReplicationConfig};

/// Try to recover a snapshot when we don't have the encryption key
/// available yet. After unseal the remaining WAL will be applied.
pub fn recover_encrypted_storage_snapshot(config: &Config, replication: &ReplicationConfig) {
    if recover(
        replication,
        &config.encrypted_storage_path(),
        &replication.encrypted_bucket_url(),
    )
    .is_err()
    {
        // This is expected to happen as we don't have encryption key
        // to apply wal to db
        let tmp_db_path = format!("{}.tmp", config.encrypted_storage_path());
        if std::path::Path::new(&tmp_db_path).exists() {
            let _ = std::fs::rename(tmp_db_path, config.encrypted_storage_path());
        }
    }
}

pub fn recover(
    replication: &ReplicationConfig,
    output_path: &str,
    bucket_url: &str,
) -> anyhow::Result<()> {
    let has_backup = has_backup(replication, bucket_url)?;
    if !has_backup {
        info!("No backup found");
        return Ok(());
    }
    // Local storage should be more up to date, so don't overwrite it
    if std::path::Path::new(output_path).exists() {
        info!("Found local storage at `{output_path}` and a backup. Using local storage");
        return Ok(());
    }

    let mut cmd = std::process::Command::new("litestream");
    cmd.arg("restore")
        .arg("-o")
        .arg(output_path)
        .arg(bucket_url)
        .stdin(Stdio::inherit())
        // TODO: pipe to configured file ouput
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("LITESTREAM_ACCESS_KEY_ID", &replication.access_key_id)
        .env(
            "LITESTREAM_SECRET_ACCESS_KEY",
            &replication.secret_access_key,
        );

    match cmd.status() {
        Ok(status) => {
            if status.success() {
                info!("Successfully restored backup");
                Ok(())
            } else {
                Err(anyhow::Error::msg("Failed to recover from backup"))
            }
        }
        Err(err) => Err(anyhow::Error::msg(format!(
            "Failed to recover from backup. Error: {err:#?}"
        ))),
    }
}

fn has_backup(replication: &ReplicationConfig, bucket_url: &str) -> anyhow::Result<bool> {
    let mut cmd = std::process::Command::new("litestream");
    cmd.arg("snapshots")
        .arg(bucket_url)
        .env("LITESTREAM_ACCESS_KEY_ID", &replication.access_key_id)
        .env(
            "LITESTREAM_SECRET_ACCESS_KEY",
            &replication.secret_access_key,
        );

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                // Kind of hacky
                let resp = String::from_utf8_lossy(&output.stdout);
                let lines = resp.lines().count();
                Ok(lines > 1)
            } else {
                Err(anyhow::Error::msg("Failed to recover from backup"))
            }
        }
        Err(err) => Err(anyhow::Error::msg(format!(
            "Failed to lookup backup. Error: {err:#?}"
        ))),
    }
}

pub fn replicate(
    replication: &ReplicationConfig,
    encryption_key: Option<String>,
    db_path: &str,
    bucket_url: &str,
) -> anyhow::Result<Child> {
    let mut cmd = std::process::Command::new("litestream");
    cmd.arg("replicate")
        .arg(db_path)
        .arg(bucket_url)
        .stdin(Stdio::inherit())
        // TODO: pipe to configured file ouput
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("LITESTREAM_ACCESS_KEY_ID", &replication.access_key_id)
        .env(
            "LITESTREAM_SECRET_ACCESS_KEY",
            &replication.secret_access_key,
        );

    // TODO: this is really bad. Couple of other approaches that should be revisited:
    // - pass it in stdin to subprocess
    // - wrap litestream in FFI to avoid the subprocess mess
    // - build own rust library. This will eventually be the solution given that
    //   litestream has stopped supporting streaming replication.
    if let Some(encryption_key) = encryption_key {
        cmd.env("ENCRYPTION_KEY", encryption_key);
    }

    match cmd.spawn() {
        Ok(child) => {
            info!("Replication started");
            Ok(child)
        }
        Err(error) => Err(anyhow::Error::msg(format!(
            "Failed to setup replication. Error: {error:#?}"
        ))),
    }
}
