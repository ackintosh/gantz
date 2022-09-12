use chrono::{DateTime, NaiveDateTime, ParseResult, Utc};

// https://github.com/torproject/torspec/blob/main/dir-spec.txt
// 3.4.1. Vote and consensus status document formats
pub(crate) fn parse_consensus_document(consensus: String) -> Result<Consensus, ParseError> {
    let mut valid_after = None;
    let mut valid_until = None;

    for line in consensus.lines() {
        let strs = line.split_whitespace().collect::<Vec<_>>();
        match strs[0] {
            "network-status-version" => {
                assert_eq!(3, strs.len());
                if strs[1] != "3" || strs[2] != "microdesc" {
                    return Err(ParseError::UnsupportedDocumentFormatVersion(String::from(
                        strs[1],
                    )));
                }
            }
            "vote-status" => {
                assert_eq!(2, strs.len());
                if strs[1] != "consensus" {
                    return Err(ParseError::UnexpectedVoteStatus(String::from(strs[1])));
                }
            }
            // TODO: consensus-methods
            // TODO: consensus-method
            "valid-after" => {
                assert_eq!(3, strs.len());
                match NaiveDateTime::parse_from_str(
                    &format!("{} {}", strs[1], strs[2]),
                    "%Y-%m-%d %H:%M:%S",
                ) {
                    Ok(datetime) => valid_after = Some(DateTime::<Utc>::from_utc(datetime, Utc)),
                    Err(e) => {
                        return Err(ParseError::DateTimeParseError("valid-after".to_string(), e))
                    }
                }
            }
            "valid-until" => {
                assert_eq!(3, strs.len());
                match NaiveDateTime::parse_from_str(
                    &format!("{} {}", strs[1], strs[2]),
                    "%Y-%m-%d %H:%M:%S",
                ) {
                    Ok(datetime) => valid_until = Some(DateTime::<Utc>::from_utc(datetime, Utc)),
                    Err(e) => {
                        return Err(ParseError::DateTimeParseError("valid-until".to_string(), e))
                    }
                }
            }
            _ => {
                // TODO
            }
        }
    }

    Ok(Consensus {
        valid_after: valid_after.unwrap(),
        valid_until: valid_until.unwrap(),
    })
}

#[derive(Debug)]
pub(crate) enum ParseError {
    UnsupportedDocumentFormatVersion(String),
    UnexpectedVoteStatus(String),
    DateTimeParseError(String, chrono::ParseError),
}

#[derive(Debug)]
pub(crate) struct Consensus {
    pub(crate) valid_after: DateTime<Utc>,
    pub(crate) valid_until: DateTime<Utc>,
}
