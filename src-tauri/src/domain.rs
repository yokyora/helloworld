use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "deepseek-flash";
pub const MAX_INPUT_CHARS: usize = 4000;
pub const MAX_CONTEXT_CHARS: usize = 16000;
pub const SYSTEM_PROMPT: &str = "你是 Hello World，一位温和、坦诚的桌面陪伴者。默认用简洁自然的中文回应，尊重用户边界。你只能看到本次提供的对话；没有屏幕感知、长期记忆或操作电脑的能力，不要声称你看到了用户屏幕、记住了未提供的信息或完成了外部操作。";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: &'static str,
}

impl AppError {
    pub const fn new(code: &'static str, message: &'static str) -> Self { Self { code, message } }
    pub fn storage() -> Self { Self::new("storage", "本地数据读写失败，请检查磁盘空间和目录权限。") }
    pub fn cancelled() -> Self { Self::new("cancelled", "已停止回复；服务端已处理的请求仍可能计费。") }
    pub fn busy() -> Self { Self::new("busy", "请先停止或等待当前回复，再执行此操作。") }
}

impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self { Self::storage() }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetState { pub resting: bool, pub quiet: bool }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub model: String,
    pub has_api_key: bool,
    pub allow_remote: bool,
}

// Intentionally no Debug/Serialize: this input can contain a plaintext secret.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsInput {
    pub model: String,
    pub api_key: Option<String>,
    pub remove_key: bool,
    pub allow_remote: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub user_text: String,
    pub assistant_text: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub pet: PetState,
    pub settings: Settings,
    pub turns: Vec<Turn>,
    pub active_request: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message { pub role: String, pub content: String }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delta { pub request_id: String, pub text: String }

pub fn validate_model(model: &str) -> Result<(), AppError> {
    if matches!(model, "deepseek-flash" | "deepseek-v4-pro") { Ok(()) }
    else { Err(AppError::new("invalid_model", "请选择支持的 DeepSeek 模型。")) }
}

pub fn validate_message(id: &str, text: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(AppError::new("invalid_request", "请求标识无效，请重试。"));
    }
    if text.trim().is_empty() || text.chars().count() > MAX_INPUT_CHARS {
        return Err(AppError::new("invalid_input", "请输入 1～4000 个字符。"));
    }
    Ok(())
}

pub fn context(turns: &[Turn], input: String) -> Vec<Message> {
    let mut budget = MAX_CONTEXT_CHARS.saturating_sub(input.chars().count());
    let mut pairs = Vec::new();
    for turn in turns.iter().rev().filter(|t| t.status == "complete").take(8) {
        let size = turn.user_text.chars().count() + turn.assistant_text.chars().count();
        if size > budget { break; }
        budget -= size;
        pairs.push(turn);
    }
    let mut result = vec![Message { role: "system".into(), content: SYSTEM_PROMPT.into() }];
    for turn in pairs.into_iter().rev() {
        result.push(Message { role: "user".into(), content: turn.user_text.clone() });
        result.push(Message { role: "assistant".into(), content: turn.assistant_text.clone() });
    }
    result.push(Message { role: "user".into(), content: input });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn turn(id: &str, status: &str) -> Turn { Turn { id: id.into(), user_text: id.into(), assistant_text: "答".into(), status: status.into(), created_at: 0 } }
    #[test]
    fn context_excludes_failed_turns_and_keeps_pairs_in_order() {
        let messages = context(&[turn("1", "complete"), turn("2", "cancelled"), turn("3", "complete")], "新问题".into());
        assert_eq!(messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), [SYSTEM_PROMPT, "1", "答", "3", "答", "新问题"]);
    }
    #[test]
    fn context_is_bounded_without_cutting_a_pair() {
        let mut old = turn("old", "complete"); old.assistant_text = "中".repeat(MAX_CONTEXT_CHARS);
        assert_eq!(context(&[old, turn("recent", "complete")], "新".into()).len(), 4);
        let many: Vec<_> = (0..30).map(|i| turn(&i.to_string(), "complete")).collect();
        assert_eq!(context(&many, "新".into()).len(), 18);
    }
    #[test]
    fn rejects_empty_oversized_and_invalid_requests() {
        assert!(validate_message("ok", "  ").is_err());
        assert!(validate_message("ok", &"中".repeat(4001)).is_err());
        assert!(validate_message("../bad", "hi").is_err());
        assert!(validate_model("unknown").is_err());
    }
}
