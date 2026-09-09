//! Network connectivity diagnostics - a root-cause tool for "无法连接" / the
//! classic `error sending request for url (https://e-hentai.org/popular)`.
//! Triggered from 设置 -> 网络诊断. It reports, step by step:
//!   - the currently configured proxy (a stale dead proxy is a common culprit),
//!   - system DNS vs DoH vs the bundled IP table (catches DNS poisoning),
//!   - TCP 443 reachability per candidate IP (catches routing/firewall blocks),
//!   - an end-to-end HTTPS probe with and without the proxy (catches SNI-DPI /
//!     TLS resets), so DNS-poisoning can be told apart from a hard block,
//!   - HTTPS pinned to a TCP-reachable resolved IPv4, which tells "system DNS
//!     poisoned" apart from "SNI-DPI / hard block".

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use serde::Serialize;

use super::{client::USER_AGENT, dns};

/// Hosts probed for DNS resolution.
const DNS_HOSTS: &[&str] = &["e-hentai.org", "exhentai.org", "ehgt.org"];
/// DoH endpoints probed explicitly (AliDNS primary + hostname fallback).
const DOH_ENDPOINTS: &[(&str, &str)] = &[
    ("DoH 阿里 223.5.5.5", "https://223.5.5.5/dns-query"),
    ("DoH 阿里 dns.alidns.com", "https://dns.alidns.com/dns-query"),
];

#[derive(Debug, Clone, Serialize)]
pub struct DiagStep {
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetDiag {
    /// True when the end-to-end HTTPS probe succeeded (a transport-level reach).
    pub ok: bool,
    pub proxy: String,
    pub steps: Vec<DiagStep>,
}

fn fmt_ips(ips: &[IpAddr]) -> String {
    if ips.is_empty() {
        "（无）".to_string()
    } else {
        let joined = ips.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        format!("{joined}（{n} 个）", n = ips.len())
    }
}


pub async fn run() -> NetDiag {
    let proxy = super::client::proxy_url();
    let mut diag = NetDiag {
        ok: false,
        proxy: proxy.clone().unwrap_or_default(),
        steps: Vec::new(),
    };

    match &proxy {
        Some(p) => diag.steps.push(DiagStep {
            label: "当前代理配置".into(),
            status: "已设置".into(),
            detail: format!(
                "{} —— 若 VPN / 代理已关闭而这里仍残留该地址，通常就是一连就超时的直接原因；请先在「高级 → 代理 URL」里清空后重试。",
                p
            ),
        }),
        None => diag.steps.push(DiagStep {
            label: "当前代理配置".into(),
            status: "未设置".into(),
            detail: "直连模式（未走代理）。".into(),
        }),
    }

    // 1. DNS: system vs DoH vs builtin
    for host in DNS_HOSTS {
        let recs = dns::system_hosts(host).await;
        let poisoned = recs.iter().any(|ip| dns::is_poisoned(ip));
        let status = if recs.is_empty() {
            "无解析结果"
        } else if poisoned {
            "疑似被 DNS 污染"
        } else {
            "有解析"
        };
        diag.steps.push(DiagStep {
            label: format!("系统 DNS：{host}"),
            status: status.into(),
            detail: if recs.is_empty() {
                "系统解析无记录；若系统 DNS 返回假 IP（被污染）会导致连接失败，需依赖 DoH。".into()
            } else {
                format!(
                    "{}。{} 说明系统 DNS 不可靠，下面的 DoH 结果可作为对照。",
                    fmt_ips(&recs),
                    if poisoned { "命中疑似污染 IP，" } else { "" }
                )
            },
        });
    }

    for &(label, ep) in DOH_ENDPOINTS {
        match dns::lookup_doh(ep, "e-hentai.org").await {
            Ok(v) if !v.is_empty() => diag.steps.push(DiagStep {
                label: label.into(),
                status: "可用".into(),
                detail: format!("解析到 e-hentai.org：{}", fmt_ips(&v)),
            }),
            Ok(_) => diag.steps.push(DiagStep {
                label: label.into(),
                status: "无结果".into(),
                detail: "DoH 可达但未返回记录。".into(),
            }),
            Err(e) => diag.steps.push(DiagStep {
                label: label.into(),
                status: "失败".into(),
                detail: format!("{e}"),
            }),
        }
    }

    let built = dns::builtin_hosts("e-hentai.org");
    diag.steps.push(DiagStep {
        label: "内置 IP 表 (e-hentai.org)".into(),
        status: if built.is_empty() { "空" } else { "已载入" }.into(),
        detail: if built.is_empty() { "该主机不在内置表内。".into() } else { fmt_ips(&built) },
    });

    // 2. TCP 443 reachability to the union of all candidates for e-hentai.org
    let candidates = union_candidates("e-hentai.org").await;
    if candidates.is_empty() {
        diag.steps.push(DiagStep {
            label: "TCP 443 连通性".into(),
            status: "无候选".into(),
            detail: "各解析来源都没得出任何 IP。".into(),
        });
    } else {
        diag.steps.push(DiagStep {
            label: format!("TCP 443 连通性（{} 个候选 IP，5s 超时）", candidates.len()),
            status: "探测中".into(),
            detail: "逐个尝试建立到 443 端口的 TCP 连接。".into(),
        });
        for ip in &candidates {
            let (status, note) = tcp_probe(*ip).await;
            diag.steps.push(DiagStep {
                label: format!("TCP {ip}:443"),
                status: status,
                detail: note,
            });
        }
    }

    // 3. End-to-end HTTPS probe, with and without the proxy
    let direct = https_probe("https://e-hentai.org/", None).await;
    let direct_ok = direct.starts_with("HTTP");
    diag.steps.push(DiagStep {
        label: "HTTPS 直连 e-hentai.org".into(),
        status: if direct_ok { "可达" } else { "失败" }.into(),
        detail: direct,
    });
    if let Some(p) = &proxy {
        let via = https_probe("https://e-hentai.org/", Some(p)).await;
        diag.steps.push(DiagStep {
            label: "HTTPS 经代理 e-hentai.org".into(),
            status: if via.starts_with("HTTP") { "可达" } else { "失败" }.into(),
            detail: via,
        });
    }

    // 3b. HTTPS pinned to a TCP-reachable resolved IPv4: tells "system DNS is
    // poisoned" (pinned works) apart from "SNI-DPI / hard block" (pinned fails).
    let pinned = pinned_https_probe("e-hentai.org", "https://e-hentai.org/").await;
    let pinned_ok = pinned.starts_with("HTTP");
    diag.steps.push(DiagStep {
        label: "HTTPS 直连（固定内置/DoH IP）e-hentai.org".into(),
        status: if pinned_ok { "可达" } else { "失败" }.into(),
        detail: pinned,
    });
    if !direct_ok && pinned_ok {
        diag.steps.push(DiagStep {
            label: "诊断结论".into(),
            status: "系统 DNS 被污染".into(),
            detail: "系统 DNS 返回了不可达或蜜罐 IP 导致直连失败；固定到 TCP 可达的内置/DoH IP 后 TLS 正常，建议保持「使用内置站点 IP 表」开启（默认为开）。".into(),
        });
    } else if !pinned_ok {
        diag.steps.push(DiagStep {
            label: "诊断结论".into(),
            status: "疑似 SNI-DPI 阻断".into(),
            detail: "即使固定到 TCP 可达的 IP，TLS 握手仍失败（通常为 SNI-DPI 阻断），需在「高级 → 代理 URL」配置可用代理后重试。".into(),
        });
    }

    diag.ok = pinned_ok;
    diag
}

/// Union of system + DoH (both endpoints) + builtin IPs for `host`, deduped.
async fn union_candidates(host: &str) -> Vec<IpAddr> {
    let mut out = Vec::new();
    out.extend(dns::system_hosts(host).await);
    for &(_, ep) in DOH_ENDPOINTS {
        if let Ok(v) = dns::lookup_doh(ep, host).await {
            out.extend(v);
        }
    }
    out.extend(dns::builtin_hosts(host));
    let mut seen = std::collections::HashSet::new();
    out.retain(|ip| !ip.is_unspecified() && seen.insert(*ip));
    out
}

async fn tcp_probe(ip: IpAddr) -> (String, String) {
    let addr = SocketAddr::new(ip, 443);
    match tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(addr)).await {
        Ok(Ok(_)) => ("可连".into(), "TCP 握手成功（该 IP 路由可达）。".into()),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            ("端口拒绝".into(), "对端主动拒绝（443 未开放或为假 IP）。".into())
        }
        Ok(Err(e)) => ("连接被拒".into(), format!("connect 失败：{e}")),
        Err(_) => ("超时 / 被丢包".into(), "5s 未响应：路由不可达或防火墙丢弃了该 IP。".into()),
    }
}

async fn https_probe(url: &str, proxy: Option<&str>) -> String {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(12))
        .no_proxy();
    if let Some(p) = proxy {
        match reqwest::Proxy::all(p) {
            Ok(px) => builder = builder.proxy(px),
            Err(e) => return format!("代理解析失败：{e}"),
        }
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return format!("客户端构建失败：{e}"),
    };
    match client.get(url).send().await {
        Ok(resp) => {
            let code = resp.status().as_u16();
            if code == 403 || code == 429 {
                format!("HTTP {}（网络已连通，但被 Cloudflare/风控拦截，属 HTTP 层问题）", code)
            } else {
                format!("HTTP {}", code)
            }
        }
        Err(e) => {
            let s = e.to_string();
            if s.contains("handshake") || s.to_lowercase().contains("tls") || s.contains("certificate") {
                format!("TLS/握手失败：{s}（若所有 IP 都如此，倾向 SNI-DPI 阻断，需走代理）")
            } else {
                format!("连接失败：{s}")
            }
        }
    }
}

/// HTTPS probe pinned to TCP-reachable resolved IPv4 candidates. Distinguishes
/// "system DNS poisoned" (pinned works) from "SNI-DPI / hard block" (pinned
/// fails the handshake).
async fn pinned_https_probe(host: &str, url: &str) -> String {
    let candidates = dns::resolve(host).await;
    let mut reachable: Vec<IpAddr> = Vec::new();
    for ip in candidates {
        if !ip.is_ipv4() {
            continue;
        }
        if tcp_ok(ip).await {
            reachable.push(ip);
            if reachable.len() >= 3 {
                break;
            }
        }
    }
    if reachable.is_empty() {
        return "内置表 / DoH / 系统均未给出 TCP 可达的 IPv4 候选。".into();
    }
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(12))
        .no_proxy();
    for ip in &reachable {
        builder = builder.resolve(host, SocketAddr::new(*ip, 443));
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return format!("客户端构建失败：{e}"),
    };
    match client.get(url).send().await {
        Ok(resp) => {
            let code = resp.status().as_u16();
            if code == 403 || code == 429 {
                format!("HTTP {}（已连通，但被 Cloudflare/风控拦截，属 HTTP 层问题）", code)
            } else {
                format!("HTTP {}（经 TCP 可达 IPv4）", code)
            }
        }
        Err(e) => {
            let s = e.to_string();
            if s.contains("handshake") || s.to_lowercase().contains("tls") || s.contains("certificate") {
                format!("TLS/握手失败：{s}（TCP 可达但握手被断，倾向 SNI-DPI 阻断，需走代理）")
            } else {
                format!("连接失败：{s}")
            }
        }
    }
}

/// Quick TCP reachability check (2s) used to pick a working IPv4 for the
/// pinned HTTPS probe.
async fn tcp_ok(ip: IpAddr) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(SocketAddr::new(ip, 443)),
        )
        .await,
        Ok(Ok(_))
    )
}
