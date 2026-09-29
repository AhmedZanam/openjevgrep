pub mod backend;
pub mod cache;
pub mod chunk;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod eval;
pub mod exact;
pub mod mcp;
pub mod output;
pub mod retrieval;
pub mod scanner;
pub mod search;

pub use error::{Error, Result};
