// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! SMTP DATA size limits (streaming read, dot-stuffing).

use chatmail_types::{ChatmailError, Result};
use tokio::io::AsyncBufRead;

/// RFC 5321 dot-stuffing: a leading `.` on a DATA line is removed.
pub fn unstuff_smtp_data_line(line: &str) -> &str {
    line.strip_prefix('.').unwrap_or(line)
}

/// Bytes contributed by one DATA line (unstuffed payload + CRLF).
pub fn smtp_data_line_octets(line: &str) -> u64 {
    let n = unstuff_smtp_data_line(line).len();
    u64::try_from(n).unwrap_or(u64::MAX).saturating_add(2)
}

/// Optional `SIZE=` ESMTP parameter on `MAIL FROM`.
pub fn parse_smtp_size_parameter(line: &str) -> Result<Option<u64>> {
    let upper = line.to_ascii_uppercase();
    let Some(idx) = upper.find("SIZE=") else {
        return Ok(None);
    };
    let digits: String = line[idx + 5..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return Err(ChatmailError::protocol("bad SIZE parameter"));
    }
    digits
        .parse::<u64>()
        .map(Some)
        .map_err(|_| ChatmailError::protocol("SIZE out of range"))
}

/// Read SMTP DATA until a lone `.`, enforcing `max_bytes` on stored content.
///
/// If the limit is exceeded, remaining lines are drained but not stored.
pub async fn read_smtp_data_limited<R>(
    lines: &mut tokio::io::Lines<R>,
    max_bytes: u64,
) -> Result<Vec<u8>>
where
    R: AsyncBufRead + Unpin,
{
    let mut data = Vec::new();
    let mut over_limit = false;

    while let Some(line) = lines.next_line().await? {
        if line == "." {
            break;
        }
        if !over_limit {
            let add = smtp_data_line_octets(&line);
            if data.len() as u64 + add > max_bytes {
                over_limit = true;
            } else {
                let unstuffed = unstuff_smtp_data_line(&line);
                data.extend_from_slice(unstuffed.as_bytes());
                data.extend_from_slice(b"\r\n");
            }
        }
    }

    if over_limit {
        return Err(ChatmailError::message_too_large());
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_types::ChatmailError;
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[test]
    fn unstuff_strips_one_leading_dot() {
        assert_eq!(unstuff_smtp_data_line(".."), ".");
        assert_eq!(unstuff_smtp_data_line("hello"), "hello");
    }

    #[test]
    fn smtp_data_line_octets_includes_crlf() {
        assert_eq!(smtp_data_line_octets("ab"), 4);
        assert_eq!(smtp_data_line_octets(".."), 3);
    }

    #[test]
    fn parse_size_from_mail_from() {
        let s = parse_smtp_size_parameter("MAIL FROM:<a@test> SIZE=4096").unwrap();
        assert_eq!(s, Some(4096));
        assert_eq!(parse_smtp_size_parameter("MAIL FROM:<a@test>").unwrap(), None);
    }

    #[test]
    fn parse_size_is_case_insensitive() {
        assert_eq!(
            parse_smtp_size_parameter("MAIL FROM:<a@test> size=128").unwrap(),
            Some(128)
        );
    }

    #[test]
    fn parse_size_rejects_non_numeric() {
        assert!(parse_smtp_size_parameter("MAIL FROM:<a@test> SIZE=abc").is_err());
    }

    #[tokio::test]
    async fn read_smtp_data_limited_accepts_body_under_limit() {
        let input = "From: a@test\r\n\r\nbody\r\n.\r\n";
        let mut lines = BufReader::new(input.as_bytes()).lines();
        let body = read_smtp_data_limited(&mut lines, 64).await.unwrap();
        assert_eq!(body, b"From: a@test\r\n\r\nbody\r\n");
    }

    #[tokio::test]
    async fn read_smtp_data_limited_accepts_exactly_at_limit() {
        let input = "aaaa\r\n.\r\n";
        let mut lines = BufReader::new(input.as_bytes()).lines();
        assert_eq!(smtp_data_line_octets("aaaa"), 6);
        let body = read_smtp_data_limited(&mut lines, 6).await.unwrap();
        assert_eq!(body, b"aaaa\r\n");
    }

    #[tokio::test]
    async fn read_smtp_data_limited_rejects_when_second_line_exceeds() {
        let input = "aa\r\nbbbb\r\n.\r\n";
        let mut lines = BufReader::new(input.as_bytes()).lines();
        let err = read_smtp_data_limited(&mut lines, 8).await.unwrap_err();
        assert!(matches!(err, ChatmailError::MessageTooLarge));
    }

    #[tokio::test]
    async fn read_smtp_data_limited_does_not_store_lines_after_overflow() {
        let input = "aa\r\nbbbb\r\n.\r\n";
        let mut lines = BufReader::new(input.as_bytes()).lines();
        let _ = read_smtp_data_limited(&mut lines, 8).await.unwrap_err();
        assert!(
            lines.next_line().await.unwrap().is_none(),
            "reader should be drained through end-of-data marker"
        );
    }

    #[tokio::test]
    async fn read_smtp_data_limited_counts_unstuffed_dot_line() {
        let input = "..end\r\n.\r\n";
        let mut lines = BufReader::new(input.as_bytes()).lines();
        assert_eq!(smtp_data_line_octets("..end"), 6);
        let err = read_smtp_data_limited(&mut lines, 5).await.unwrap_err();
        assert!(matches!(err, ChatmailError::MessageTooLarge));
    }
}
