mod consensus;

use crate::consensus::{
    cache_consensus_document, get_consensus_document_from_cache, parse_consensus_document,
};
use chrono::Utc;
use std::net::Ipv4Addr;

// *** Specs ***
//
// * Tor directory protocol, version 3 *
// > 4. Directory cache operation
// > 5. Client operation
// https://github.com/torproject/torspec/blob/main/dir-spec.txt

#[tokio::main]
async fn main() {
    let now = Utc::now();

    let consensus = if let Some(document) = get_consensus_document_from_cache(&now).await {
        println!("Using cached consensus document.");
        parse_consensus_document(&document).unwrap()
    } else {
        // TODO: Select directory authority randomly.
        let da = directory_authorities().pop().unwrap();
        println!("Downloading consensus document from {}", da.consensus_url());
        // The consensus document is compressed using deflate algorithm.
        let client = reqwest::Client::builder().deflate(true).build().unwrap();
        // TODO: error handling
        let res = client.get(da.consensus_url()).send().await.unwrap();
        // TODO: error handling
        let document = res.text().await.unwrap();
        let consensus = parse_consensus_document(&document).unwrap();
        cache_consensus_document(&document, &consensus.valid_until).await;

        consensus
    };

    // TODO: error handling
    assert!(consensus.valid_after <= now && now <= consensus.valid_until);
    println!("{:?}", consensus);
}

fn directory_authorities() -> Vec<DirectoryAuthority> {
    // https://consensus-health.torproject.org/
    vec![
        DirectoryAuthority::new("maatuska".into(), Ipv4Addr::new(171, 25, 193, 9), 443, 80),
        // DirectoryAuthority::new("moria1".into(), Ipv4Addr::new(128, 31, 0, 34), 9131, 9101),
    ]
}

struct DirectoryAuthority {
    name: String,
    ip: Ipv4Addr,
    dir_port: u32,
    tor_port: u32,
}

impl DirectoryAuthority {
    fn new(name: String, ip: Ipv4Addr, dir_port: u32, tor_port: u32) -> Self {
        DirectoryAuthority {
            name,
            ip,
            dir_port,
            tor_port,
        }
    }

    /// The URL to directory authority's consensus.
    //
    // https://github.com/torproject/torspec/blob/main/dir-spec.txt
    //    The most recent v3 consensus should be available at:
    //       http://<hostname>/tor/status-vote/current/consensus[.z]
    //
    //    Similarly, the v3 microdescriptor consensus should be available at:
    //     http://<hostname>/tor/status-vote/current/consensus-microdesc[.z]
    //
    // Note: A .z URL is a compressed versions of the consensus.
    //
    // https://github.com/torproject/torspec/blob/main/dir-spec.txt
    //    Microdescriptors are a stripped-down version of server descriptors
    //    generated by the directory authorities which may additionally contain
    //    authority-generated information.  Microdescriptors contain only the
    //    most relevant parts that clients care about.  Microdescriptors are
    //    expected to be relatively static and only change about once per week.
    //    Microdescriptors do not contain any information that clients need to
    //    use to decide which servers to fetch information about, or which
    //    servers to fetch information from.
    pub(crate) fn consensus_url(&self) -> String {
        // TODO: https://github.com/servo/rust-url
        format!(
            "http://{}:{}/tor/status-vote/current/consensus-microdesc.z",
            self.ip, self.dir_port
        )
    }
}
