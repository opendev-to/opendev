//! Argument expansion for skill templates.

/// Expand `$ARGUMENTS` and positional `$1`, `$2`, etc. in skill content.
///
/// - `$ARGUMENTS` is replaced with the full argument string.
/// - `$1`, `$2`, ... are replaced with positional arguments.
/// - Quoted strings (`"multi word"` or `'multi word'`) count as a single argument.
/// - If no placeholders exist, arguments are appended at the end.
pub(super) fn expand_skill_arguments(content: &str, arguments: &str) -> String {
    let positional = parse_positional_args(arguments);

    let has_arguments_placeholder = content.contains("$ARGUMENTS");
    let has_positional = content.contains("$1");

    let mut result = content.to_string();

    if has_arguments_placeholder {
        result = result.replace("$ARGUMENTS", arguments);
    }

    for (i, arg) in positional.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        result = result.replace(&placeholder, arg);
    }

    if !has_arguments_placeholder && !has_positional && !arguments.is_empty() {
        result.push_str("\n\n## Input\n\n");
        result.push_str(arguments);
        result.push('\n');
    }

    result
}

/// Parse a string into positional arguments, respecting quotes.
///
/// `"hello world" foo 'bar baz'` → `["hello world", "foo", "bar baz"]`
pub(super) fn parse_positional_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();

    while let Some(&ch) = chars.peek() {
        match ch {
            '"' | '\'' => {
                let quote = ch;
                chars.next();
                while let Some(&c) = chars.peek() {
                    if c == quote {
                        chars.next();
                        break;
                    }
                    current.push(c);
                    chars.next();
                }
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
                chars.next();
            }
            _ => {
                current.push(ch);
                chars.next();
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;
