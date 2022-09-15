use bitflags::bitflags;
use chrono::{DateTime, NaiveDateTime, Utc};
use rand::Rng;
use std::net::Ipv4Addr;

const CACHE_KEY_BODY: &str = "consensus_document_body";
const CACHE_KEY_VALID_UNTIL: &str = "consensus_document_valid_until";
const ONION_ROUTER_LIMIT: usize = 100;

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
    let mut tmp_onion_router: Option<OnionRouter> = None;
    let mut onion_routers = vec![];

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
            "r" => {
                if let Some(or) = tmp_onion_router {
                    if or.is_available() {
                        onion_routers.push(or);
                        if onion_routers.len() >= ONION_ROUTER_LIMIT {
                            tmp_onion_router = None;
                            break;
                        }
                    }
                }
                // "r" SP nickname SP identity SP digest SP publication SP IP SP ORPort SP DirPort
                //         NL
                tmp_onion_router = Some(OnionRouter {
                    nickname: strs[1].to_string(),
                    ip: strs[5].parse().expect("valid IPv4 address"),
                    or_port: strs[6].parse().expect("valid (OR) port number"),
                    dir_port: strs[7].parse().expect("valid (Dir) port number"),
                    flags: Flags::empty(),
                });
            }
            // A series of space-separated status flags.
            "s" => {
                if let Some(or) = tmp_onion_router.as_mut() {
                    for flag_index in 1..strs.len() {
                        or.flags.insert(strs[flag_index].into());
                    }
                } else {
                    panic!("No tmp_onion_router exists");
                }
            }
            _ => {
                // TODO
            }
        }
    }

    if let Some(or) = tmp_onion_router {
        if or.is_available() {
            onion_routers.push(or);
        }
    }

    Ok(Consensus {
        valid_after: valid_after.unwrap(),
        valid_until: valid_until.unwrap(),
        onion_routers,
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
    pub(crate) onion_routers: Vec<OnionRouter>,
}

impl Consensus {
    pub(crate) fn choose_guard_relay(&self) -> Result<&OnionRouter, String> {
        let mut rng = rand::thread_rng();
        let uniform = rand::distributions::Uniform::new(0, self.onion_routers.len() - 1);

        let mut attempted = 0;

        while attempted < 100 {
            let i = rng.sample(uniform);

            if let Some(or) = self.onion_routers.get(i) {
                if or.flags.contains(Flags::GUARD) {
                    return Ok(or);
                }
            }

            attempted += 1;
        }

        return Err("Could not find aguard node.".to_string());
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OnionRouter {
    nickname: String,
    ip: Ipv4Addr,
    or_port: u16,
    dir_port: u16,
    flags: Flags,
}

impl OnionRouter {
    // 5.4.1. Choosing routers for circuits.
    // https://github.com/torproject/torspec/blob/main/dir-spec.txt
    //
    // - Clients SHOULD NOT use non-'Valid' or non-'Running' routers unless
    //   requested to do so.
    //
    // - Clients SHOULD NOT use non-'Fast' routers for any purpose other than
    //   very-low-bandwidth circuits (such as introduction circuits).
    //
    // - Clients SHOULD NOT use non-'Stable' routers for circuits that are
    //   likely to need to be open for a very long time (such as those used for
    //   IRC or SSH connections).
    fn is_stable(&self) -> bool {
        self.flags
            .contains(Flags::VALID | Flags::RUNNING | Flags::FAST | Flags::STABLE)
    }

    fn is_available(&self) -> bool {
        // "0" represents "none"
        self.is_stable() && self.dir_port > 0
    }
}

bitflags! {
    pub(crate) struct Flags: u32 {
        const AUTHORITY = 0b0000000000001;
        const BAD_EXIT = 0b0000000000010;
        const EXIT = 0b0000000000100;
        const FAST = 0b0000000001000;
        const GUARD = 0b0000000010000;
        const HS_DIR = 0b0000000100000;
        const MIDDLE_ONLY = 0b0000001000000;
        const NO_ED_CONSENSUS = 0b0000010000000;
        const STABLE = 0b0000100000000;
        const STALE_DESC = 0b0001000000000;
        const RUNNING = 0b0010000000000;
        const VALID = 0b0100000000000;
        const V2DIR = 0b1000000000000;
    }
}

impl From<&str> for Flags {
    fn from(s: &str) -> Self {
        match s {
            "Authority" => Flags::AUTHORITY,
            "BadExit" => Flags::BAD_EXIT,
            "Exit" => Flags::EXIT,
            "Fast" => Flags::FAST,
            "Guard" => Flags::GUARD,
            "HSDir" => Flags::HS_DIR,
            "MiddleOnly" => Flags::MIDDLE_ONLY,
            "NoEdConsensus" => Flags::NO_ED_CONSENSUS,
            "Stable" => Flags::STABLE,
            "StaleDesc" => Flags::STALE_DESC,
            "Running" => Flags::RUNNING,
            "Valid" => Flags::VALID,
            "V2Dir" => Flags::V2DIR,
            _ => unreachable!(),
        }
    }
}
