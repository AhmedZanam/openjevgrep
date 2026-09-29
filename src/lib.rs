pub mod backend;
pub mod cache;
pub mod chunk;
pub mod cli;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod error;
pub mod eval;
pub mod exact;
pub mod mcp;
pub mod model;
pub mod output;
pub mod retrieval;
pub mod scanner;
pub mod search;
pub mod server;

pub use error::{Error, Result};
