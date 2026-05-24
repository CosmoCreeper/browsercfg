use std::fs;
use std::net::TcpListener;
use std::io::{BufRead, BufReader, Write};

const TEST_PORT_PREF: &str = "browsercfg.test.port";
const TEST_RESULTS_PREF: &str = "browsercfg.test.active";

#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum TestStatus {
    Pass,
    Fail,
    Skip,
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestStatus::Pass => write!(f, "PASS"),
            TestStatus::Fail => write!(f, "FAIL"),
            TestStatus::Skip => write!(f, "SKIP"),
        }
    }
}

fn kill_browser(pid: u32) {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("pkill")
            .arg("-TERM")
            .arg("-P")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();
    }
    
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .arg("/F")
            .arg("/T")
            .arg("/PID")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();
    }
}

fn inject_test_prefs(profile_path: &str, port: u16) {
    let prefs = format!(
        "\nuser_pref(\"{TEST_PORT_PREF}\", {port});\nuser_pref(\"{TEST_RESULTS_PREF}\", true);\n"
    );
    let user_js_path = format!("{profile_path}/user.js");
    let existing = fs::read_to_string(&user_js_path).unwrap_or_default();

    if !existing.contains(TEST_PORT_PREF) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&user_js_path)
            .expect("err: failed to open user.js for writing");
        file.write_all(prefs.as_bytes())
            .expect("err: failed to write test prefs to user.js");
    } else {
        let cleaned: String = existing
            .lines()
            .filter(|l| !l.contains(TEST_PORT_PREF) && !l.contains(TEST_RESULTS_PREF))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&user_js_path, format!("{cleaned}{prefs}"))
            .expect("err: failed to update test prefs in user.js");
    }
}

fn cleanup_test_prefs(profile_path: &str) {
    let user_js_path = format!("{profile_path}/user.js");
    if let Ok(contents) = fs::read_to_string(&user_js_path) {
        let cleaned: String = contents
            .lines()
            .filter(|l| !l.contains(TEST_PORT_PREF) && !l.contains(TEST_RESULTS_PREF))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&user_js_path, cleaned).expect("err: failed to clean up test prefs");
    }
}

fn parse_message(raw: &str) -> Option<ParsedMessage> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    match v.get("type")?.as_str()? {
        "test:result" => {
            let name = v.get("name")?.as_str()?.to_string();
            let status = match v.get("status")?.as_str()? {
                "pass" => TestStatus::Pass,
                "fail" => TestStatus::Fail,
                "skip" => TestStatus::Skip,
                _ => return None,
            };
            let error = v.get("error").and_then(|e| e.as_str()).map(|s| s.to_string());
            Some(ParsedMessage::Result(TestResult { name, status, error }))
        }
        "suite:done" => Some(ParsedMessage::Done),
        _ => None,
    }
}

enum ParsedMessage {
    Result(TestResult),
    Done,
}

fn run_ws_server(listener: TcpListener) -> Vec<TestResult> {
    let mut results = Vec::new();
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut skip = 0usize;

    println!("Waiting for browser to connect...");
    let (stream, _addr) = listener.accept().expect("err: failed to accept WebSocket connection");

    let mut reader = BufReader::new(stream.try_clone().expect("err: cannot clone tcp stream"));
    let mut writer = stream;

    let mut headers = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).expect("err: failed to read HTTP upgrade");
        if line == "\r\n" { break; }
        headers.push(line.trim().to_string());
    }

    let ws_key = headers.iter()
        .find(|h| h.to_lowercase().starts_with("sec-websocket-key:"))
        .and_then(|h| h.splitn(2, ':').nth(1))
        .map(|s| s.trim())
        .expect("err: missing Sec-WebSocket-Key in upgrade request");

    let accept = {
        let combined = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", ws_key);
        let hash = sha1_smol(combined.as_bytes());
        base64_encode(&hash)
    };

    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    writer.write_all(response.as_bytes()).expect("err: failed to send WebSocket handshake");

    let mut raw_reader = reader.into_inner();
    loop {
        let text = match read_ws_frame(&mut raw_reader) {
            Some(t) => t,
            None => break,
        };

        match parse_message(&text) {
            Some(ParsedMessage::Result(result)) => {
                match result.status {
                    TestStatus::Pass => {
                        println!("\x1b[32m✓\x1b[0m {}", result.name);
                        pass += 1;
                    }
                    TestStatus::Fail => {
                        println!("\x1b[31m✗\x1b[0m {}", result.name);
                        if let Some(ref err) = result.error {
                            println!("    \x1b[31m{}\x1b[0m", err);
                        }
                        fail += 1;
                    }
                    TestStatus::Skip => {
                        println!("\x1b[33m-\x1b[0m {} (skipped)", result.name);
                        skip += 1;
                    }
                }
                results.push(result);
            }
            Some(ParsedMessage::Done) => break,
            None => eprintln!("  warn: unrecognised message: {text}"),
        }
    }

    println!();
    println!(
        "Results: \x1b[32m{pass} passed\x1b[0m, \x1b[31m{fail} failed\x1b[0m, \x1b[33m{skip} skipped\x1b[0m"
    );

    results
}

fn read_ws_frame(stream: &mut impl std::io::Read) -> Option<String> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).ok()?;

    let _fin = (header[0] & 0x80) != 0;
    let opcode = header[0] & 0x0F;
    let masked = (header[1] & 0x80) != 0;
    let payload_len_raw = (header[1] & 0x7F) as usize;

    if opcode != 1 { return None; }

    let payload_len = match payload_len_raw {
        126 => {
            let mut buf = [0u8; 2];
            stream.read_exact(&mut buf).ok()?;
            u16::from_be_bytes(buf) as usize
        }
        127 => {
            let mut buf = [0u8; 8];
            stream.read_exact(&mut buf).ok()?;
            u64::from_be_bytes(buf) as usize
        }
        n => n,
    };

    let mask = if masked {
        let mut m = [0u8; 4];
        stream.read_exact(&mut m).ok()?;
        Some(m)
    } else {
        None
    };

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).ok()?;

    if let Some(mask) = mask {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }

    String::from_utf8(payload).ok()
}

fn sha1_smol(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0x00); }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i*4..i*4+4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d),0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, &word) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { CHARS[((b1 & 0xF) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[b2 & 0x3F] as char } else { '=' });
    }
    out
}

pub fn test(profile_path: &str, browser_exe: &str, browser_profile_path: &str) -> Vec<TestResult> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("err: failed to bind test server");
    let port = listener.local_addr().unwrap().port();

    println!("Starting test server on port {port}...");
    inject_test_prefs(profile_path, port);

    let child = std::process::Command::new(browser_exe)
        .arg("-profile")
        .arg(browser_profile_path)
        .arg("--headless")
        .spawn()
        .expect("err: failed to start browser for testing");

    let pid = child.id();
    let results = run_ws_server(listener);

    kill_browser(pid);
    cleanup_test_prefs(profile_path);

    results
}

