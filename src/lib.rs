#[cfg(all(feature = "test-adapters", not(debug_assertions)))]
compile_error!("the test-adapters feature cannot be included in release builds");

pub mod adapters;
pub mod brief;
pub mod cli;
pub mod completions;
pub mod contract;
pub mod credential;
pub mod doctor;
pub mod error;
pub mod model;
pub mod policy;
pub mod repl;
pub mod run;
pub mod schema;
pub mod store;
pub mod ui;
pub mod util;
