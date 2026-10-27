use std::{future::Future, pin::Pin, time::Duration};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
use crate::domain::{AppError, Message};

pub const ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
pub struct ChatRequest { pub model: String, pub api_key: Zeroizing<String>, pub messages: Vec<Message> }
pub type Emit<'a> = &'a mut (dyn FnMut(&str) -> Result<(), AppError> + Send);
pub trait ModelProvider: Send + Sync {
    fn stream<'a>(&'a self, request: ChatRequest, cancel: CancellationToken, emit: Emit<'a>)
        -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'a>>;
}

pub struct DeepSeek { client: reqwest::Client, endpoint: String }
impl DeepSeek {
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15)).build()
            .map_err(|_| AppError::new("network_init", "无法初始化安全网络连接。"))?;
        Ok(Self { client, endpoint: ENDPOINT.into() })
    }
    async fn receive(&self, request: ChatRequest, emit: Emit<'_>) -> Result<(), AppError> {
        let response = self.client.post(&self.endpoint).bearer_auth(request.api_key.as_str())
            .json(&json!({"model":request.model,"messages":request.messages,"stream":true,
                "thinking":{"type":"disabled"},"max_tokens":2048}))
            .send().await.map_err(|_| network_error())?;
        if response.status() != reqwest::StatusCode::OK { return Err(http_error(response.status().as_u16())); }
        let is_sse = response.headers().get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok()).is_some_and(|h| h.starts_with("text/event-stream"));
        if !is_sse { return Err(protocol_error()); }
        let mut chunks = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut finish = false;
        let mut content_bytes = 0usize;
        loop {
            let chunk = tokio::time::timeout(Duration::from_secs(30), chunks.next()).await
                .map_err(|_| AppError::new("timeout", "DeepSeek 响应超时，请稍后重试。"))?;
            let Some(chunk) = chunk else { return Err(AppError::new("interrupted", "连接中断，已收到的内容保留为未完成回复。")); };
            let chunk = chunk.map_err(|_| network_error())?;
            for data in parser.push(&chunk)? {
                if data.trim() == "[DONE]" { return if finish && content_bytes > 0 { Ok(()) } else { Err(protocol_error()) }; }
                let value: Value = serde_json::from_str(&data).map_err(|_| protocol_error())?;
                if value.get("error").is_some() { return Err(protocol_error()); }
                let Some(choice) = value.get("choices").and_then(|v| v.as_array()).and_then(|v| v.first()) else {
                    if value.get("usage").is_some() { continue; }
                    return Err(protocol_error());
                };
                if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
                    content_bytes += text.len();
                    if content_bytes > 65536 { return Err(AppError::new("output_limit", "回复超过本次长度限制，已停止接收。")); }
                    if !text.is_empty() { emit(text)?; }
                }
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    match reason {
                        "stop" => finish = true,
                        "length" => return Err(AppError::new("output_limit", "回复达到长度上限，已保留收到的部分。")),
                        "content_filter" => return Err(AppError::new("filtered", "服务端未能完成这次回复，请调整内容后重试。")),
                        _ => return Err(protocol_error()),
                    }
                }
            }
        }
    }
}
impl ModelProvider for DeepSeek {
    fn stream<'a>(&'a self, request: ChatRequest, cancel: CancellationToken, emit: Emit<'a>)
        -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'a>> {
        Box::pin(async move {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(AppError::cancelled()),
                result = tokio::time::timeout(Duration::from_secs(120), self.receive(request, emit)) =>
                    result.unwrap_or_else(|_| Err(AppError::new("timeout", "本次回复超过两分钟，已停止等待。"))),
            }
        })
    }
}

fn network_error() -> AppError { AppError::new("network", "无法连接 DeepSeek 或连接已中断，请检查网络后重试。") }
fn protocol_error() -> AppError { AppError::new("protocol", "DeepSeek 返回了未完成或不支持的响应，请稍后重试。") }
fn http_error(status: u16) -> AppError {
    match status {
        401 | 403 => AppError::new("authentication", "DeepSeek 拒绝了此密钥，请检查密钥和账户权限。"),
        402 => AppError::new("balance", "DeepSeek 账户余额不足，请检查服务账户。"),
        429 => AppError::new("rate_limit", "请求过于频繁，请稍后重试。"),
        400 | 404 | 422 => AppError::new("request_rejected", "服务端不接受当前请求，请检查模型配置。"),
        _ => AppError::new("service", "DeepSeek 服务暂时不可用，请稍后重试。"),
    }
}

// Buffer bytes until a complete line exists so split UTF-8 characters are preserved.
#[derive(Default)]
struct SseParser { line: Vec<u8>, data: String }
impl SseParser {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, AppError> {
        let mut events = Vec::new();
        for byte in chunk {
            if *byte != b'\n' {
                self.line.push(*byte);
                if self.line.len() + self.data.len() > 65536 { return Err(protocol_error()); }
                continue;
            }
            let line = std::str::from_utf8(&self.line).map_err(|_| protocol_error())?.trim_end_matches('\r');
            if line.is_empty() {
                if !self.data.is_empty() { events.push(std::mem::take(&mut self.data)); }
            } else if let Some(value) = line.strip_prefix("data:") {
                if !self.data.is_empty() { self.data.push('\n'); }
                self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
            }
            self.line.clear();
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn mock_server(status: u16, body: &str, hold_open: bool) -> (DeepSeek, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}");
        let task = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut received = Vec::new();
                loop {
                    let mut bytes = [0; 2048]; let n = socket.read(&mut bytes).await.unwrap();
                    assert!(n > 0); received.extend_from_slice(&bytes[..n]); assert!(received.len() < 32768);
                    if let Some(end) = received.windows(4).position(|w| w==b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&received[..end]).unwrap().to_lowercase();
                        let length: usize = headers.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                        if received.len() >= end + 4 + length {
                            let request: Value = serde_json::from_slice(&received[end+4..end+4+length]).unwrap();
                            assert_eq!(request["model"],"deepseek-flash");
                            assert_eq!(request["stream"],true);
                            assert_eq!(request["thinking"]["type"],"disabled");
                            assert_eq!(request["messages"][0]["content"],"test question");
                            break;
                        }
                    }
                }
                // Deliberately split headers, SSE records, JSON, and multibyte UTF-8.
                for chunk in response.as_bytes().chunks(7) {
                    if socket.write_all(chunk).await.is_err() { return; }
                    tokio::task::yield_now().await;
                }
                if hold_open { tokio::time::sleep(Duration::from_secs(3)).await; }
            }).await.expect("mock server timeout");
        });
        let mut provider = DeepSeek::new().unwrap();
        provider.client = reqwest::Client::builder().no_proxy().build().unwrap();
        provider.endpoint = endpoint;
        (provider,task)
    }
    fn request() -> ChatRequest {
        ChatRequest {model:"deepseek-flash".into(),api_key:Zeroizing::new("test-only-not-real".into()),messages:vec![Message {role:"user".into(),content:"test question".into()}]}
    }
    const PARTIAL: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"},\"finish_reason\":null}]}\n\n";
    #[tokio::test]
    async fn http_stream_roundtrip_requires_finish_and_done() {
        let body = format!("{PARTIAL}data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n");
        let (provider,server) = mock_server(200,&body,false).await;
        let mut output = String::new();
        provider.stream(request(),CancellationToken::new(),&mut |s| {output.push_str(s);Ok(())}).await.unwrap();
        server.await.unwrap(); assert_eq!(output,"你好");
    }
    #[tokio::test]
    async fn premature_eof_keeps_partial_output_but_is_not_success() {
        let (provider,server) = mock_server(200,PARTIAL,false).await;
        let mut output = String::new();
        let result = provider.stream(request(),CancellationToken::new(),&mut |s| {output.push_str(s);Ok(())}).await;
        server.await.unwrap(); assert_eq!(output,"你好"); assert_eq!(result.unwrap_err().code,"interrupted");
    }
    #[tokio::test]
    async fn cancellation_drops_an_active_http_stream() {
        let (provider,server) = mock_server(200,PARTIAL,true).await;
        let token = CancellationToken::new(); let signal = token.clone();
        let result = tokio::time::timeout(Duration::from_secs(2),provider.stream(request(),token,&mut |_| {signal.cancel();Ok(())})).await.unwrap();
        server.abort(); let _ = server.await; assert_eq!(result.unwrap_err().code,"cancelled");
    }
    #[tokio::test]
    async fn authentication_failure_is_redacted() {
        let (provider,server) = mock_server(401,"private server diagnostics",false).await;
        let result = provider.stream(request(),CancellationToken::new(),&mut |_| panic!("unexpected delta")).await;
        server.await.unwrap(); let error = result.unwrap_err(); assert_eq!(error.code,"authentication"); assert!(!error.message.contains("private"));
    }
    #[test]
    fn parses_utf8_split_at_every_byte_crlf_comments_and_multiple_events() {
        let input = ": keepalive\r\n\r\ndata: {\"text\":\"你好\"}\r\n\r\ndata: [DONE]\n\n";
        let mut p = SseParser::default(); let mut events = Vec::new();
        for b in input.as_bytes() { events.extend(p.push(&[*b]).unwrap()); }
        assert_eq!(events, ["{\"text\":\"你好\"}", "[DONE]"]);
    }
    #[test]
    fn bounds_unterminated_events_and_supports_multiline_data() {
        let mut p = SseParser::default();
        assert_eq!(p.push(b"data: one\ndata: two\n\n").unwrap(), ["one\ntwo"]);
        assert!(p.push(&vec![b'x'; 65537]).is_err());
    }
    #[tokio::test]
    async fn precancelled_requests_never_reach_the_network() {
        let provider = DeepSeek::new().unwrap(); let token = CancellationToken::new(); token.cancel();
        let request = ChatRequest {model: "deepseek-flash".into(),api_key:Zeroizing::new("test-only".into()),messages:vec![]};
        let result = provider.stream(request, token, &mut |_| panic!("no delta expected")).await;
        assert_eq!(result.unwrap_err().code, "cancelled");
    }
    #[test]
    fn status_errors_never_echo_response_bodies() {
        assert_eq!(http_error(401).code,"authentication");
        assert_eq!(http_error(402).code,"balance");
        assert_eq!(http_error(429).code,"rate_limit");
    }
}
