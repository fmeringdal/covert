# Generate dynamic PostgreSQL credentials

## Setup database
```sh
docker pull postgres:latest

docker run \
          --detach \
          --name learn-postgres \
          -e POSTGRES_USER=root \
          -e POSTGRES_PASSWORD=rootpassword \
          -p 5432:5432 \
          --rm \
          postgres
docker ps -f name=learn-postgres --format "table {{.Names}}\t{{.Status}}"

docker exec -i \
          learn-postgres \
          psql -U root -c "CREATE ROLE \"ro\" NOINHERIT;"

docker exec -i \
          learn-postgres \
          psql -U root -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO \"ro\";"
```

## Unseal Covert

```sh
covert operator init
covert operator unseal --unseal-keys "<key1>,<key2>,<key3>"
```

## Configure PostgreSQL secret engine
```sh
covert secrets enable -n psql -p psql/

covert psql add-role --name foo --mount psql/ --sql "CREATE ROLE \"{{name}}\" WITH LOGIN PASSWORD '{{password}}' VALID UNTIL '{{expiration}}' INHERIT;GRANT ro TO \"{{name}}\"" --revocation-sql "DROP ROLE \"{{name}}\""

covert psql set-connection --conection-url "postgresql://root:rootpassword@127.0.0.1:5432/postgres?sslmode=disable" --mount psql/
```

## Manage dynamic credentials
```sh
# Generate credentials
covert psql creds --name foo --mount psql/

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
covert psql creds --name foo --mount psql/

# Disable engine
covert secrets disable -p psql/

# Signing in to db no longer works as all leases has been revoked
psql "postgresql://<username>:<password>@127.0.0.1:5432/postgres?sslmode=disable"
```