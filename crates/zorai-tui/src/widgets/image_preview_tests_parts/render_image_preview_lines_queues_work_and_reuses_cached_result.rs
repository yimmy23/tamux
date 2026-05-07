    use super::super::*;

    fn plain_lines(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn render_image_preview_lines_queues_work_and_reuses_cached_result() {
        let runtime = PreviewRuntime::new();
        let path =
            std::env::temp_dir().join(format!("zorai-image-preview-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_fn(128, 128, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
        })
        .save(&path)
        .expect("fixture PNG should write");

        let initial = render_image_preview_lines_with_runtime(
            &runtime,
            path.to_str().expect("temp path should be valid UTF-8"),
            32,
            12,
            &ThemeTokens::default(),
        );
        let initial_plain = plain_lines(&initial);
        assert!(
            initial_plain
                .iter()
                .any(|line| line.contains("Loading image preview...")),
            "expected first render to avoid synchronous decode and show a loading state, got {initial_plain:?}"
        );
        assert_eq!(runtime.queue_len_for_tests(), 1);

        let pending_again = render_image_preview_lines_with_runtime(
            &runtime,
            path.to_str().expect("temp path should be valid UTF-8"),
            32,
            12,
            &ThemeTokens::default(),
        );
        let pending_plain = plain_lines(&pending_again);
        assert!(
            pending_plain
                .iter()
                .any(|line| line.contains("Loading image preview...")),
            "expected pending renders to keep using the queued loading state, got {pending_plain:?}"
        );
        assert_eq!(runtime.queue_len_for_tests(), 1);

        assert!(runtime.process_next_job_for_tests());

        let ready = render_image_preview_lines_with_runtime(
            &runtime,
            path.to_str().expect("temp path should be valid UTF-8"),
            32,
            12,
            &ThemeTokens::default(),
        );
        let ready_plain = plain_lines(&ready);
        assert!(
            ready_plain[0].contains("(128x128)"),
            "expected cached preview header to include source dimensions, got {ready_plain:?}"
        );
        assert!(
            !ready_plain
                .iter()
                .any(|line| line.contains("Loading image preview...")),
            "expected ready render to use cached preview data, got {ready_plain:?}"
        );
        assert!(
            ready
                .iter()
                .skip(1)
                .any(|line| line.spans.iter().any(|span| span.content.as_ref() == "▀")),
            "expected cached preview to contain rendered image rows"
        );
    }
