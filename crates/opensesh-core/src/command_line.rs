//! Command lines as text (PLAN Sprint 12): a shell to run (`wsl.exe -d Ubuntu`,
//! `"C:\Program Files\Git\bin\bash.exe" --login -i`), split into a program and its arguments.
//! Whitespace separates words; single or double quotes keep what they enclose together, with no
//! escapes, so Windows paths keep their backslashes (to put a quote inside a word, quote it with
//! the other kind).

/// Errors of [`split`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    /// A quote isn't closed.
    #[error("a {0} quote isn't closed")]
    Unclosed(char),
    /// Nothing to run.
    #[error("the command line is empty")]
    Empty,
}

/// The words of a command line (see the module documentation).
///
/// # Errors
///
/// An unclosed quote, or no words.
pub fn split(line: &str) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if let Some(open) = quote {
        return Err(SplitError::Unclosed(open));
    }
    if in_word {
        words.push(word);
    }
    if words.is_empty() {
        return Err(SplitError::Empty);
    }
    Ok(words)
}

/// A command line that [`split`] turns back into `words`.
#[must_use]
pub fn join<S: AsRef<str>>(words: &[S]) -> String {
    words
        .iter()
        .map(|word| {
            let word = word.as_ref();
            if !word.is_empty()
                && !word.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            {
                word.to_owned()
            } else if word.contains('"') {
                format!("'{word}'")
            } else {
                format!("\"{word}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn command_lines_split_and_join() {
        assert_eq!(split("bash").unwrap(), ["bash"]);
        assert_eq!(
            split(r#""C:\Program Files\Git\bin\bash.exe" --login -i"#).unwrap(),
            [r"C:\Program Files\Git\bin\bash.exe", "--login", "-i"]
        );
        assert_eq!(
            split("  wsl.exe   -d 'Ubuntu 24.04' ").unwrap(),
            ["wsl.exe", "-d", "Ubuntu 24.04"]
        );
        assert_eq!(
            split(r#"sh -c 'echo "hi"'"#).unwrap(),
            ["sh", "-c", r#"echo "hi""#]
        );
        assert_eq!(split(r#"a"b c"d"#).unwrap(), ["ab cd"]);
        assert_eq!(split(r#"x """#).unwrap(), ["x", ""]);
        assert_eq!(split("'open").unwrap_err(), SplitError::Unclosed('\''));
        assert_eq!(split("   ").unwrap_err(), SplitError::Empty);
        for words in [
            vec![r"C:\Program Files\PowerShell\7\pwsh.exe"],
            vec!["wsl.exe", "-d", "Ubuntu 24.04"],
            vec!["sh", "-c", r#"echo "hi""#],
            vec!["x", ""],
        ] {
            assert_eq!(split(&join(&words)).unwrap(), words, "{}", join(&words));
        }
    }
}
