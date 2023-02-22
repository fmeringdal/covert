# Covert secrets management

[![CI](https://github.com/fmeringdal/covert/actions/workflows/ci.yml/badge.svg)](https://github.com/fmeringdal/covert/actions/workflows/ci.yml)

## What

Covert is a (experimental) secrets management solution written in Rust, mostly a rewrite of [HashiCorp Vault](https://github.com/hashicorp/vault).

## Getting started

Install Covert with [Cargo](https://doc.rust-lang.org/cargo/getting-started/index.html)
```sh
cargo install covert
```

Start the Covert server in dev mode
```sh
covert server
```

Check out some of the examples in the [examples folder](./examples/).