include!("file_preview_parts/scroll_offset_from_thumb_offset_to_file_preview_cache_key.rs");
include!("file_preview_parts/syntax_highlighting.rs");
include!("file_preview_parts/build_cached_lines_to_scrollbar_layout.rs");
#[cfg(test)]
mod tests {
    include!("file_preview_tests_parts/cached_snapshot_reuses_built_lines_for_same_preview_input_to_git_diff.rs");
}
