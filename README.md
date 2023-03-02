# Covert secrets management

[![CI](https://github.com/fmeringdal/covert/actions/workflows/ci.yml/badge.svg)](https://github.com/fmeringdal/covert/actions/workflows/ci.yml)

Covert is a secrets management solution written in Rust that leverages [SQLCipher](https://github.com/sqlcipher/sqlcipher) for encrypted storage and [Litestream](https://github.com/benbjohnson/litestream) for replication. Covert has a very similar API to [HashiCorp Vault](https://github.com/hashicorp/vault) and takes a lot of inspiration from it, but aims to be a simpler and more affordable option. Some of the features included are:

- Versioned Key-Value secrets
- Dynamic secrets (only PostgreSQL currently)
- Namespaces
- Streaming replication
- Type safe and flexible framework for writing new secrets engines and authentication methods

**NOTE**: This is a experimental software which is not yet suitable for production use-cases.

## Getting started

Install Covert with [Cargo](https://doc.rust-lang.org/cargo/getting-started/index.html)
```sh
cargo install covert
```

Start the Covert server
```sh
covert server --config ./config.example.toml
```

Check out some of the examples in the [examples folder](./examples/).
