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

/// Words that carry no content (fillers, discourse markers, function words), skipped when
/// measuring how much of a transcript survived cleanup.
const NOISE: &[&str] = &[
    "음", "어", "그", "저", "뭐", "막", "좀", "이제", "약간", "그냥", "그러니까", "그니까", "지금", "보면", "이거", "그거", "저거", "이런",
    "그런", "저기", "있잖아", "아니", "아니다", "그리고", "근데", "그래서", "그러면", "그러면은", "하고", "해가지고", "가지고", "이렇게",
    "그렇게", "뭐지", "뭐라", "그럴까", "딱", "아무튼", "하여튼", "일단", "혹시", "되게", "너무", "진짜", "정말", "것", "거", "같아", "같은데",
    "um", "uh", "like", "so", "basically", "you", "know", "the", "a", "an", "and", "to", "of", "it", "is", "that", "this", "i", "we",
    "can", "just", "ok", "okay", "oh", "well", "maybe", "or", "something", "kind", "sort",
];

/// Fraction (0–1) of the source's content words that still appear in `out`: Hangul words are
/// matched by their first two syllables (particles and endings change when text is polished),
/// Latin words case-insensitively; spaces are ignored on the output side. A polished rewrite
/// of a complete transcript scores ~0.6–0.9; one that silently dropped half the requests
/// scores far lower.
pub fn coverage(source: &str, out: &str) -> f32 {
    coverage_detail(source, out).0
}

/// (coverage, number of content words in the source).
pub fn coverage_detail(source: &str, out: &str) -> (f32, u32) {
    let hay: String = out.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    let is_hangul = |c: char| ('\u{AC00}'..='\u{D7A3}').contains(&c);
    // Tech terms said in Korean sounds come back in Latin script (불칸 에스디케이 → Vulkan SDK):
    // each Latin word that is new in the output may stand in for up to two missing Hangul words.
    let src_lower = source.to_lowercase();
    let new_latin = out
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 2 && w.chars().any(|c| c.is_ascii_alphabetic()) && !src_lower.contains(&w.to_lowercase()))
        .count() as u32;
    let mut translit_credit = new_latin * 2;
    let (mut total, mut hit) = (0u32, 0u32);
    for w in source.split(|c: char| !(c.is_alphanumeric() || is_hangul(c))) {
        let w = w.to_lowercase();
        let n = w.chars().count();
        if n < 2 || NOISE.contains(&w.as_str()) || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let key: String = if w.chars().next().is_some_and(is_hangul) { w.chars().take(2).collect() } else { w.clone() };
        if NOISE.contains(&key.as_str()) {
            continue;
        }
        total += 1;
        if hay.contains(&key) {
            hit += 1;
        } else if translit_credit > 0 && key.chars().next().is_some_and(is_hangul) {
            translit_credit -= 1;
            hit += 1;
        }
    }
    (if total == 0 { 1.0 } else { hit as f32 / total as f32 }, total)
}

/// Split a long transcript into chunks of roughly `max` characters at sentence boundaries
/// (falling back to word boundaries), so a small model cleans every part instead of
/// summarizing the whole.
pub fn chunk_sentences(text: &str, max: usize) -> Vec<String> {
    let mut sentences: Vec<String> = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, '.' | '?' | '!' | '\n' | '。') {
            sentences.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        sentences.push(cur);
    }
    // Unpunctuated speech: cut long sentences at word boundaries.
    let mut pieces: Vec<String> = vec![];
    for s in sentences {
        if s.chars().count() <= max {
            pieces.push(s);
            continue;
        }
        let mut part = String::new();
        for w in s.split_whitespace() {
            if part.chars().count() + w.chars().count() > max && !part.is_empty() {
                pieces.push(std::mem::take(&mut part));
            }
            if !part.is_empty() {
                part.push(' ');
            }
            part.push_str(w);
        }
        if !part.is_empty() {
            pieces.push(part);
        }
    }
    let mut chunks: Vec<String> = vec![];
    let mut acc = String::new();
    for p in pieces {
        if !acc.is_empty() && acc.chars().count() + p.chars().count() > max {
            chunks.push(std::mem::take(&mut acc).trim().to_string());
        }
        if !acc.is_empty() && !acc.ends_with(char::is_whitespace) {
            acc.push(' ');
        }
        acc.push_str(p.trim());
    }
    if !acc.trim().is_empty() {
        chunks.push(acc.trim().to_string());
    }
    chunks
}

/// The speaker asked something but the rewrite has no question left (a question turned into
/// a request or an answer). Questions that are really polite requests — "can you check …?",
/// "좀 봐 줄래?" — may become instructions: that is the cleanup a coding-agent prompt wants.
pub fn lost_question(source: &str, out: &str) -> bool {
    if out.contains('?') || out.contains('？') {
        return false;
    }
    question_sentences(source).iter().any(|q| !is_polite_request(q))
}

fn question_sentences(text: &str) -> Vec<String> {
    let mut found = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        match c {
            '?' | '？' => found.push(std::mem::take(&mut cur)),
            '.' | '!' | '\n' | '。' => cur.clear(),
            _ => {}
        }
    }
    found
}

/// Asking someone to do something: "(um, so) can/could/would/will you …?", "… please …?",
/// "~해 줄래?", "~해 줄 수 있어?", "~해 주실래요?", "~해 주면 안 돼?".
fn is_polite_request(question: &str) -> bool {
    const FILLERS: &[&str] = &["um", "uh", "so", "and", "okay", "ok", "also", "hey", "well", "oh", "like", "then", "but", "actually"];
    let lower = question.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect();
    let mut rest = words.iter().skip_while(|w| FILLERS.contains(w));
    if let (Some(first), Some(second)) = (rest.next(), rest.next()) {
        if matches!(*first, "can" | "could" | "would" | "will") && matches!(*second, "you" | "u" | "we") {
            return true;
        }
    }
    if words.contains(&"please") {
        return true;
    }
    const KO_ENDINGS: &[&str] = &[
        "줄래", "줄래요", "줄수있어", "줄수있어요", "줄수있니", "줄수있나", "줄수있나요", "줄수있을까", "줄수있을까요",
        "줄수있겠어", "줄수있겠어요", "주겠어", "주겠어요", "주겠니", "주실래요", "주시겠어요", "주실수있나요",
        "주실수있을까요", "주실수있어요", "주면안돼", "주면안돼요", "주면안될까", "주면안될까요",
    ];
    let compact: String = question.chars().filter(|c| !c.is_whitespace() && !matches!(c, '?' | '？' | '~' | '.' | '!')).collect();
    KO_ENDINGS.iter().any(|e| compact.ends_with(e))
}

#[cfg(test)]
mod coverage_tests {
    use super::{chunk_sentences, coverage, lost_question};

    #[test]
    fn chunks_keep_all_text() {
        let t = "첫 문장이야. 두 번째 문장은 좀 더 길어서 여기까지 이어져. 세 번째? 네 번째 문장 끝.";
        let c = chunk_sentences(t, 30);
        assert!(c.len() >= 2);
        assert_eq!(c.join(" ").replace(' ', ""), t.replace(' ', ""));
        let long = "단어 ".repeat(100);
        assert!(chunk_sentences(&long, 50).iter().all(|c| c.chars().count() <= 52));
    }

    #[test]
    fn complete_rewrite_scores_high_and_truncation_low() {
        let raw = "음 일단 음성 프로필을 유저가 선택할 수 있게 메뉴 하나 만들어 줘 그리고 한번 고르면 로컬스토리지에 저장되게 하고 디폴트는 케이트로 하고 기존 음성도 옵션에 놔둬";
        let good = "음성 프로필을 선택할 수 있는 메뉴를 만들어 줘. 고른 값은 로컬스토리지에 저장하고, 디폴트는 케이트, 기존 음성도 옵션에 남겨 둬.";
        let cut = "음성 프로필을 선택할 수 있는 메뉴를 만들어 줘.";
        assert!(coverage(raw, good) > 0.6, "{}", coverage(raw, good));
        assert!(coverage(raw, cut) < 0.4, "{}", coverage(raw, cut));
    }

    #[test]
    fn tech_terms_in_latin_script_count_as_kept() {
        let raw = "빌드할 때 윈도우 쪽에서 불칸 에스디케이 없으면 에러 나는 거 씨피유로 폴백되게 하거나 리드미에 설치 가이드 넣어 줘";
        let out = "Windows에서 빌드할 때 Vulkan SDK가 없으면 에러가 나. CPU로 폴백되게 하거나 README에 설치 가이드를 넣어 줘.";
        assert!(coverage(raw, out) > 0.8, "{}", coverage(raw, out));
    }

    #[test]
    fn detects_question_turned_into_request() {
        assert!(lost_question("비밀번호를 맞게 입력해도 통과할 수 없는 거야?", "비밀번호를 맞게 입력해도 통과할 수 없게 해 줘."));
        assert!(!lost_question("비밀번호를 맞게 입력해도 통과할 수 없는 거야?", "비밀번호를 맞게 입력해도 통과할 수 없는 거야?"));
        assert!(lost_question("What does the cache reuse flag actually do?", "The cache reuse flag reuses the KV cache."));
    }

    #[test]
    fn polite_requests_may_become_instructions() {
        // Seen on Windows: every level rewrote these as instructions, all were rejected, and the
        // transcript went in with its "um"s.
        assert!(!lost_question(
            "Um, so can you check why the login test is failing? And uh, don't commit anything yet, just tell me what you find.",
            "Check why the login test is failing. Don't commit anything yet, just tell me what you find."
        ));
        assert!(!lost_question(
            "음 그 로그인 테스트 왜 실패하는지 좀 봐 줄래? 그리고 아직 커밋은 하지 말고 뭐 찾았는지만 알려 줘",
            "로그인 테스트가 왜 실패하는지 좀 봐 줘. 아직 커밋은 하지 말고 찾은 내용만 알려 줘."
        ));
        assert!(!lost_question("이거 리드미에 정리해 줄 수 있어?", "이거 README에 정리해 줘."));
        assert!(!lost_question("Could you please rename the hook?", "Rename the hook."));
        // A real question next to a request still has to stay a question.
        assert!(lost_question("Can you fix the build? And why did it break?", "Fix the build, and check why it broke."));
        assert!(lost_question("Why would you cache that?", "Don't cache that."));
    }
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
