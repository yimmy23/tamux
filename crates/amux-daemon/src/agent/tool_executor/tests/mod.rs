use super::*;

include!("part1.rs");
include!("part2.rs");
include!("part3.rs");
mod part4;
include!("part5.rs");
include!("part6.rs");
include!("part7.rs");
include!("part8.rs");
include!("part9.rs");
include!("part10.rs");

fn current_dir_test_lock() -> &'static std::sync::Mutex<()> {
    crate::test_support::env_test_mutex()
}
