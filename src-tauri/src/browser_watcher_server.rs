use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::browser_watcher::BrowserHeartbeat;

pub const BROWSER_WATCHER_ADDRESS: &str = "127.0.0.1:27123";
pub const BROWSER_WATCHER_ENDPOINT: &str = "http://127.0.0.1:27123/v1/heartbeat";
const MAX_REQUEST_BYTES: usize = 32 * 1024;

pub fn run_browser_watcher_server<Status, Heartbeat>(
    token: String,
    report_status: Status,
    process_heartbeat: Heartbeat,
) where
    Status: Fn(bool, Option<String>) + Send + Sync + 'static,
    Heartbeat: Fn(BrowserHeartbeat) -> Result<(), String> + Send + Sync + 'static,
{
    let listener = match TcpListener::bind(BROWSER_WATCHER_ADDRESS) {
        Ok(listener) => listener,
        Err(error) => {
            report_status(false, Some(error.to_string()));
            return;
        }
    };
    report_status(true, None);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let response = handle_connection(&mut stream, &token, &process_heartbeat);
                let _ = write_response(&mut stream, response);
            }
            Err(error) => report_status(true, Some(error.to_string())),
        }
    }
    report_status(false, Some("browser watcher listener stopped".into()));
}

#[derive(Debug, PartialEq, Eq)]
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct Response {
    status: u16,
    body: String,
}

fn handle_connection<Heartbeat>(
    stream: &mut TcpStream,
    token: &str,
    process_heartbeat: &Heartbeat,
) -> Response
where
    Heartbeat: Fn(BrowserHeartbeat) -> Result<(), String>,
{
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = match read_request(stream) {
        Ok(request) => request,
        Err(error) => return response(400, &error),
    };
    if request.method == "OPTIONS" {
        return response(204, "");
    }
    if request.method != "POST" || request.path != "/v1/heartbeat" {
        return response(404, "not found");
    }
    if request
        .headers
        .get("x-daily-task-monitor-token")
        .map(String::as_str)
        != Some(token)
    {
        return response(401, "invalid watcher token");
    }
    let heartbeat = match serde_json::from_slice::<BrowserHeartbeat>(&request.body) {
        Ok(heartbeat) => heartbeat,
        Err(error) => return response(400, &format!("invalid heartbeat: {error}")),
    };
    match process_heartbeat(heartbeat) {
        Ok(()) => response(202, "accepted"),
        Err(error) => response(422, &error),
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("request ended before headers were complete".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("request is too large".into());
        }
        if let Some(index) = find_header_end(&bytes) {
            break index;
        }
    };
    let (method, path, headers) = {
        let header_text = std::str::from_utf8(&bytes[..header_end])
            .map_err(|_| "request headers are not UTF-8")?;
        let mut lines = header_text.split("\r\n");
        let request_line = lines.next().ok_or("request line is missing")?;
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .ok_or("request method is missing")?
            .to_string();
        let path = request_parts
            .next()
            .ok_or("request path is missing")?
            .to_string();
        let mut headers = HashMap::new();
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
        (method, path, headers)
    };
    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "content-length is invalid")?
        .unwrap_or_default();
    if content_length > MAX_REQUEST_BYTES {
        return Err("request body is too large".into());
    }
    let body_start = header_end + 4;
    while bytes.len().saturating_sub(body_start) < content_length {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("request body is incomplete".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("request is too large".into());
        }
    }
    Ok(Request {
        method,
        path,
        headers,
        body: bytes[body_start..body_start + content_length].to_vec(),
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn response(status: u16, body: &str) -> Response {
    Response {
        status,
        body: body.into(),
    }
}

fn write_response(stream: &mut TcpStream, response: Response) -> std::io::Result<()> {
    let reason = match response.status {
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        422 => "Unprocessable Content",
        _ => "Error",
    };
    let payload = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-Daily-Task-Monitor-Token\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.body.len(),
        response.body
    );
    stream.write_all(payload.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{find_header_end, response};

    #[test]
    fn locates_complete_http_headers() {
        assert_eq!(
            find_header_end(b"POST / HTTP/1.1\r\nA: b\r\n\r\n{}"),
            Some(21)
        );
        assert_eq!(find_header_end(b"POST / HTTP/1.1\r\nA: b"), None);
    }

    #[test]
    fn response_keeps_status_and_body() {
        let response = response(422, "invalid heartbeat");
        assert_eq!(response.status, 422);
        assert_eq!(response.body, "invalid heartbeat");
    }
}
