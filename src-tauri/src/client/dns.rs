//! Port of SXJ `EhHosts`: custom DNS resolution for EH hosts.
//!
//! Resolution order (mirrors SXJ): bundled static IP table -> DoH (RFC 8484
//! GET). The system/OS resolver is NEVER consulted for these EH hosts, so DNS
//! pollution cannot be injected locally; on a transient failure fresh DoH
//! results are cached into a refresh-override table that prefixes the bundled
//! table. The DNS wire format is handled here directly (no extra dependency),
//! reusing the project's `base64` crate. EH hosts are pinned to these IPs by
//! default so the cold-start request does not depend on a (possibly poisoned)
//! system resolver.

use std::net::IpAddr;
use std::sync::{OnceLock, RwLock};

use base64::Engine;

/// Whether to prefer the bundled IP table before DoH / system DNS.
static USE_BUILTIN: OnceLock<RwLock<bool>> = OnceLock::new();
/// DoH endpoint override (example `https://223.5.5.5/dns-query`); empty = default.
static DOH_URL: OnceLock<RwLock<Option<String>>> = OnceLock::new();
/// Escalation flag: when false only the static IP table is used (fast cold start);
/// set true on a transient failure so DoH + system DNS are also consulted.
static ALLOW_NETWORK: OnceLock<RwLock<bool>> = OnceLock::new();
/// Per-host DoH refresh overrides. After a transient connect failure, fresh DoH
/// results are cached here and prefixed to the bundled table so stale pinned
/// IPs are replaced without editing the const table. Empty until first use.
static REFRESH: OnceLock<RwLock<std::collections::HashMap<String, Vec<IpAddr>>>> = OnceLock::new();

/// Stores fresh DoH results for a host as preferred overrides. Empty input
/// clears the override so a host falls back to the bundled table.
pub fn set_refresh_data(host: &str, ips: &[IpAddr]) {
    let slot = REFRESH.get_or_init(|| RwLock::new(std::collections::HashMap::new()));
    let mut m = slot.write().unwrap();
    let v: Vec<IpAddr> = ips.iter().copied().filter(|i| !i.is_unspecified()).collect();
    if v.is_empty() {
        m.remove(host);
    } else {
        m.insert(host.to_string(), v);
    }
}

/// User-supplied host -> IP override table (SXJ `Hosts` equivalent). Takes
/// precedence over the bundled table and DoH results.
static CUSTOM: OnceLock<RwLock<std::collections::HashMap<String, Vec<IpAddr>>>> = OnceLock::new();

/// Replaces the user host override table entirely (empty map clears it).
pub fn set_custom_hosts(map: std::collections::HashMap<String, Vec<IpAddr>>) {
    let slot = CUSTOM.get_or_init(|| RwLock::new(std::collections::HashMap::new()));
    let mut m = slot.write().unwrap();
    m.clear();
    for (host, ips) in map {
        let v: Vec<IpAddr> = ips.into_iter().filter(|i| !i.is_unspecified()).collect();
        if !v.is_empty() {
            m.insert(host, v);
        }
    }
}

/// Parses a multi-line `host ip[,ip...]` settings block into a host->IPs map.
pub fn parse_hosts_text(text: &str) -> std::collections::HashMap<String, Vec<IpAddr>> {
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(|c: char| c == ' ' || c == '\t' || c == ',');
        let host = match parts.next() {
            Some(h) => h.trim().trim_end_matches('.').to_ascii_lowercase(),
            None => continue,
        };
        if host.is_empty() {
            continue;
        }
        let ips: Vec<IpAddr> = parts
            .filter_map(|p| p.trim().parse::<IpAddr>().ok())
            .filter(|i| !i.is_unspecified())
            .collect();
        if !ips.is_empty() {
            let e: &mut Vec<IpAddr> = map.entry(host).or_default();
            for ip in ips {
                if !e.contains(&ip) {
                    e.push(ip);
                }
            }
        }
    }
    map
}

/// Convenience: parses and installs the hosts block into the override table.
pub fn apply_hosts_text(text: &str) {
    set_custom_hosts(parse_hosts_text(text));
}

fn custom(host: &str) -> Vec<IpAddr> {
    CUSTOM
        .get()
        .and_then(|m| m.read().unwrap().get(host).cloned())
        .unwrap_or_default()
}

/// China-reachable default DoH (AliDNS) used when no override is configured.
const DEFAULT_DOH: &str = "https://223.5.5.5/dns-query";

/// Secondary DoH endpoint (a hostname with a normal leaf cert) tried after
/// the IP-literal alias in case TLS verification on a literal IP fails.
const DOH_ALIDNS: &str = "https://dns.alidns.com/dns-query";

/// Third DoH endpoint (Tencent DNSPod) as a China-friendly fallback when
/// AliDNS (`dns.alidns.com`) is flaky or blocked.
const DEFAULT_DOH_PUB: &str = "https://doh.pub/dns-query";

pub fn set_allow_network_resolve(v: bool) {
    let slot = ALLOW_NETWORK.get_or_init(|| RwLock::new(false));
    *slot.write().unwrap() = v;
}
fn allow_network() -> bool {
    ALLOW_NETWORK.get().map(|l| *l.read().unwrap()).unwrap_or(false)
}

/// Whether the HTTP client should pin EH hosts to custom-resolved IPs. Always
/// true: EH hosts must never fall through to the (potentially poisoned) OS
/// resolver. The bundled table simply sets the order of the candidates.
pub fn should_pin() -> bool {
    true
}

const BUILTIN: &[(&str, &[&str])] = &[
    (
        "e-hentai.org",
        &[
            "104.20.18.168",
            "104.20.19.168",
            "172.66.132.196",
            "172.66.140.62",
            "172.67.2.238",
        ],
    ),
    ("repo.e-hentai.org", &["104.20.18.168", "104.20.19.168", "172.67.2.238"]),
    ("forums.e-hentai.org", &["172.66.132.196", "172.66.140.62"]),
    ("upld.e-hentai.org", &["89.149.221.236", "95.211.208.236"]),
    ("ehgt.org", &["109.236.85.28", "62.112.8.21", "89.39.106.43"]),
    (
        "exhentai.org",
        &[
            "178.175.128.251", "178.175.128.252", "178.175.128.253", "178.175.128.254",
            "178.175.129.251", "178.175.129.252", "178.175.129.253", "178.175.129.254",
            "178.175.132.19", "178.175.132.20", "178.175.132.21", "178.175.132.22",
        ],
    ),
    ("upld.exhentai.org", &["178.175.132.22", "178.175.129.254", "178.175.128.254"]),
    (
        "s.exhentai.org",
        &[
            "178.175.129.253", "178.175.129.254",
            "178.175.128.253", "178.175.128.254",
            "178.175.132.21", "178.175.132.22",
        ],
    ),
];

pub fn set_use_builtin(v: bool) {
    let slot = USE_BUILTIN.get_or_init(|| RwLock::new(true));
    *slot.write().unwrap() = v;
}

pub fn set_doh_url(url: Option<String>) {
    let cleaned = url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    let slot = DOH_URL.get_or_init(|| RwLock::new(None));
    *slot.write().unwrap() = cleaned;
}

fn use_builtin() -> bool {
    USE_BUILTIN.get().map(|l| *l.read().unwrap()).unwrap_or(true)
}

fn doh_url() -> String {
    DOH_URL
        .get()
        .and_then(|l| l.read().ok())
        .and_then(|g| g.clone())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_DOH.to_string())
}

/// Hostnames that we pin to custom-resolved IPs: the bundled table plus any
/// user hosts overrides. Other hosts (e.g. a user mirror) keep the OS resolver.
pub fn pinned_hosts() -> Vec<String> {
    let mut out: Vec<String> = BUILTIN.iter().map(|(h, _)| h.to_string()).collect();
    if let Some(m) = CUSTOM.get() {
        for h in m.read().unwrap().keys() {
            if !out.iter().any(|x| x == h) {
                out.push(h.clone());
            }
        }
    }
    out
}

/// Read-only snapshots + explicit one-shot lookups used by the network
/// diagnostic (`client::diag`).
pub fn doh_endpoint() -> String {
    doh_url()
}
pub async fn lookup_doh(doh: &str, host: &str) -> anyhow::Result<Vec<IpAddr>> {
    doh_lookup(doh, host).await
}
pub async fn system_hosts(host: &str) -> Vec<IpAddr> {
    system_lookup(host).await
}
pub fn builtin_hosts(host: &str) -> Vec<IpAddr> {
    builtin(host)
}

/// Resolve `host` (EH hosts only) bypassing the OS resolver entirely.
/// Fast/cold-start path: bundled table only (no network), in stable table order
/// so the first connect hits a reachable anycast IP. After a transient failure
/// (or when the bundled table is disabled) it merges builtin + DoH, refreshing
/// the override table from DoH so a stale/blocked pinned IP is replaced.
pub async fn resolve(host: &str) -> Vec<IpAddr> {
    if allow_network() || !use_builtin() {
        return resolve_bypass_system(host).await;
    }
    builtin(host)
}

/// Escalated merge: bundled IPv4 table first -> DoH, deduped, IPv4 before IPv6.
/// DoH results are written into the refresh-override table (no system DNS).
async fn resolve_bypass_system(host: &str) -> Vec<IpAddr> {
    let doh = clean_candidates(doh_any(host).await);
    if !doh.is_empty() {
        set_refresh_data(host, &doh);
    }
    order_and_filter(builtin(host), doh, Vec::new())
}

/// Orders and filters a host's candidate IP sets for connection. Places the
/// bundled IPv4 table at the head (reachable anycast first), appends DoH and
/// system results, drops unspecified addresses, and removes poisoned-looking
/// *system* candidates (the untrusted source). IPv4 always precedes IPv6.
fn order_and_filter(built: Vec<IpAddr>, doh: Vec<IpAddr>, sys: Vec<IpAddr>) -> Vec<IpAddr> {
    let mut out = Vec::new();
    out.extend(built.into_iter().filter(|ip| !ip.is_unspecified()));
    out.extend(doh.into_iter().filter(|ip| !ip.is_unspecified()));
    out.extend(
        sys.into_iter()
            .filter(|ip| !ip.is_unspecified() && !is_poisoned(ip)),
    );
    dedup(&mut out);
    let mut v4: Vec<IpAddr> = out.iter().copied().filter(|i| i.is_ipv4()).collect();
    let mut v6: Vec<IpAddr> = out.iter().copied().filter(|i| i.is_ipv6()).collect();
    shuffle(&mut v6);
    v4.extend(v6);
    v4
}
/// Tries the configured DoH, then AliDNS (223.5.5.5), then dns.alidns.com and
/// Tencent DNSPod (doh.pub), returning the first non-empty result set so one
/// broken endpoint no longer sinks the whole escalation (e.g. an IP-literal
/// cert that TLS rejects).
async fn doh_any(host: &str) -> Vec<IpAddr> {
    let configured = doh_url();
    let mut endpoints = Vec::new();
    if !configured.is_empty() {
        endpoints.push(configured);
    }
    endpoints.push(DEFAULT_DOH.to_string());
    endpoints.push(DOH_ALIDNS.to_string());
    endpoints.push(DEFAULT_DOH_PUB.to_string());
    let mut seen = std::collections::HashSet::new();
    for ep in endpoints {
        if !seen.insert(ep.clone()) {
            continue;
        }
        if let Ok(v) = doh_lookup(&ep, host).await {
            if !v.is_empty() {
                let mut v = v;
                shuffle(&mut v);
                return v;
            }
        }
    }
    Vec::new()
}

fn dedup(v: &mut Vec<IpAddr>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|ip| seen.insert(*ip));
}

fn builtin(host: &str) -> Vec<IpAddr> {
    let mut out = custom(host);
    if use_builtin() {
        if let Some((_, ips)) = BUILTIN.iter().find(|(h, _)| *h == host) {
            out.extend(ips.iter().filter_map(|s| (*s).parse::<IpAddr>().ok()));
        }
    }
    if let Some(m) = REFRESH.get() {
        if let Some(ips) = m.read().unwrap().get(host) {
            out.extend(ips.iter().copied());
        }
    }
    dedup(&mut out);
    out
}

/// Drops IPs that must never be connected to: unspecified addresses and known
/// honeypot ranges. Applied to DoH results so a poisoned answer is neither
/// pinned nor cached into the refresh-override table.
fn clean_candidates(ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.into_iter()
        .filter(|ip| !ip.is_unspecified() && !is_poisoned(ip))
        .collect()
}

/// Heuristic for GFW-style DNS poisoning of these EH hosts (telecom honeypot,
/// Facebook ranges, private/loopback). Applied to *system*-resolved candidates
/// during the escalated merge; DoH and the bundled table are trusted sources.
pub fn is_poisoned(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V6(v) => v.is_loopback() || v.is_unspecified(),
        IpAddr::V4(v) => {
            let o = v.octets();
            // private / loopback / link-local / cgnat / unspecified
            if o[0] == 10
                || (o[0] == 172 && o[1] & 0xF0 == 16)
                || (o[0] == 192 && o[1] == 168)
                || o[0] == 127
                || o[0] == 169 && o[1] == 254
                || (o[0] == 100 && o[1] >= 64 && o[1] <= 127)
                || (o[0] == 0 && o[1] == 0 && o[2] == 0 && o[3] == 0)
            {
                return true;
            }
            o == [199, 16, 156, 38]
                || o == [199, 16, 156, 12]
                || o == [203, 0, 113, 0]
                || (o[0] == 31 && o[1] == 13)
                || (o[0] == 157 && o[1] == 240)
                || (o[0] == 173 && o[1] == 252)
        }
    }
}

fn shuffle<T>(v: &mut [T]) {
    let len = v.len();
    if len < 2 {
        return;
    }
    let mut seed = nanos();
    for i in (1..len).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (seed as usize) % (i + 1);
        v.swap(i, j);
    }
}

fn nanos() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

async fn system_lookup(host: &str) -> Vec<IpAddr> {
    let mut v = Vec::new();
    if let Ok(iter) = tokio::net::lookup_host((host, 443)).await {
        for sa in iter {
            let ip = sa.ip();
            if !ip.is_unspecified() {
                v.push(ip);
            }
        }
    }
    shuffle(&mut v);
    v
}

async fn doh_lookup(doh: &str, host: &str) -> anyhow::Result<Vec<IpAddr>> {
    let cli = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let mut out = Vec::new();
    // Query A then AAAA.
    for qtype in [1u16, 28u16] {
        let query = build_dns_query(dns_id(), host, qtype);
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&query);
        let url = format!("{}?dns={}", doh.trim_end_matches('/'), b64);
        let resp = cli
            .get(&url)
            .header("accept", "application/dns-message")
            .send()
            .await?;
        if !resp.status().is_success() {
            continue;
        }
        let bytes = resp.bytes().await?;
        out.extend(parse_dns_response(&bytes));
    }
    Ok(out)
}

fn dns_id() -> u16 {
    (nanos() as u16) | 0x8000
}

/// Builds a minimal DNS query packet (header + QNAME + QTYPE/QCLASS).
fn build_dns_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut q = Vec::with_capacity(64);
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&0x0100u16.to_be_bytes()); // RD = 1
    q.extend_from_slice(&0x0001u16.to_be_bytes()); // QDCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    for label in name.split('.') {
        let lb = label.as_bytes();
        q.push(lb.len() as u8);
        q.extend_from_slice(lb);
    }
    q.push(0);
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&0x0001u16.to_be_bytes()); // IN
    q
}

/// Extracts A/AAAA RDATA addresses from a DNS response.
fn parse_dns_response(buf: &[u8]) -> Vec<IpAddr> {
    let mut out = Vec::new();
    if buf.len() < 12 {
        return out;
    }
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    // Skip the question section.
    let Some(after_q) = skip_name(buf, 12) else { return out; };
    let mut pos = after_q + 4;
    for _ in 0..ancount {
        let Some(after_n) = skip_name(buf, pos) else { break; };
        if after_n + 10 > buf.len() {
            break;
        }
        let rtype = u16::from_be_bytes([buf[after_n], buf[after_n + 1]]);
        let rdlen = u16::from_be_bytes([buf[after_n + 8], buf[after_n + 9]]) as usize;
        let rdata_start = after_n + 10;
        if rdata_start + rdlen > buf.len() {
            break;
        }
        if rtype == 1 && rdlen == 4 {
            let b = &buf[rdata_start..rdata_start + 4];
            out.push(IpAddr::V4(std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3])));
        } else if rtype == 28 && rdlen == 16 {
            let mut octs = [0u8; 16];
            octs.copy_from_slice(&buf[rdata_start..rdata_start + 16]);
            out.push(IpAddr::V6(std::net::Ipv6Addr::from(octs)));
        }
        pos = rdata_start + rdlen;
    }
    out
}

/// Skips a domain name, handling `0xC0` compression pointers used in answers.
fn skip_name(buf: &[u8], start: usize) -> Option<usize> {
    let mut pos = start;
    loop {
        if pos >= buf.len() {
            return None;
        }
        let len = buf[pos] as usize;
        if len == 0 {
            return Some(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            return Some(pos + 2);
        }
        pos += 1 + len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_priority() {
        set_use_builtin(true);
        set_doh_url(None);
        // e-hentai.org and ehgt.org must resolve from the bundled table (offline).
        let v = builtin("e-hentai.org");
        assert!(!v.is_empty());
        assert!(v.iter().all(|ip| !ip.is_unspecified()));
        assert_eq!(builtin("no-such-host.example"), Vec::<IpAddr>::new());
        set_use_builtin(true);
    }

    #[test]
    fn bundled_ips_are_valid_sockets() {
        for (host, ips) in BUILTIN {
            for s in ips.iter() {
                let ip: IpAddr = s.parse().expect("bundled raw IP parses");
                assert!(!ip.is_unspecified(), "{host} -> {ip}");
            }
        }
    }

    #[test]
    fn dns_wire_roundtrip_query() {
        let q = build_dns_query(0x1234, "e-hentai.org", 1);
        assert_eq!(&q[..2], &[0x12, 0x34]);
        assert!(q.contains(&b'e'));
        // QDCOUNT == 1
        assert_eq!(&q[4..6], &[0x00, 0x01]);
    }
    #[tokio::test]
    async fn fast_path_uses_builtin_when_offline() {
        set_use_builtin(true);
        set_allow_network_resolve(false);
        let v = resolve("e-hentai.org").await;
        assert!(!v.is_empty(), "fast path should resolve from bundled table without network");
    }

    #[test]
    fn should_pin_defaults_to_builtin() {
        set_use_builtin(true);
        set_allow_network_resolve(false);
        assert!(should_pin(), "EH hosts are always pinned (full OS-DNS bypass)");
        set_use_builtin(false);
        assert!(should_pin(), "pinning holds even when bundled table is disabled");
        set_use_builtin(true);
    }

    #[test]
    fn builtin_merges_refresh_overrides() {
        set_use_builtin(true);
        let const_ip: IpAddr = "104.20.18.168".parse().unwrap();
        let fresh_ip: IpAddr = "1.2.3.4".parse().unwrap();
        set_refresh_data("e-hentai.org", &[fresh_ip]);
        let v = builtin("e-hentai.org");
        assert!(v.contains(&fresh_ip), "refresh override is included ahead of the table");
        assert!(v.contains(&const_ip), "bundled table IP is still kept");
        // Deduplicated: a fresh IP equal to a bundled one appears once.
        let len_before = builtin("e-hentai.org").len();
        set_refresh_data("e-hentai.org", &[const_ip]);
        let after = builtin("e-hentai.org");
        assert_eq!(after.len() + 1, len_before, "duplicate refresh IP is deduped");
        // Empty refresh clears the override for this host.
        set_refresh_data("e-hentai.org", &[]);
        assert!(!builtin("e-hentai.org").contains(&fresh_ip));
    }

    #[test]
    fn bypass_merge_has_no_system_candidates() {
        // The client resolve path passes an empty system set: only bundled + DoH
        // candidates may appear, so poisoned system IPs can never be used.
        let b: IpAddr = "104.20.18.168".parse().unwrap();
        let doh: IpAddr = "199.16.156.38".parse().unwrap();
        let out = order_and_filter(vec![b], vec![doh], Vec::new());
        assert!(out.contains(&b));
        assert!(out.contains(&doh));
        assert_eq!(out.len(), 2, "no system candidates in the bypass path");
    }

    #[test]
    fn clean_candidates_drops_poisoned_doh_ips() {
        let good: IpAddr = "104.20.18.168".parse().unwrap();
        let fb: IpAddr = "31.13.95.34".parse().unwrap();
        let private: IpAddr = "10.0.0.1".parse().unwrap();
        let unspecified: IpAddr = "::".parse().unwrap();
        let out = clean_candidates(vec![good, fb, private, unspecified]);
        assert_eq!(out, vec![good], "honeypot / private / unspecified dropping in DoH path");
    }

    #[test]
    fn custom_hosts_parse_and_take_priority() {
        set_use_builtin(true);
        let map = parse_hosts_text(
            "# comment line\ne-hentai.org 1.2.3.4,5.6.7.8\nehgt.org 9.9.9.9\n   \n",
        );
        assert_eq!(map.get("e-hentai.org").map(Vec::len), Some(2));
        assert_eq!(map.get("ehgt.org").map(Vec::len), Some(1));
        assert!(map.get("no-such").is_none());

        set_custom_hosts(parse_hosts_text("e-hentai.org 203.0.113.7"));
        let custom_ip: IpAddr = "203.0.113.7".parse().unwrap();
        let const_ip: IpAddr = "104.20.18.168".parse().unwrap();
        let v = builtin("e-hentai.org");
        assert_eq!(v[0], custom_ip, "user host override is the highest priority");
        assert!(v.contains(&const_ip), "bundled table still contributes below the override");
        assert!(
            pinned_hosts().iter().any(|h| h == "e-hentai.org"),
            "custom host is included in pinned hosts"
        );

        set_custom_hosts(Default::default());
    }

    #[test]
    fn is_poisoned_detects_honeypots_and_private() {
        let fb: IpAddr = "31.13.112.4".parse().unwrap();
        assert!(is_poisoned(&fb));
        let priv_: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(is_poisoned(&priv_));
        let v6_lo: IpAddr = "::1".parse().unwrap();
        assert!(is_poisoned(&v6_lo));
        let cloudflare: IpAddr = "104.20.18.168".parse().unwrap();
        assert!(!is_poisoned(&cloudflare), "Cloudflare anycast IP is not treated as poisoned");
    }

    #[test]
    fn order_and_filter_prefers_builtin_and_drops_poisoned_system() {
        let b1: IpAddr = "104.20.18.168".parse().unwrap();
        let b2: IpAddr = "104.20.19.168".parse().unwrap();
        let doh_real: IpAddr = "199.16.156.38".parse().unwrap();
        let sys_dead: IpAddr = "75.126.115.192".parse().unwrap();
        let sys_poisoned: IpAddr = "31.13.112.4".parse().unwrap();
        let sys_v6: IpAddr = "2001::c73b:960d".parse().unwrap();
        let unspecified: IpAddr = "::".parse().unwrap();
        let out = order_and_filter(
            vec![b1, b2],
            vec![doh_real],
            vec![sys_dead, sys_poisoned, sys_v6, unspecified],
        );
        assert_eq!(out[0], b1, "bundled table order is preserved at the head");
        assert_eq!(out[1], b2);
        assert!(out.contains(&doh_real), "DoH/legit IP kept even if its range is unusual");
        assert!(out.contains(&sys_dead), "unreachable-but-unflagged system IP retained (behind builtin)");
        assert!(!out.contains(&sys_poisoned), "poisoned system IP dropped");
        assert!(out.contains(&sys_v6), "kept IPv6 goes after all IPv4");
        assert!(!out.iter().any(|i| i.is_unspecified()));
        let first_v6 = out.iter().position(|i| i.is_ipv6()).expect("has an IPv6 candidate");
        assert!(out.iter().take(first_v6).all(|i| i.is_ipv4()), "IPv4 precedes IPv6");
    }
    #[test]
    fn dedup_keeps_unique() {
        let a = "104.20.18.168".parse::<IpAddr>().unwrap();
        let b = "104.20.19.168".parse::<IpAddr>().unwrap();
        let mut v = vec![a, a, b];
        dedup(&mut v);
        assert_eq!(v.len(), 2);
    }

}
