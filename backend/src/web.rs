//! Web search and page reading for every employee (research, docs, market info).
//! Search uses Bing's HTML results (DuckDuckGo as a fallback): no API key. Pages are fetched as text;
//! addresses inside this machine / network are refused so the admin API can't be reached this way.
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const MAX_PAGE_CHARS: usize = 20_000;

fn private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || v.is_documentation() || v.octets()[0] == 100 && (64..128).contains(&v.octets()[1]),
        IpAddr::V6(v) => v.is_loopback() || v.is_unspecified() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// The host is on the public internet (every address it resolves to).
fn public_host(host: &str) -> bool {
    let h = host.trim_matches(['[', ']']);
    if h.eq_ignore_ascii_case("localhost") || h.ends_with(".localhost") || h.ends_with(".local") || h.ends_with(".internal") {
        return false;
    }
    if let Ok(ip) = h.parse::<IpAddr>() {
        return !private_ip(ip);
    }
    match (h, 443).to_socket_addrs() {
        Ok(addrs) => {
            let v: Vec<_> = addrs.collect();
            !v.is_empty() && v.iter().all(|a| !private_ip(a.ip()))
        }
        Err(_) => false,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(20))
        // Each redirect hop must stay on the public internet too.
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 5 {
                a.stop()
            } else if a.url().host_str().is_some_and(public_host) {
                a.follow()
            } else {
                a.stop()
            }
        }))
        .build()
        .unwrap_or_default()
}

fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|e| *e <= 10);
        let Some(e) = end else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..e];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes `<tag …>…</tag>` blocks (script, style…), case-insensitively.
fn drop_blocks(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = String::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find(&open).map(|i| i + pos) {
        out.push_str(&html[pos..i]);
        match lower[i..].find(&close) {
            Some(j) => pos = i + j + close.len(),
            None => return out,
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// HTML → readable text: blocks on their own lines, tags removed, entities decoded.
pub fn html_to_text(html: &str) -> String {
    let mut h = html.to_string();
    for t in ["script", "style", "noscript", "svg", "head", "iframe", "template"] {
        h = drop_blocks(&h, t);
    }
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in h.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
                if ["p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article", "header", "footer", "pre", "table", "ul", "ol"].contains(&name.as_str()) {
                    out.push('\n');
                }
                if name == "li" && !tag.starts_with('/') {
                    out.push_str("- ");
                }
            }
            (true, _) => tag.push(c),
            (false, _) => out.push(c),
        }
    }
    let text = entities(&out);
    let mut lines: Vec<String> = vec![];
    for l in text.lines() {
        let l = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() && lines.last().is_some_and(|x| x.is_empty()) {
            continue;
        }
        lines.push(l);
    }
    lines.join("\n").trim().to_string()
}

fn title_of(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let i = lower.find("<title")?;
    let s = lower[i..].find('>')? + i + 1;
    let e = lower[s..].find("</title>")? + s;
    Some(entities(html[s..e].trim())).filter(|t| !t.is_empty())
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = vec![];
    for ch in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, b) in ch.iter().enumerate() {
            n |= val(*b)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if ch.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if ch.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Bing wraps result links (…/ck/a?…&u=a1<base64url>…): the real address.
fn unwrap_bing(href: &str) -> String {
    let href = entities(href);
    if href.contains("bing.com/ck/") {
        if let Some(u) = href.split(['&', '?']).find_map(|kv| kv.strip_prefix("u=a1")) {
            if let Some(b) = base64url_decode(u).and_then(|b| String::from_utf8(b).ok()) {
                return b;
            }
        }
    }
    href
}

fn strip_tags(s: &str) -> String {
    html_to_text(s).replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Hit {
    title: String,
    url: String,
    snippet: String,
}

fn parse_bing(html: &str) -> Vec<Hit> {
    html.split("<li class=\"b_algo\"")
        .skip(1)
        .filter_map(|c| {
            let h = c.find("<h2")?;
            let a = c[h..].find("href=\"")? + h + 6;
            let href = &c[a..a + c[a..].find('"')?];
            let t0 = c[a..].find('>')? + a + 1;
            let title = strip_tags(&c[t0..t0 + c[t0..].find("</a>")?]);
            let snippet = c.find("<p").and_then(|p| {
                let s = c[p..].find('>')? + p + 1;
                let body = &c[s..s + c[s..].find("</p>")?];
                // Drop Bing's "Read more" link at the end of the snippet.
                Some(strip_tags(body.split("<a class=\"b_algoReadMore\"").next().unwrap_or(body)))
            });
            let url = unwrap_bing(href);
            (url.starts_with("http") && !title.is_empty()).then_some(Hit { title, url, snippet: snippet.unwrap_or_default() })
        })
        .collect()
}

fn parse_ddg(html: &str) -> Vec<Hit> {
    html.split("class=\"result__a\"")
        .skip(1)
        .filter_map(|c| {
            let a = c.find("href=\"")? + 6;
            let raw = entities(&c[a..a + c[a..].find('"')?]);
            // //duckduckgo.com/l/?uddg=<encoded>&…
            let url = raw
                .split(['?', '&'])
                .find_map(|kv| kv.strip_prefix("uddg="))
                .map(|u| percent_decode(u))
                .unwrap_or(raw);
            let t0 = c.find('>')? + 1;
            let title = strip_tags(&c[t0..t0 + c[t0..].find("</a>")?]);
            let snippet = c.find("result__snippet").and_then(|p| {
                let s = c[p..].find('>')? + p + 1;
                Some(strip_tags(&c[s..s + c[s..].find("</a>")?]))
            });
            (url.starts_with("http") && !title.is_empty()).then_some(Hit { title, url, snippet: snippet.unwrap_or_default() })
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode_query(q: &str) -> String {
    q.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else if b == b' ' { "+".into() } else { format!("%{b:02X}") })
        .collect()
}

/// Top results for a query, as a numbered list (title, address, snippet).
pub async fn search(query: &str) -> Result<String, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Thiếu từ khóa tìm kiếm".into());
    }
    let c = client();
    let mut hits = match c.get(format!("https://www.bing.com/search?q={}&setlang=vi&count=10", encode_query(q))).send().await {
        Ok(r) => parse_bing(&r.text().await.unwrap_or_default()),
        Err(_) => vec![],
    };
    if hits.is_empty() {
        if let Ok(r) = c.post("https://html.duckduckgo.com/html/").form(&[("q", q)]).send().await {
            hits = parse_ddg(&r.text().await.unwrap_or_default());
        }
    }
    if hits.is_empty() {
        return Err("Không tìm được kết quả (máy tìm kiếm không trả lời). Thử từ khóa khác hoặc mở thẳng trang bằng fetch_url.".into());
    }
    Ok(hits
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet.chars().take(300).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The readable text of a public web page.
/// GET a public http(s) address (never this machine / the local network).
async fn get_public(url: &str) -> Result<reqwest::Response, String> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| "Địa chỉ không hợp lệ".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Chỉ đọc được trang http:// hoặc https://".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_string();
    if !tokio::task::spawn_blocking(move || public_host(&host)).await.unwrap_or(false) {
        return Err("Không đọc được địa chỉ nội bộ (máy này / mạng nội bộ). Bản xem thử của dự án thì mở bằng công cụ trình duyệt.".into());
    }
    client().get(parsed).send().await.map_err(|e| format!("Không tải được: {e}"))
}

/// The exact bytes of a public file (image, SVG, font, data…), at most 10 MB.
pub async fn download(url: &str) -> Result<(Vec<u8>, String), String> {
    let res = get_public(url).await?;
    if !res.status().is_success() {
        return Err(format!("Máy chủ trả về HTTP {} — kiểm tra lại địa chỉ (tìm bằng web_search).", res.status().as_u16()));
    }
    let kind = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if res.content_length().is_some_and(|n| n > 10_000_000) {
        return Err("Tệp lớn hơn 10 MB".into());
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 10_000_000 {
        return Err("Tệp lớn hơn 10 MB".into());
    }
    Ok((bytes.to_vec(), kind))
}

pub async fn fetch(url: &str) -> Result<String, String> {
    let res = get_public(url).await?;
    let status = res.status();
    let kind = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
    if !(kind.is_empty() || kind.contains("html") || kind.contains("text") || kind.contains("json") || kind.contains("xml")) {
        return Err(format!("Trang trả về tệp {kind}, không phải văn bản"));
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    let body = String::from_utf8_lossy(&bytes[..bytes.len().min(3_000_000)]).into_owned();
    let looks_html = kind.contains("html") || (kind.is_empty() && body.trim_start().to_ascii_lowercase().starts_with("<!doctype html")) || (kind.is_empty() && body.trim_start().to_ascii_lowercase().starts_with("<html"));
    let (title, text) = if looks_html { (title_of(&body), html_to_text(&body)) } else { (None, body) };
    let mut text: String = text.chars().take(MAX_PAGE_CHARS).collect();
    if text.chars().count() >= MAX_PAGE_CHARS {
        text.push_str("\n…(đã cắt bớt)");
    }
    Ok(format!("{}HTTP {}\n{}", title.map(|t| format!("Tiêu đề: {t}\n")).unwrap_or_default(), status.as_u16(), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_internal_addresses() {
        assert!(!public_host("localhost"));
        assert!(!public_host("127.0.0.1"));
        assert!(!public_host("10.1.2.3"));
        assert!(!public_host("192.168.1.10"));
        assert!(!public_host("169.254.169.254"));
        assert!(!public_host("[::1]"));
        assert!(public_host("8.8.8.8"));
    }

    #[test]
    fn reads_bing_links_and_text() {
        assert_eq!(unwrap_bing("https://www.bing.com/ck/a?!&amp;&amp;p=x&amp;u=a1aHR0cHM6Ly9yZWFjdC5kZXYvYmxvZy8yMDI0LzEyLzA1L3JlYWN0LTE5&amp;ntb=1"), "https://react.dev/blog/2024/12/05/react-19");
        let html = "<li class=\"b_algo\"><h2><a href=\"https://example.com/a\"><strong>React</strong> v19</a></h2><div><p class=\"x\">Bản mới &amp; tốt</p></div></li>";
        let h = parse_bing(html);
        assert_eq!((h[0].title.as_str(), h[0].url.as_str(), h[0].snippet.as_str()), ("React v19", "https://example.com/a", "Bản mới & tốt"));
        let t = html_to_text("<html><head><title>T</title></head><body><script>x()</script><h1>Xin chào</h1><p>A&nbsp;B</p><ul><li>một</li></ul></body></html>");
        assert!(t.contains("Xin chào") && t.contains("A B") && t.contains("- một") && !t.contains("x()"));
        assert_eq!(percent_decode("https%3A%2F%2Fa.com%2Fx%20y"), "https://a.com/x y");
    }
}

#[cfg(test)]
mod live {
    /// Real network: `cargo test --release live_web -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_web() {
        let r = super::search("react 19 release notes").await.unwrap();
        println!("{r}");
        assert!(r.contains("1. ") && r.contains("http"));
        let p = super::fetch("https://example.com/").await.unwrap();
        println!("{}", &p[..p.len().min(300)]);
        assert!(p.contains("Example Domain"));
        assert!(super::fetch("http://127.0.0.1:8787/api/state").await.is_err());
    }
}
