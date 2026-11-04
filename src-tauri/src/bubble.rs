use std::sync::{Arc,Mutex};
use serde::{Deserialize,Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use crate::{application::{access,Application}, domain::{AppError,BubbleAnchor}, window_shell::{ShellState,ShellMode,apply_mode}};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content { revision: u64, text: String, reply: bool, left: bool }
#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ModelLayout { pub model_id: String, pub x: f64, pub y: f64, pub width: f64, pub height: f64, pub default_anchor: Option<BubbleAnchor> }
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Calibration { model_ready: bool, calibrated: bool, active: bool }
pub struct BubbleState { ready: bool, content: Content, model: Option<ModelLayout>, anchor: Option<BubbleAnchor>, calibrating: bool }
impl BubbleState {
    pub fn new(quiet: bool) -> Self {
        Self { ready: false, content: Content { revision: 0, text: if quiet { "勿扰中" } else { "待机中" }.into(), reply: false, left: false }, model: None, anchor: None, calibrating: false }
    }
}
fn failure(_: impl std::fmt::Display) -> AppError { AppError::new("bubble", "角色气泡更新失败，请重新打开桌宠。") }

// Anchor is mapped from model coordinates into the current pet canvas.
fn placement(pet: (i32,i32,u32,u32), anchor: BubbleAnchor, desired: (u32,u32), area: (i32,i32,u32,u32), gap: i32) -> (i32,i32,u32,u32,bool) {
    let (x,y,w,h) = (pet.0 as i64,pet.1 as i64,pet.2 as i64,pet.3 as i64);
    let (ax,ay,aw,ah) = (area.0 as i64,area.1 as i64,area.2 as i64,area.3 as i64);
    let gap = gap as i64;
    let head_left = x+(w as f64*anchor.x).round() as i64;
    let head_right = head_left;
    let head_y = y+(h as f64*anchor.y).round() as i64;
    let right = (ax+aw-head_right-gap).max(0);
    let left = (head_left-gap-ax).max(0);
    let wanted = (desired.0 as i64).min(aw);
    let on_left = right < wanted && (left >= wanted || left > right);
    let space = if on_left { left } else { right };
    // A very narrow work area can have no usable side: keep the bubble on-screen.
    let width = wanted.min(space.max((64_i64).min(aw)));
    let height = (desired.1 as i64).min(ah);
    let bx = if on_left { head_left-gap-width } else { head_right+gap };
    // CSS tail tip is 20 logical pixels above the bubble window's bottom edge.
    let by = head_y-height+h/9;
    (bx.clamp(ax,ax+aw-width) as i32, by.clamp(ay,ay+ah-height) as i32,
        width as u32,height as u32,on_left)
}

fn content(revision: u64, resting: bool, quiet: bool, sending: bool, reply: Option<&str>) -> Content {
    let reply = reply.filter(|s| !s.trim().is_empty());
    let is_reply = !resting && !quiet && !sending && reply.is_some();
    let text = if resting { "休息中 · Zzz".into() }
        else if sending { "正在回复…".into() }
        else if quiet { "勿扰中".into() }
        else if let Some(reply) = reply { reply.chars().take(400).collect() }
        else { "待机中".into() };
    Content { revision, text, reply: is_reply, left: false }
}

pub fn sync(app: &AppHandle, revision: u64, resting: bool, quiet: bool, sending: bool, reply: Option<&str>) -> Result<(),AppError> {
    {
        let state = app.state::<Mutex<BubbleState>>();
        let mut state = state.lock().map_err(failure)?;
        if revision < state.content.revision { return Ok(()); }
        state.content = content(revision,resting,quiet,sending,reply);
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = refresh(&handle) { eprintln!("{}",error.message); }
    }).map_err(failure)
}

pub fn refresh(app: &AppHandle) -> Result<(),AppError> {
    let Some(bubble) = app.get_webview_window("bubble") else { return Ok(()); };
    let Some(pet) = app.get_webview_window("pet") else { return Ok(()); };
    let (mut payload, anchor) = {
        let state = app.state::<Mutex<BubbleState>>();
        let state = state.lock().map_err(failure)?;
        if !state.ready { return Ok(()); }
        if state.calibrating || state.anchor.is_none() || state.model.is_none() { return bubble.hide().map_err(failure); }
        let model = state.model.as_ref().unwrap();
        let point = state.anchor.unwrap();
        (state.content.clone(),map_anchor(model,point))
    };
    if !pet.is_visible().map_err(failure)? || pet.is_minimized().map_err(failure)? {
        return bubble.hide().map_err(failure);
    }
    let scale = {
        let state = app.state::<Mutex<ShellState>>();
        let state = state.lock().map_err(failure)?;
        if state.scaling { return Ok(()); }
        state.scales.pet_percent as f64 / 100.0
    };
    let dpi = pet.scale_factor().map_err(failure)?;
    let monitor = pet.current_monitor().map_err(failure)?.ok_or_else(|| failure("monitor"))?;
    let area = monitor.work_area();
    let pos = pet.outer_position().map_err(failure)?;
    let size = pet.inner_size().map_err(failure)?;
    let logical = if payload.reply { (244.0,136.0) } else { (118.0,52.0) };
    let rect = placement((pos.x,pos.y,size.width,size.height),anchor,
        ((logical.0*scale*dpi).round() as u32,(logical.1*scale*dpi).round() as u32),
        (area.position.x,area.position.y,area.size.width,area.size.height),(22.0*scale*dpi).round() as i32);
    // set_zoom is relative to this WebView's DPI, which may differ across monitors.
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOCOPYBITS};
        let hwnd = bubble.hwnd().map_err(failure)?;
        if unsafe { SetWindowPos(hwnd.0 as _,std::ptr::null_mut(),rect.0,rect.1,rect.2 as i32,rect.3 as i32,
            SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOCOPYBITS) } == 0 { return Err(failure("SetWindowPos")); }
    }
    #[cfg(not(windows))]
    {
        bubble.set_size(tauri::PhysicalSize::new(rect.2,rect.3)).map_err(failure)?;
        bubble.set_position(tauri::PhysicalPosition::new(rect.0,rect.1)).map_err(failure)?;
    }
    bubble.set_zoom(scale*dpi/bubble.scale_factor().map_err(failure)?).map_err(failure)?;
    payload.left = rect.4;
    bubble.emit("bubble-content",payload).map_err(failure)?;
    bubble.show().map_err(failure)?;
    Ok(())
}

fn map_anchor(model: &ModelLayout, point: BubbleAnchor) -> BubbleAnchor {
    BubbleAnchor { x:model.x+point.x*model.width, y:model.y+point.y*model.height }
}
fn calibration(state: &BubbleState) -> Calibration {
    Calibration {model_ready:state.model.is_some(),calibrated:state.anchor.is_some(),active:state.calibrating}
}
fn notify_calibration(app: &AppHandle) -> Result<(),AppError> {
    let status = calibration(&*app.state::<Mutex<BubbleState>>().lock().map_err(failure)?);
    app.emit_to("pet","calibration-mode",status.active).map_err(failure)?;
    app.emit_to("main","bubble-calibration",status).map_err(failure)
}
async fn update_ui(window: &WebviewWindow, show_settings: bool) -> Result<(),AppError> {
    let app = window.app_handle().clone();
    let (done,result) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || {
        let outcome = (|| { notify_calibration(&app)?; refresh(&app)?;
            if show_settings { apply_mode(&app,ShellMode::Settings)?; } Ok(()) })();
        let _ = done.send(outcome);
    }).map_err(failure)?;
    result.await.map_err(failure)?
}
#[tauri::command]
pub async fn register_bubble_model(window: WebviewWindow, app: tauri::State<'_,Arc<Application>>, layout: ModelLayout) -> Result<(),AppError> {
    if window.label() != "pet" { return Err(AppError::new("permission","此窗口不能注册角色。")); }
    if layout.model_id.len()!=64 || !layout.model_id.bytes().all(|b| b.is_ascii_hexdigit()) ||
        ![layout.x,layout.y,layout.width,layout.height].iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0) ||
        layout.width == 0.0 || layout.height == 0.0 || layout.x+layout.width > 1.001 || layout.y+layout.height > 1.001 {
        return Err(AppError::new("anchor","角色布局数据无效。"));
    }
    if let Some(point) = layout.default_anchor { point.validate()?; }
    let id = layout.model_id.clone();
    let saved = access(&app,move |c| c.store.bubble_anchor(&id)).await?;
    {
        let state = window.state::<Mutex<BubbleState>>();
        let mut state = state.lock().map_err(failure)?;
        if state.model.as_ref().map(|m| &m.model_id) != Some(&layout.model_id) {
            state.anchor = saved.or(layout.default_anchor);
            state.calibrating = false;
        }
        state.model = Some(layout);
    }
    update_ui(&window,false).await
}
#[tauri::command]
pub fn get_bubble_calibration(window: WebviewWindow) -> Result<Calibration,AppError> {
    if window.label() != "main" { return Err(AppError::new("permission","此窗口不能读取校准设置。")); }
    Ok(calibration(&*window.state::<Mutex<BubbleState>>().lock().map_err(failure)?))
}
#[tauri::command]
pub async fn start_bubble_calibration(window: WebviewWindow) -> Result<(),AppError> {
    if window.label() != "main" { return Err(AppError::new("permission","此窗口不能开始校准。")); }
    {
        let state = window.state::<Mutex<BubbleState>>();
        let mut state = state.lock().map_err(failure)?;
        if state.model.is_none() { return Err(AppError::new("anchor","请等待角色加载完成。")); }
        state.calibrating = true;
    }
    crate::window_shell::set_shell_mode(window.clone(),ShellMode::Idle).await?;
    window.app_handle().get_webview_window("pet").ok_or_else(|| failure("pet"))?.set_focus().map_err(failure)?;
    update_ui(&window,false).await
}
#[tauri::command]
pub async fn finish_bubble_calibration(window: WebviewWindow, app: tauri::State<'_,Arc<Application>>, point: Option<BubbleAnchor>) -> Result<(),AppError> {
    if window.label() != "pet" { return Err(AppError::new("permission","请在角色窗口校准。")); }
    let id = {
        let state = window.state::<Mutex<BubbleState>>();
        let state = state.lock().map_err(failure)?;
        if !state.calibrating { return Err(AppError::new("anchor","请先在设置中开始校准。")); }
        state.model.as_ref().ok_or_else(|| failure("model"))?.model_id.clone()
    };
    if let Some(point) = point {
        point.validate()?;
        let save_id = id.clone();
        access(&app,move |c| c.store.save_bubble_anchor(&save_id,point)).await?;
    }
    {
        let state = window.state::<Mutex<BubbleState>>();
        let mut state = state.lock().map_err(failure)?;
        if state.model.as_ref().map(|m| &m.model_id) != Some(&id) { return Err(AppError::new("anchor","角色已切换，请重新校准。")); }
        if let Some(point) = point { state.anchor = Some(point); }
        state.calibrating = false;
    }
    update_ui(&window,true).await
}

#[tauri::command]
pub async fn bubble_ready(window: WebviewWindow) -> Result<(),AppError> {
    if window.label() != "bubble" { return Err(AppError::new("permission","此窗口不是气泡窗口。")); }
    window.state::<Mutex<BubbleState>>().lock().map_err(failure)?.ready = true;
    let app = window.app_handle().clone();
    let (done,result) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || { let _ = done.send(refresh(&app)); }).map_err(failure)?;
    result.await.map_err(failure)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn right_then_left_and_top_edge() {
        let anchor = BubbleAnchor{x:0.5,y:0.2};
        assert_eq!(placement((800,500,180,180),anchor,(244,136),(0,0,1920,1040),22),(912,420,244,136,false));
        assert_eq!(placement((1740,0,180,180),anchor,(244,136),(0,0,1920,1040),22),(1564,0,244,136,true));
        assert_eq!(placement((-1920,0,180,180),anchor,(118,52),(-1920,0,1920,1040),22),(-1808,4,118,52,false));
    }
    #[test]
    fn narrow_space_and_scaled_sizes_stay_on_screen() {
        for scale in [0.75,1.0,1.5,2.0] {
            let size = (180.0*scale) as u32;
            for x in [0,100,300] {
                let (bx,by,w,h,_) = placement((x,0,size,size),BubbleAnchor{x:0.5,y:0.2},((244.0*scale) as u32,(136.0*scale) as u32),(0,0,700,400),22);
                assert!(bx >= 0 && by >= 0 && bx as u32+w <= 700 && by as u32+h <= 400);
            }
        }
        assert_eq!(placement((140,80,180,180),BubbleAnchor{x:0.5,y:0.2},(244,136),(0,0,440,380),22),(0,0,208,136,true));
    }
    #[test]
    fn bubble_tracks_head_anchor_at_every_user_scale() {
        for percent in [75,100,125,150,175,200] {
            let size = 180*percent/100;
            let height = 52*percent/100;
            let gap = (22*percent/100) as i32;
            let (x,y,_,_,left) = placement((800,500,size,size),BubbleAnchor{x:0.5,y:0.2},(118*percent/100,height),(0,0,2400,1600),gap);
            assert!(!left);
            assert_eq!(x,800+(size as f64/2.0).round() as i32+gap);
            assert_eq!(y+height as i32-(size/9) as i32,500+(size/5) as i32);
            assert!(x < 800+size as i32,"Bubble must use transparent space next to the head");
        }
    }
    #[test]
    fn model_space_anchor_maps_through_fitted_canvas() {
        let model = ModelLayout {model_id:"test".into(),x:0.25,y:0.04,width:0.5,height:0.92,default_anchor:None};
        let point = map_anchor(&model,BubbleAnchor{x:0.6,y:0.25});
        assert!((point.x-0.55).abs()<1e-10);
        assert!((point.y-0.27).abs()<1e-10);
        assert!(BubbleAnchor{x:1.1,y:0.5}.validate().is_err());
        assert!(BubbleAnchor{x:0.5,y:f64::INFINITY}.validate().is_err());
    }
    #[test]
    fn state_priority_and_bounded_reply_without_provider() {
        assert_eq!(content(1,true,false,false,Some("reply")).text,"休息中 · Zzz");
        assert!(!content(2,false,true,false,Some("reply")).reply);
        assert_eq!(content(3,false,false,true,None).text,"正在回复…");
        let c = content(4,false,false,false,Some(&"好".repeat(500)));
        assert!(c.reply); assert_eq!(c.text.chars().count(),400);
        assert_eq!(content(5,false,false,false,None).text,"待机中");
    }
}
