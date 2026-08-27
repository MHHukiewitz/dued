//! Unrelated types whose common method names look like network I/O.

pub struct NetClient;

impl NetClient {
    pub fn new() -> Self {
        let _ = reqwest_like_fetch();
        Self
    }

    pub fn get(&self) -> u32 {
        reqwest_like_fetch()
    }

    pub fn as_str(&self) -> &'static str {
        "net"
    }

    pub fn is_empty(&self) -> bool {
        true
    }
}

impl Default for NetClient {
    fn default() -> Self {
        Self::new()
    }
}

fn reqwest_like_fetch() -> u32 {
    // Effect tagger matches `reqwest` as network.
    let _client = "reqwest::Client";
    0
}
