# Generate dynamic PostgreSQL credentials

## Setup PostgreSQL database
```sh
docker pull postgres:latest

docker run \
          --detach \
          --name covert-experiment \
          -e POSTGRES_USER=root \
          -e POSTGRES_PASSWORD=rootpassword \
          -p 5432:5432 \
          --rm \
          postgres
```

## Unseal Covert

```sh
covert operator init --shares 1 --threshold 1
covert operator unseal --unseal-keys "<key1>"
# Export the root token received after unseal to your environment
export COVERT_TOKEN=<TOKEN>
```

## Configure PostgreSQL secret engine
```sh
# Enable the postgres secrets engine at path "psql/"
covert secrets enable psql --path psql/

# Set connection string for the secrets engine
covert psql set-connection psql/ --connection-url "postgresql://root:rootpassword@127.0.0.1:5432/postgres?sslmode=disable"

# Add a role called "foo" with the given sql creation and revocation commands.
covert psql add-role --name foo --path psql/ --sql "CREATE ROLE \"{{name}}\" WITH LOGIN PASSWORD '{{password}}' VALID UNTIL '{{expiration}}' INHERIT;GRANT SELECT ON ALL TABLES IN SCHEMA public TO \"{{name}}\"" --revocation-sql "DROP ROLE \"{{name}}\""
```

## Manage dynamic credentials
```sh
# Generate credentials for role "foo"
covert psql creds --name foo --path psql/

# Sign in to db with credentials
psql "postgresql://<username>:<password>@127.0.0.1:5432/postgres?sslmode=disable"

# Lookup leases for the secret engine
covert lease list-mount psql

# Revoke a lease
covert lease revoke <LEASE_ID>

# Revoke all leases for a secret engine
covert lease revoke-mount psql/

# Signing in to db no longer works
psql "postgresql://<username>:<password>@127.0.0.1:5432/postgres?sslmode=disable"
```

## Disable secret engine
```sh
# Generate credentials
covert psql creds --name foo --path psql/

# Disable engine at path "psql/"
covert secrets disable psql/

# Signing in to db no longer works as all leases have been revoked
psql "postgresql://<username>:<password>@127.0.0.1:5432/postgres?sslmode=disable"
```