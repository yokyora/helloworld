use std::sync::Arc;
use tauri::{Emitter, State, WebviewWindow, ipc::Channel};
use crate::{application::{Application,access}, domain::*};

fn main_window(window: &WebviewWindow) -> Result<(),AppError> {
    if window.label()=="main" { Ok(()) } else { Err(AppError::new("permission", "此窗口不能访问应用数据。")) }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetStatus { revision: u64, pet: PetState, sending: bool }

async fn pet_status(app: &Arc<Application>) -> Result<PetStatus, AppError> {
    access(app, |c| Ok(PetStatus { revision: c.revision,
        pet: PetState { resting: c.resting, quiet: c.store.quiet()? }, sending: c.active.is_some() })).await
}

async fn notify_pet(window: &WebviewWindow, app: &Arc<Application>) -> Result<(), AppError> {
    window.emit_to("pet", "pet-status", pet_status(app).await?)
        .map_err(|_| AppError::new("window", "角色状态同步失败，请重新打开桌宠。"))
}

#[tauri::command]
pub async fn get_pet_status(window: WebviewWindow, app: State<'_,Arc<Application>>) -> Result<PetStatus, AppError> {
    if window.label() != "pet" { return Err(AppError::new("permission", "此窗口不是角色窗口。")); }
    pet_status(&app).await
}

#[tauri::command]
pub async fn get_snapshot(window: WebviewWindow, app: State<'_,Arc<Application>>) -> Result<Snapshot,AppError> {
    main_window(&window)?; access(&app,|c| c.snapshot()).await
}
#[tauri::command]
pub async fn save_settings(window: WebviewWindow, app: State<'_,Arc<Application>>, input: SettingsInput) -> Result<Snapshot,AppError> {
    main_window(&window)?; access(&app,move |c| c.save_settings(input)).await
}
#[tauri::command]
pub async fn set_pet_state(window: WebviewWindow, app: State<'_,Arc<Application>>, resting: bool, quiet: bool) -> Result<Snapshot,AppError> {
    main_window(&window)?;
    let snapshot = access(&app, move |c| {
        c.store.set_quiet(quiet)?; c.resting=resting;
        c.revision += 1;
        if resting { if let Some(a) = &c.active { a.cancel.cancel(); } }
        c.snapshot()
    }).await?;
    notify_pet(&window, &app).await?;
    Ok(snapshot)
}
#[tauri::command]
pub async fn cancel_reply(window: WebviewWindow, app: State<'_,Arc<Application>>, request_id: String) -> Result<(),AppError> {
    main_window(&window)?;
    validate_message(&request_id,"cancel")?;
    access(&app,move |c| { c.cancel(request_id); Ok(()) }).await
}
#[tauri::command]
pub async fn clear_conversation(window: WebviewWindow, app: State<'_,Arc<Application>>) -> Result<Snapshot,AppError> {
    main_window(&window)?;
    access(&app,|c| {c.ensure_idle()?;c.store.clear()?;c.revision+=1;c.snapshot()}).await
}
#[tauri::command]
pub async fn send_message(window: WebviewWindow, app: State<'_,Arc<Application>>, request_id: String, text: String, on_delta: Channel<Delta>) -> Result<Snapshot,AppError> {
    main_window(&window)?;
    let id = request_id.clone();
    let (request,cancel) = access(&app,move |c| c.prepare(id,text.trim().to_owned())).await?;
    // UI synchronization must not leave a prepared reply pending if an event fails.
    if let Err(error) = notify_pet(&window, &app).await { eprintln!("{}", error.message); }
    let mut output = String::new();
    let result = app.provider.stream(request,cancel, &mut |text| {
        output.push_str(text);
        on_delta.send(Delta {request_id:request_id.clone(),text:text.into()}).map_err(|_| AppError::cancelled())
    }).await;
    let stored_result = result.clone();
    let snapshot = access(&app,move |c| c.finish(&request_id,&output,&stored_result)).await?;
    if let Err(error) = notify_pet(&window, &app).await { eprintln!("{}", error.message); }
    result?;
    Ok(snapshot)
}
