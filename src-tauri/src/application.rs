use std::{collections::VecDeque, sync::{Arc, Mutex}};
use tokio_util::sync::CancellationToken;
use crate::{domain::*, provider::{ChatRequest, ModelProvider}, secrets, storage::Store};

pub struct ActiveRequest { pub id: String, pub cancel: CancellationToken }
pub struct Core { pub store: Store, pub resting: bool, pub active: Option<ActiveRequest>, pub revision: u64, pub early_cancels: VecDeque<String> }
pub struct Application { pub core: Mutex<Core>, pub provider: Box<dyn ModelProvider> }

impl Core {
    pub fn snapshot(&self) -> Result<Snapshot, AppError> {
        Ok(Snapshot {revision:self.revision,pet:PetState {resting:self.resting,quiet:self.store.quiet()?},settings:self.store.settings()?,turns:self.store.turns()?,active_request:self.active.as_ref().map(|a| a.id.clone())})
    }
    pub fn ensure_idle(&self) -> Result<(), AppError> { if self.active.is_some() { Err(AppError::busy()) } else { Ok(()) } }
    pub fn prepare(&mut self, id: String, text: String) -> Result<(ChatRequest, CancellationToken), AppError> {
        validate_message(&id,&text)?;
        if let Some(index) = self.early_cancels.iter().position(|cancelled| cancelled == &id) {
            self.early_cancels.remove(index);
            return Err(AppError::cancelled());
        }
        self.ensure_idle()?;
        if self.resting { return Err(AppError::new("resting", "先叫醒桌宠，再开始聊天。")); }
        let settings = self.store.settings()?;
        if !settings.allow_remote { return Err(AppError::new("consent", "请先在设置中允许将对话发送给 DeepSeek。")); }
        validate_model(&settings.model)?;
        let encrypted = self.store.encrypted_key()?.ok_or_else(|| AppError::new("missing_key", "请先在设置中保存 DeepSeek API Key。"))?;
        let key = secrets::unprotect(&encrypted)?;
        let messages = context(&self.store.turns()?, text.clone());
        self.store.start(&id,&text)?;
        let cancel = CancellationToken::new();
        self.active = Some(ActiveRequest {id,cancel:cancel.clone()});
        self.revision += 1;
        Ok((ChatRequest {model:settings.model,api_key:key,messages},cancel))
    }
    pub fn cancel(&mut self, id: String) {
        if let Some(a) = &self.active { if a.id == id { a.cancel.cancel(); return; } }
        // IPC calls can arrive before prepare's blocking task acquires the lock.
        if !self.early_cancels.contains(&id) {
            if self.early_cancels.len() >= 32 { self.early_cancels.pop_front(); }
            self.early_cancels.push_back(id);
        }
    }
    pub fn save_settings(&mut self, input: SettingsInput) -> Result<Snapshot, AppError> {
        self.ensure_idle()?;
        validate_model(&input.model)?;
        let key = input.api_key.map(zeroize::Zeroizing::new);
        let encrypted = if input.remove_key { None } else if let Some(key) = key.as_ref().filter(|k| !k.trim().is_empty()) {
            let key = key.trim();
            if !(8..=512).contains(&key.len()) || !key.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(AppError::new("invalid_key", "密钥格式不正确，请检查是否包含空格或换行。"));
            }
            Some(secrets::protect(key.as_bytes())?)
        } else { self.store.encrypted_key()? };
        self.store.save_settings(&input.model,encrypted.as_deref(),input.allow_remote)?;
        self.revision += 1;
        self.snapshot()
    }
    pub fn finish(&mut self, id: &str, text: &str, result: &Result<(),AppError>) -> Result<Snapshot,AppError> {
        let active = self.active.as_ref().filter(|a| a.id == id).ok_or_else(|| AppError::new("stale", "这条回复已失效。"))?;
        let status = if active.cancel.is_cancelled() { "cancelled" } else { match result {Ok(_) => "complete",Err(e) if e.code=="cancelled" => "cancelled",Err(_) => "error"} };
        let saved = self.store.finish(id,text,status);
        self.active = None;
        self.revision += 1;
        saved?;
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn core() -> Core { Core {store:Store::memory(),resting:false,active:None,revision:0,early_cancels:VecDeque::new()} }
    #[test]
    fn early_cancel_and_consent_fail_before_persisting_messages() {
        let mut c = core(); c.cancel("early".into());
        assert_eq!(c.prepare("early".into(),"hi".into()).err().unwrap().code,"cancelled");
        assert_eq!(c.prepare("no-consent".into(),"hi".into()).err().unwrap().code,"consent");
        assert!(c.store.turns().unwrap().is_empty());
    }
    #[cfg(windows)]
    #[test]
    fn busy_rejection_cancellation_and_stale_results_preserve_current_job() {
        let mut c = core();
        c.save_settings(SettingsInput {model:DEFAULT_MODEL.into(),api_key:Some("test-only-not-real".into()),remove_key:false,allow_remote:true}).unwrap();
        let (_, token) = c.prepare("one".into(),"你好".into()).unwrap();
        assert_eq!(c.prepare("two".into(),"你好".into()).err().unwrap().code,"busy");
        assert!(c.save_settings(SettingsInput {model:DEFAULT_MODEL.into(),api_key:None,remove_key:true,allow_remote:false}).is_err());
        assert!(c.finish("stale", "bad", &Ok(())).is_err());
        c.cancel("old-id".into()); assert!(!token.is_cancelled());
        c.cancel("one".into()); assert!(token.is_cancelled());
        let state = c.finish("one","部分内容",&Ok(())).unwrap();
        assert_eq!(state.turns[0].status,"cancelled"); assert!(state.active_request.is_none());
        let (request,_) = c.prepare("two".into(),"继续".into()).unwrap();
        assert_eq!(request.messages.len(),2);
    }
    #[cfg(windows)]
    #[test]
    fn settings_never_return_secret_and_revocation_blocks_send() {
        let mut c = core();
        let state = c.save_settings(SettingsInput {model:DEFAULT_MODEL.into(),api_key:Some("test-only-secret".into()),remove_key:false,allow_remote:true}).unwrap();
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("test-only-secret")); assert!(state.settings.has_api_key);
        c.save_settings(SettingsInput {model:DEFAULT_MODEL.into(),api_key:None,remove_key:false,allow_remote:false}).unwrap();
        assert_eq!(c.prepare("blocked".into(),"hi".into()).err().unwrap().code,"consent");
        c.save_settings(SettingsInput {model:DEFAULT_MODEL.into(),api_key:None,remove_key:true,allow_remote:true}).unwrap();
        assert_eq!(c.prepare("missing".into(),"hi".into()).err().unwrap().code,"missing_key");
    }
}

// SQLite and DPAPI run off the UI/async executor; never hold the core lock over network await.
pub async fn access<T: Send + 'static>(app: &Arc<Application>, action: impl FnOnce(&mut Core)->Result<T,AppError> + Send + 'static) -> Result<T,AppError> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut core = app.core.lock().map_err(|_| AppError::new("state", "应用状态异常，请重启。"))?;
        action(&mut core)
    }).await.map_err(|_| AppError::new("task", "后台任务未能完成，请重试。"))?
}
