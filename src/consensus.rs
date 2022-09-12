use chrono::{DateTime, NaiveDateTime, Utc};

const CACHE_KEY_BODY: &str = "consensus_document_body";
const CACHE_KEY_VALID_UNTIL: &str = "consensus_document_valid_until";

fn cache_dir() -> String {
    format!("{}/.gants", dirs::home_dir().unwrap().display())
}

pub(crate) async fn cache_consensus_document(consensus: &String, valid_until: &DateTime<Utc>) {
    cacache::write(cache_dir(), CACHE_KEY_BODY, consensus)
        .await
        .unwrap();
    cacache::write(cache_dir(), CACHE_KEY_VALID_UNTIL, valid_until.to_rfc3339())
        .await
        .unwrap();
}

pub(crate) async fn get_consensus_document_from_cache(now: &DateTime<Utc>) -> Option<String> {
    let valid_until = match cacache::read(cache_dir(), CACHE_KEY_VALID_UNTIL).await {
        Ok(s) => {
            let valid_until_string = String::from_utf8(s).unwrap();
            DateTime::parse_from_rfc3339(&valid_until_string).unwrap()
        }
        Err(e) => {
            println!("{:?}", e);
            return None;
        }
    };

    if &valid_until < now {
        return None;
    }

    Some(String::from_utf8(cacache::read(cache_dir(), CACHE_KEY_BODY).await.unwrap()).unwrap())
}

// https://github.com/torproject/torspec/blob/main/dir-spec.txt
// 3.4.1. Vote and consensus status document formats
pub(crate) fn parse_consensus_document(consensus: &String) -> Result<Consensus, ParseError> {
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
