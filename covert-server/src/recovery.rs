use synclite::config::RestoreConfig;
use tokio::sync::broadcast;
use tracing::info;

use crate::ReplicationConfig;

/// TODO
pub async fn has_encrypted_storage_backup(
    replication: &ReplicationConfig,
) -> Result<bool, synclite::error::Error> {
    let replica_config = synclite::replica::Config::S3(synclite::replica::s3::Config {
        bucket: replication.bucket.to_string(),
        endpoint: replication.endpoint.clone(),
        region: replication.region.clone(),
        prefix: replication.encrypted_db_prefix(),
    });
    synclite::has_backup(replica_config).await
}

pub async fn recover(
    replication: &ReplicationConfig,
    output_path: &str,
    prefix: &str,
    encryption_key: Option<String>,
) -> Result<(), synclite::error::Error> {
    let restore_config = RestoreConfig {
        db_path: output_path.to_string(),
        replica: synclite::replica::Config::S3(synclite::replica::s3::Config {
            bucket: replication.bucket.to_string(),
            endpoint: replication.endpoint.clone(),
            region: replication.region.clone(),
            prefix: prefix.to_string(),
        }),
        if_not_exists: false,
        encryption_key,
    };

    if synclite::has_backup(restore_config.replica.clone()).await? {
        synclite::restore(restore_config).await
    } else {
        Ok(())
    }
}

pub async fn replicate(
    replication: &ReplicationConfig,
    encryption_key: Option<String>,
    db_path: &str,
    prefix: &str,
    stop_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let config = synclite::config::ReplicateConfig {
        db_path: db_path.to_string(),
        replica: synclite::replica::Config::S3(synclite::replica::s3::Config {
            bucket: replication.bucket.to_string(),
            endpoint: replication.endpoint.clone(),
            region: replication.region.clone(),
            prefix: prefix.to_string(),
        }),
        encryption_key,
    };
    info!("Starting to replicate with: {config:#?}");
    tokio::spawn(async move {
        if let Err(err) = synclite::replicate(config, stop_rx).await {
            tracing::error!("Replication failed with an error {err:?}");
            // TODO: shutdown
        }
        tracing::info!("Replication stopped");
    });

    Ok(())
}
