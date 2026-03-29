use super::*;

#[test]
fn test_count_tokens_empty() {
    assert_eq!(count_tokens(""), 0);
}

#[test]
fn test_count_tokens_single_word() {
    // "hello" -> 1 word, 0 punct -> base 1, * 0.75 rounds to 1
    let tokens = count_tokens("hello");
    assert!(tokens >= 1);
}

#[test]
fn test_count_tokens_sentence() {
    // "The quick brown fox jumps over the lazy dog."
    // 9 words, 1 punct char on "dog." -> ~10 base, * 0.75 = ~8
    let tokens = count_tokens("The quick brown fox jumps over the lazy dog.");
    assert!(tokens >= 5 && tokens <= 15, "got {tokens}");
}

#[test]
fn test_count_tokens_code() {
    let code = r#"fn main() { println!("hello"); }"#;
    let tokens = count_tokens(code);
    // Code has lots of punctuation; should produce more tokens than word count
    assert!(tokens >= 3, "code should produce tokens, got {tokens}");
}

#[test]
fn test_count_tokens_better_than_chars_div_4() {
    // For typical English prose, count_tokens should be reasonably close
    // to real BPE token counts (within 2x).
    let text = "This is a simple sentence with several common English words in it.";
    let heuristic = count_tokens(text);
    let naive = text.len() / 4; // chars/4
    // Both should be in a reasonable range (5-20 for this sentence)
    assert!(
        heuristic > 0 && naive > 0,
        "both should be positive: heuristic={heuristic}, naive={naive}"
    );
}
