#![allow(dead_code)]

#[path = "modal_parts/default_to_last_error.rs"]
mod default_to_last_error;

#[path = "modal_parts/default_command_items.rs"]
mod default_command_items;

pub use default_to_last_error::*;

#[cfg(test)]
#[path = "tests/modal.rs"]
mod tests;
