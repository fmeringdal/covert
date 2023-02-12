#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

pub mod auth;
pub mod backend;
pub mod entity;
pub mod error;
pub mod methods;
pub mod mount;
pub mod policy;
pub mod psql;
pub mod request;
pub mod response;
pub mod state;
pub mod token;
