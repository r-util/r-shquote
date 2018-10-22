//! POSIX Shell Compatible Argument Parser
//!
//! This crate implements POSIX Shell compatible `quote` and `unquote` operations. These allow to
//! quote arbitrary strings so they are not interpreted by a shell if taken as input. In the same
//! way it allows unquoting these strings to get back the original input.
//!
//! The way this quoting works is mostly standardized by POSIX. However, many existing
//! implementations support additional features. These are explicitly not supported by this crate,
//! and it is not the intention of this crate to support these quirks and peculiarities.
//!
//! The basic operations provided are [`quote()`] and [`unquote()`], which both take a UTF-8
//! string as input, and produce the respective output string.
//!
//! # Examples
//!
//! ```
//! let str = "Hello World!";
//!
//! println!("Quoted input: {}", r_shquote::quote(str));
//! ```
//!
//! Unquote operations can fail when the input is not well defined. The returned error contains
//! diagnostics to identify where exactly the parser failed:
//!
//! ```
//! let quote = "'foobar";
//! let res = r_shquote::unquote(quote).unwrap_err();
//!
//! println!("Unquote operation failed: {}", res);
//! ```
//!
//! Combining the quote and unquote operation always produces the original input:
//!
//! ```
//! let str = "foo bar";
//!
//! assert_eq!(str, r_shquote::unquote(&r_shquote::quote(str)).unwrap());
//! ```

/// Error information for unquote operations
///
/// This error contains diagnostics from an unquote-operation. In particular, it contains the
/// character and byte offsets of the cursor where the error originated.
///
/// # Examples
///
/// ```
/// let quote = "'Hello' 'World!";
/// let res = r_shquote::unquote(quote).unwrap_err();
///
/// match res {
///     r_shquote::UnquoteError::UnterminatedSingleQuote { char_cursor: x, .. } |
///     r_shquote::UnquoteError::UnterminatedDoubleQuote { char_cursor: x, .. } => {
///         println!("Input: {}", quote);
///         println!("       {}^--- unterminated quote", " ".repeat(x));
///     },
/// }
/// ```
#[derive(Debug, Clone)]
pub enum UnquoteError {
    UnterminatedSingleQuote {
        char_cursor: usize,
        byte_cursor: usize,
    },
    UnterminatedDoubleQuote {
        char_cursor: usize,
        byte_cursor: usize,
    },
}

impl std::fmt::Display for UnquoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for UnquoteError { }

/// Quote string
///
/// This takes a string and quotes it according to POSIX Shell rules. The result can be passed to
/// POSIX compatible shells and it will be interpreted as a single token. The [`unquote()`]
/// operation implements the inverse.
///
/// Note that there is no canonical way to quote strings. There are infinite ways to quote a
/// string. This implementation always quotes using sequences of single-quotes. This mimics what a
/// lot of other implementations do. Furthermore, redundant quotes may be added, even thought a
/// shorter output would be possible. This is again done to stay compatible with other existing
/// implementations and make comparisons easier. Nevertheless, a caller must never try to predict
/// the possible escaping and quoting done by this function.
///
/// # Examples
///
/// ```
/// assert_eq!(r_shquote::quote("foobar"), "'foobar'");
/// ```
pub fn quote(source: &str) -> String {
    // This is far from perfect and produces many overly verbose results, for instance:
    //   `'` => `''\'''`
    //   `` => `''`
    //   ...
    // However, this is done purposefully to make the behavior more inline with other
    // implementations, and at the same time keep the implementation simple. If an optimized
    // version is requested, we can always provide alternatives.

    let mut acc = String::with_capacity(source.len() + 2);
    let mut parts = source.split('\'');

    acc.push('\'');

    if let Some(part) = parts.next() {
            acc.push_str(part);
    }

    parts.fold(&mut acc, |acc, part| {
        acc.push_str("\'\\\'\'");
        acc.push_str(part);
        acc
    });

    acc.push('\'');
    acc
}

fn unquote_open_single(acc: &mut String, cursor: &mut std::iter::Enumerate<std::str::CharIndices>) -> bool {
    // This decodes a single-quote sequence. The opening single-quote was already parsed by
    // the caller. Both `&source[start]` and `cursor` point to the first character following
    // the opening single-quote.
    // Anything inside the single-quote sequence is copied verbatim to the output until the
    // next single-quote. No escape sequences are supported, not even a single-quote can be
    // escaped. However, if the sequence is not terminated, the entire operation is considered
    // invalid.
    for i in cursor {
        match i {
            (_, (_, c)) if c == '\''    => return true,
            (_, (_, c))                 => acc.push(c),
        }
    }

    false
}

fn unquote_open_double(acc: &mut String, cursor: &mut std::iter::Enumerate<std::str::CharIndices>) -> bool {
    // This decodes a double-quote sequence. The opening double-quote was already parsed by
    // the caller. Both `&source[start]` and `cursor` point to the first character following
    // the opening double-quote.
    // A double-quote sequence allows escape-sequences and goes until the closing
    // double-quote. If the sequence is not terminated, though, the entire operation is
    // considered invalid.
    loop {
        match cursor.next() {
            Some((_, (_, inner_ch))) if inner_ch == '"' => {
                // An unescaped double-quote character terminates the double-quote sequence.
                // It produces no output.
                return true;
            },
            Some((_, (_, inner_ch))) if inner_ch == '\\' => {
                // Inside a double-quote sequence several escape sequences are allowed. In
                // general, any unknown sequence is copied verbatim in its entirety including
                // the backslash. Known sequences produce the escaped character in its output
                // and makes the parser not interpret it. If the sequence is non-terminated,
                // it implies that the double-quote sequence is non-terminated and thus
                // invokes the same behavior, meaning the entire operation is refused.
                match cursor.next() {
                    Some((_, (_, esc_ch))) if esc_ch == '"'  ||
                                              esc_ch == '\\' ||
                                              esc_ch == '`'  ||
                                              esc_ch == '$'  ||
                                              esc_ch == '\n' => {
                        acc.push(esc_ch);
                    },
                    Some((_, (_, esc_ch))) => {
                        acc.push('\\');
                        acc.push(esc_ch);
                    },
                    None => {
                        return false;
                    },
                }
            },
            Some ((_, (_, inner_ch))) => {
                // Any non-special character inside a double-quote is copied
                // literally just like characters outside of it.
                acc.push(inner_ch);
            },
            None => {
                // The double-quote sequence was not terminated. The entire
                // operation is considered invalid and we have to refuse producing
                // any resulting value.
                return false;
            },
        }
    }
}

fn unquote_open_escape(acc: &mut String, cursor: &mut std::iter::Enumerate<std::str::CharIndices>) {
    // This decodes an escape sequence outside of any quote. The opening backslash was already
    // parsed by the caller. Both `&source[start]` and `cursor` point to the first character
    // following the opening backslash.
    // Outside of quotes, an escape sequence simply treats the next character literally, and
    // does not interpret it. The exceptions are literal <NL> (newline charcater) and a single
    // backslash as last character in the string. In these cases the escape-sequence is
    // stripped and produces no output. The <NL> case is a remnant of human shell input, where
    // you can input multiple lines by appending a backslash to the previous line. This causes
    // both the backslash and <NL> to be ignore, since they purely serve readability of user
    // input.
    if let Some((_, (_, esc_ch))) = cursor.next() {
        if esc_ch != '\n' {
            acc.push(esc_ch);
        }
    }
}

/// Unquote String
///
/// Unquote a single string according to POSIX Shell quoting and escaping rules. If the input
/// string is not a valid input, the operation will fail and provide diagnosis information on
/// where the first invalid part was encountered.
///
/// The result is canonical. There is only one valid unquoted result for a given input.
///
/// # Examples
///
/// ```
/// assert_eq!(r_shquote::unquote("foobar").unwrap(), "foobar");
/// ```
pub fn unquote(source: &str) -> Result<String, UnquoteError> {
    // An unquote-operation never results in a longer string. Furthermore, the common case is
    // most of the string is unquoted / unescaped. Hence, we simply allocate the same space
    // for the resulting string as the input.
    let mut acc = String::with_capacity(source.len());

    // We loop over the string. When a single-quote, double-quote, or escape sequence is
    // opened, we let out helpers parse the sub-strings. Anything else is copied over
    // literally until the end of the line.
    let mut cursor = source.char_indices().enumerate();
    loop {
        match cursor.next() {
            Some((next_idx, (next_pos, next_ch))) if next_ch == '\'' => {
                if !unquote_open_single(&mut acc, &mut cursor) {
                    break Err(
                        UnquoteError::UnterminatedSingleQuote {
                            char_cursor: next_idx,
                            byte_cursor: next_pos,
                        }
                    );
                }
            },
            Some((next_idx, (next_pos, next_ch))) if next_ch == '"' => {
                if !unquote_open_double(&mut acc, &mut cursor) {
                    break Err(
                        UnquoteError::UnterminatedDoubleQuote {
                            char_cursor: next_idx,
                            byte_cursor: next_pos,
                        }
                    );
                }
            },
            Some((_, (_, next_ch))) if next_ch == '\\' => {
                unquote_open_escape(&mut acc, &mut cursor);
            },
            Some((_, (_, next_ch))) => {
                acc.push(next_ch);
            },
            None => {
                break Ok(acc);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(quote("foobar"), "'foobar'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("'"), "''\\'''");

        assert_eq!(unquote("foobar").unwrap(), "foobar");
        assert_eq!(unquote("foo'bar'").unwrap(), "foobar");
        assert_eq!(unquote("foo\"bar\"").unwrap(), "foobar");
        assert_eq!(unquote("\\foobar\\").unwrap(), "foobar");
        assert_eq!(unquote("\\'foobar\\'").unwrap(), "'foobar'");
    }
}
