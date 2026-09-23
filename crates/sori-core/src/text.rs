//! Small text utilities: word counting and LLM output guards.

/// Words for stats. Whitespace-separated tokens (Korean 어절, English words); Chinese and
/// Japanese characters count one word each.
pub fn count_words(s: &str) -> u32 {
    let mut n = 0u32;
    for tok in s.split_whitespace() {
        let cjk = tok.chars().filter(|c| is_han_or_kana(*c)).count() as u32;
        n += if cjk > 0 { cjk.max(1) } else { 1 };
    }
    n
}

fn is_han_or_kana(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF)
}

/// Strip code fences / wrapper tags / surrounding quotes some models add.
pub fn clean_llm_output(s: &str) -> String {
    let mut t = s.trim().to_string();
    if t.starts_with("```") {
        if let Some(nl) = t.find('\n') {
            t = t[nl + 1..].to_string();
        }
        if let Some(end) = t.rfind("```") {
            t.truncate(end);
        }
        t = t.trim().to_string();
    }
    for tag in ["transcript", "output", "text", "result"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if t.starts_with(&open) && t.ends_with(&close) {
            t = t[open.len()..t.len() - close.len()].trim().to_string();
        }
    }
    t
}

/// Heuristic from open-source dictation apps: detect when the model *answered* the
/// transcript instead of cleaning it. Returns true when the output should be rejected in
/// favour of the raw transcript.
pub fn looks_like_assistant_reply(raw: &str, output: &str) -> bool {
    let o = output.trim_start().to_lowercase();
    let r = raw.to_lowercase();
    const OPENERS: &[&str] = &[
        "sure", "certainly", "here's", "here is", "of course", "i'd be happy", "as an ai",
        "물론", "네, ", "알겠습니다", "다음은", "정리해 드리", "정리하면 다음",
    ];
    if OPENERS.iter().any(|p| o.starts_with(p) && !r.trim_start().starts_with(p)) {
        return true;
    }
    // Output wildly longer than input → the model generated new content.
    let (rl, ol) = (raw.chars().count(), output.chars().count());
    rl > 0 && ol > rl * 3 + 80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(count_words("안녕하세요 반갑습니다 Sori입니다"), 3);
        assert_eq!(count_words("hello world"), 2);
    }

    #[test]
    fn guard() {
        assert!(looks_like_assistant_reply("이거 어떻게 생각해?", "물론입니다! 제 생각에는..."));
        assert!(!looks_like_assistant_reply("음 내일 보자", "내일 보자"));
    }

    #[test]
    fn strips_fences() {
        assert_eq!(clean_llm_output("```\nhi\n```"), "hi");
        assert_eq!(clean_llm_output("<output>hi</output>"), "hi");
    }
}
