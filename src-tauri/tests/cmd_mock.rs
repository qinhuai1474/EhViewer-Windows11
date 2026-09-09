//! Command-layer integration test using a local mock HTTP server (no external
//! network), per the goal's Phase-1 test plan. Proves `get_gallery_list`
//! returns structured JSON end-to-end.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

const LIST_HTML: &str = r#"<html><body>
<div class="itg"><table>
<tr>
  <td class="gl1c glthumb"><div><img style="height:177px;width:250px" data-src="https://ehgt.org/31/7a/sample-1-250-177.jpg"></div><div><div><div><div>12 page</div></div></div></div></td>
  <td class="gl2c"><div class="glname"><a href="/g/555/abc123/mock-title/"><span>Mock Title</span></a></div>
    <div class="gt" title="language:english"></div></td>
  <td class="gl3c"><div class="cn">Manga</div><div class="ir" style="background-position:-16px -21px">Rating</div>
    <div class="glhide"><div><a href="">uploader_mock</a></div><div>12 page</div></div></td>
  <td class="gl4c"><div id="posted_555">01 Jan 2020 00:00</div></td>
</tr>
</table></div>
</body></html>"#;

/// Serves `body` once on a local port. Returns the URL and the raw request head
/// it received, so the test can assert the exactly-sent headers.
fn serve_once() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/list");
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut s) => {
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let mut buf: Vec<u8> = Vec::new();
                    let mut tmp = [0u8; 1024];
                    loop {
                        match s.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let _ = s.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            LIST_HTML.len(),
                            LIST_HTML
                        )
                        .as_bytes(),
                    );
                    let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
                    break;
                }
                Err(_) => {
                    let _ = tx.send(String::new());
                    break;
                }
            }
        }
    });
    // Return the receiver (NOT blocking on recv here): the request only arrives
    // once the caller actually runs get_gallery_list against `url`.
    (url, rx)
}

#[test]
fn get_gallery_list_returns_json() {
    let (addr, rx) = serve_once();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async { ehviewer_windows11_lib::client::engine::get_gallery_list(&addr).await })
        .expect("get_gallery_list");
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].gid, 555);
    assert_eq!(result.items[0].token, "abc123");
    let json = serde_json::to_string(&result).unwrap();
    // Verify it serialises to structured JSON with camelCase fields.
    assert!(json.contains("\"items\""));
    assert!(json.contains("\"pages\":12"));
    assert!(json.contains("\"category\":\"manga\""));
    println!("get_gallery_list -> {json}");

    // The request only arrives after the client hit the mock server.
    let req = rx.recv().expect("mock server received a request");

    // Regression: the request must carry a real User-Agent header (the UA string
    // was previously inserted as a header NAME, which Cloudflare rejects with 400).
    assert!(req.to_lowercase().contains("user-agent:"), "missing User-Agent header in:\n{req}");
    assert!(
        !req.to_lowercase().lines().any(|l| l.trim_start().starts_with("mozilla/")),
        "User-Agent string must not appear as a header name in:\n{req}"
    );
}

/// Serves a sequence of responses on a single keep-alive connection, so a retry
/// (same pooled socket) observes the next response.
fn serve_seq(responses: Vec<(u16, &'static str)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/list");
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut s) => {
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    for (status, body) in responses {
                        let mut buf = [0u8; 4096];
                        let mut got = 0;
                        while got < buf.len() {
                            match s.read(&mut buf[got..]) {
                                Ok(0) => break,
                                Ok(k) => {
                                    got += k;
                                    if buf[..got].windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        let reason = if status == 509 {
                            "Bandwidth Limit Exceeded"
                        } else {
                            "OK"
                        };
                        let _ = s.write_all(
                            format!(
                                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        );
                    }
                    break;
                }
                Err(_) => break,
            }
        }
    });
    url
}

#[test]
fn get_text_retries_on_509_then_succeeds() {
    use ehviewer_windows11_lib::client::client;
    client::set_max_retries(2);
    let addr = serve_seq(vec![(509, "bandwidth-limit"), (200, "hello-body")]);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let body = rt
        .block_on(async { client::get_text(&addr, None).await })
        .expect("transient 509 should be retried to success");
    assert_eq!(body, "hello-body");
    client::set_max_retries(3);
}
