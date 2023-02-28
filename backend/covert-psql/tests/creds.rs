mod common;

use std::time::Duration;

use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig},
    operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse},
    psql::{CreateRoleParams, SetConnectionParams},
    Client,
};
use rand::{distributions::Alphanumeric, Rng};
use sqlx::Connection;

use crate::common::{setup, setup_unseal, MOUNT_PATH};

pub const CONNECTION_STR: &str =
    "postgresql://root:rootpassword@127.0.0.1:5432/postgres?sslmode=disable";

async fn create_role(sdk: &Client) -> String {
    let role_name: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(7)
        .map(char::from)
        .collect();

    // Create a role
    let role_sql = r#"
            CREATE ROLE "{{name}}" WITH LOGIN PASSWORD '{{password}}' 
                VALID UNTIL '{{expiration}}' INHERIT;
                GRANT SELECT ON ALL TABLES IN SCHEMA public TO "{{name}}""#;
    let role_revocation_sql = r#"DROP ROLE "{{name}}""#;
    let resp = sdk
        .psql
        .create_role(
            MOUNT_PATH,
            &role_name,
            &CreateRoleParams {
                sql: role_sql.to_string(),
                revocation_sql: role_revocation_sql.to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.sql, role_sql);
    assert_eq!(resp.revocation_sql, role_revocation_sql);

    role_name
}

#[tokio::test]
#[cfg_attr(not(feature = "psql-integration-test"), ignore)]
async fn generate_credentials() {
    let sdk = setup_unseal().await;

    // No connection to start with
    let resp = sdk.psql.read_connection(MOUNT_PATH).await.unwrap();
    assert!(resp.connection.is_none());

    // Set connection
    let resp = sdk
        .psql
        .set_connection(
            MOUNT_PATH,
            &SetConnectionParams {
                connection_url: CONNECTION_STR.to_string(),
                verify_connection: true,
                max_open_connections: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.connection.connection_url, CONNECTION_STR);

    // Create a role
    let role_name = create_role(&sdk).await;

    // Create credentials
    let secret_lease_resp = sdk
        .psql
        .create_credentials(MOUNT_PATH, &role_name, None)
        .await
        .unwrap();

    // Try to connect with credentials
    let mut connection = sqlx::PgConnection::connect(&format!(
        "postgresql://{}:{}@127.0.0.1:5432/postgres?sslmode=disable",
        secret_lease_resp.data.username, secret_lease_resp.data.password
    ))
    .await
    .unwrap();
    let resp = sqlx::query("SELECT * FROM information_schema.tables")
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(resp.rows_affected() > 0);

    // Revoke lease
    sdk.lease.revoke(&secret_lease_resp.lease_id).await.unwrap();

    // Connection does not work anymore
    let resp = sqlx::PgConnection::connect(&format!(
        "postgresql://{}:{}@127.0.0.1:5432/postgres?sslmode=disable",
        secret_lease_resp.data.username, secret_lease_resp.data.password
    ))
    .await;
    assert!(resp.is_err());
}

#[tokio::test]
#[cfg_attr(not(feature = "psql-integration-test"), ignore)]
async fn restore_connnection_after_seal() {
    let tmpdir = tempfile::tempdir().unwrap();
    let storage_path = tmpdir.path().to_str().unwrap().to_string();

    let sdk = setup(&storage_path).await;

    let shares = match sdk
        .operator
        .initialize(&InitializeParams {
            shares: 1,
            threshold: 1,
        })
        .await
        .unwrap()
    {
        InitializeResponse::NewKeyShares(shares) => shares.shares,
        _ => panic!("should get new shares"),
    };
    let resp = sdk
        .operator
        .unseal(&UnsealParams {
            shares: shares.clone(),
        })
        .await
        .unwrap();
    if let UnsealResponse::Complete { root_token } = resp {
        sdk.set_token(Some(root_token.to_string())).await;
    }

    // Setup mount
    sdk.mount
        .create(
            MOUNT_PATH,
            &CreateMountParams {
                variant: BackendType::Postgres,
                config: MountConfig::default(),
            },
        )
        .await
        .unwrap();

    // No connection to start with
    let resp = sdk.psql.read_connection(MOUNT_PATH).await.unwrap();
    assert!(resp.connection.is_none());

    // Set connection
    let resp = sdk
        .psql
        .set_connection(
            MOUNT_PATH,
            &SetConnectionParams {
                connection_url: CONNECTION_STR.to_string(),
                verify_connection: true,
                max_open_connections: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.connection.connection_url, CONNECTION_STR);

    // Create a role
    let role_name = create_role(&sdk).await;

    sdk.operator.seal().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    sdk.operator.unseal(&UnsealParams { shares }).await.unwrap();

    // Create credentials after unseal
    let secret_lease_resp = sdk
        .psql
        .create_credentials(MOUNT_PATH, &role_name, None)
        .await
        .unwrap();

    // Try to connect with credentials
    let mut connection = sqlx::PgConnection::connect(&format!(
        "postgresql://{}:{}@127.0.0.1:5432/postgres?sslmode=disable",
        secret_lease_resp.data.username, secret_lease_resp.data.password
    ))
    .await
    .unwrap();
    let resp = sqlx::query("SELECT * FROM information_schema.tables")
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(resp.rows_affected() > 0);

    // Revoke lease
    sdk.lease.revoke(&secret_lease_resp.lease_id).await.unwrap();
}
