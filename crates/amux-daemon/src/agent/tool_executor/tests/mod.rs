use super::*;
use std::sync::OnceLock;

include!("part1.rs");
include!("part2.rs");
include!("part3.rs");
mod part4;
include!("part5.rs");
include!("part6.rs");
include!("part7.rs");

fn current_dir_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
