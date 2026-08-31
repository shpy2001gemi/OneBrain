//! Unicode-safe display helpers. These functions never rewrite canonical text.

use unicode_segmentation::UnicodeSegmentation;

pub fn truncate_preview(input: &str, max_graphemes: usize) -> String {
    let graphemes = input.graphemes(true).collect::<Vec<_>>();
    if graphemes.len() <= max_graphemes {
        return input.to_owned();
    }
    if max_graphemes <= 3 {
        return ".".repeat(max_graphemes);
    }
    format!("{}...", graphemes[..max_graphemes - 3].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_preserve_extended_grapheme_clusters() {
        let samples = [
            "Tiếng Việt",
            "Tie\u{302}\u{301}ng Vie\u{323}\u{302}t",
            "漢字仮名",
            "👩🏽‍💻",
            "🇻🇳",
            "👨‍👩‍👧‍👦",
        ];
        for sample in samples {
            assert_eq!(truncate_preview(sample, 0), "");
            let one = truncate_preview(sample, 1);
            let expected = if sample.graphemes(true).count() == 1 {
                sample
            } else {
                "."
            };
            assert_eq!(one, expected);
            assert!(std::str::from_utf8(one.as_bytes()).is_ok());
        }
    }

    #[test]
    fn exact_77_and_80_grapheme_boundaries_are_stable() {
        let cluster = "👩🏽‍💻";
        for boundary in [1, 77, 80] {
            let exact = cluster.repeat(boundary);
            assert_eq!(truncate_preview(&exact, boundary), exact);
            let longer = cluster.repeat(boundary + 1);
            let retained = boundary.saturating_sub(3);
            assert_eq!(
                truncate_preview(&longer, boundary),
                if boundary <= 3 {
                    ".".repeat(boundary)
                } else {
                    format!("{}...", cluster.repeat(retained))
                }
            );
        }
    }
}
