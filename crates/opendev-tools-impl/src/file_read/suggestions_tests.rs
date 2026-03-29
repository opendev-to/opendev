use super::*;

#[test]
fn test_levenshtein_identical() {
    assert_eq!(levenshtein("hello", "hello"), 0);
}

#[test]
fn test_levenshtein_empty() {
    assert_eq!(levenshtein("", "abc"), 3);
    assert_eq!(levenshtein("abc", ""), 3);
    assert_eq!(levenshtein("", ""), 0);
}

#[test]
fn test_levenshtein_transposition() {
    // "flie" vs "file" = 2 (swap i and l requires delete + insert)
    assert_eq!(levenshtein("flie", "file"), 2);
}

#[test]
fn test_levenshtein_single_edit() {
    assert_eq!(levenshtein("cat", "car"), 1); // substitution
    assert_eq!(levenshtein("cat", "cats"), 1); // insertion
    assert_eq!(levenshtein("cats", "cat"), 1); // deletion
}

#[test]
fn test_levenshtein_completely_different() {
    assert_eq!(levenshtein("abc", "xyz"), 3);
}
