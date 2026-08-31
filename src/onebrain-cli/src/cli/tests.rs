#[cfg(test)]
mod cases {
    use crate::cli::helpers::*;

    // ── format_timestamp ─────────────────────────────────────────────────

    #[test]
    fn test_format_timestamp_zero() {
        assert_eq!(format_timestamp(0), "--");
    }

    #[test]
    fn test_format_timestamp_recent() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let result = format_timestamp(now - 30);
        assert!(
            result.ends_with("s ago"),
            "Expected seconds ago, got: {}",
            result
        );
    }

    #[test]
    fn test_format_timestamp_minutes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let result = format_timestamp(now - 300);
        assert!(
            result.ends_with("m ago"),
            "Expected minutes ago, got: {}",
            result
        );
    }

    // ── format_obt ───────────────────────────────────────────────────────

    #[test]
    fn test_format_obt_zero() {
        assert_eq!(format_obt(0), "0.000 OBT");
    }

    #[test]
    fn test_format_obt_small() {
        assert_eq!(format_obt(500), "0.500 OBT");
    }

    #[test]
    fn test_format_obt_thousands() {
        // 1500 milliOBT = 1.500 OBT
        assert_eq!(format_obt(1500), "1.500 OBT");
    }

    #[test]
    fn test_format_obt_millions() {
        // 1_500_000 milliOBT = 1,500.000 OBT
        assert_eq!(format_obt(1_500_000), "1,500.000 OBT");
    }

    #[test]
    fn test_format_obt_billions() {
        // 1_000_000_000 milliOBT = 1,000,000.000 OBT
        assert_eq!(format_obt(1_000_000_000), "1,000,000.000 OBT");
    }

    #[test]
    fn test_format_obt_large_with_fraction() {
        // 1_234_567_890 milliOBT = 1,234,567.890 OBT
        assert_eq!(format_obt(1_234_567_890), "1,234,567.890 OBT");
    }

    // ── format_obt_short ─────────────────────────────────────────────────

    #[test]
    fn test_format_obt_short_zero() {
        assert_eq!(format_obt_short(0), "0.000 OBT");
    }

    #[test]
    fn test_format_obt_short_value() {
        assert_eq!(format_obt_short(12_345), "12.345 OBT");
    }

    // ── format_obt_signed ────────────────────────────────────────────────

    #[test]
    fn test_format_obt_signed_positive() {
        assert_eq!(format_obt_signed(1500), "+1.500 OBT");
    }

    #[test]
    fn test_format_obt_signed_negative() {
        assert_eq!(format_obt_signed(-1500), "-1.500 OBT");
    }

    #[test]
    fn test_format_obt_signed_zero() {
        assert_eq!(format_obt_signed(0), "+0.000 OBT");
    }

    // ── bar_chart ────────────────────────────────────────────────────────

    #[test]
    fn test_bar_chart_full() {
        let chart = bar_chart(100, 100, 10);
        assert_eq!(chart, "██████████");
    }

    #[test]
    fn test_bar_chart_half() {
        let chart = bar_chart(50, 100, 10);
        assert_eq!(chart, "█████░░░░░");
    }

    #[test]
    fn test_bar_chart_empty() {
        let chart = bar_chart(0, 100, 10);
        assert_eq!(chart, "░░░░░░░░░░");
    }

    #[test]
    fn test_bar_chart_zero_max() {
        let chart = bar_chart(50, 0, 10);
        assert_eq!(chart, "░░░░░░░░░░");
    }

    // ── short_cid ────────────────────────────────────────────────────────

    #[test]
    fn test_short_cid_long() {
        assert_eq!(short_cid("abcdef1234567890"), "abcdef12");
    }

    #[test]
    fn test_short_cid_exact() {
        assert_eq!(short_cid("abcdef12"), "abcdef12");
    }

    #[test]
    fn test_short_cid_short() {
        assert_eq!(short_cid("abc"), "abc");
    }

    // ── chrono_timestamp ─────────────────────────────────────────────────

    #[test]
    fn test_chrono_timestamp_format() {
        let ts = chrono_timestamp();
        // Should be in YYYYMMDD_HHMMSS format (15 chars)
        assert_eq!(
            ts.len(),
            15,
            "Expected 15 chars (YYYYMMDD_HHMMSS), got: {} (len={})",
            ts,
            ts.len()
        );
        assert_eq!(
            &ts[8..9],
            "_",
            "Expected underscore at position 8, got: {}",
            ts
        );
        // Year should start with 20
        assert!(
            ts.starts_with("20"),
            "Expected year starting with 20, got: {}",
            ts
        );
    }

    // ── format_size ──────────────────────────────────────────────────────

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(2048), "2.0 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(5_242_880), "5.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(2_147_483_648), "2.00 GB");
    }

    // ── truncate_str ─────────────────────────────────────────────────────

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let result = truncate_str("hello world this is long", 10);
        assert!(
            result.ends_with("..."),
            "Expected truncated with ..., got: {}",
            result
        );
        assert!(
            result.chars().count() <= 10,
            "Expected at most 10 chars, got: {}",
            result
        );
    }

    // ── count_graph_items ────────────────────────────────────────────────

    #[test]
    fn test_count_graph_items_empty() {
        assert_eq!(count_graph_items(&[]), 0);
    }
}
