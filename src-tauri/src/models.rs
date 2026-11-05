use std::{collections::BTreeSet, fs, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager, WebviewWindow};
use crate::{application::{access, Application}, domain::AppError};

pub const BUILTIN: &str = "builtin-atri";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry { pub id: String, pub name: String }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection { pub id: String, pub revision: u64 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog { models: Vec<ModelEntry>, selected: Selection }
pub struct Library { pub selection: Mutex<Selection>, pub root: PathBuf }

fn error(message: &'static str) -> AppError { AppError::new("live2d", message) }
fn io_error(e: impl std::fmt::Display) -> AppError { eprintln!("Live2D 文件操作失败：{e}"); error("模型文件操作失败，请检查文件是否存在及目录权限。") }
fn main_only(window: &WebviewWindow) -> Result<(), AppError> {
    if window.label() != "main" { return Err(error("请在设置中管理角色模型。")); } Ok(())
}
pub fn valid_id(id: &str) -> bool { id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn relative(path: &str) -> Result<PathBuf, AppError> {
    if path.is_empty() || path.chars().any(|c| c.is_control() || "\\:%?#<>|\"".contains(c)) ||
        path.split('/').any(|part| part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])) {
        return Err(error("模型资源路径不受支持，请使用模型文件夹内的相对路径。"));
    }
    Ok(PathBuf::from(path))
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Descriptor { version: u32, file_references: References }
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct References {
    moc: String, textures: Vec<String>, physics: Option<String>, pose: Option<String>,
    display_info: Option<String>, user_data: Option<String>,
    #[serde(default)] expressions: Vec<Expression>,
    #[serde(default)] motions: std::collections::BTreeMap<String, Vec<Motion>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Expression { file: String }
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Motion { file: String, sound: Option<String> }
fn references(bytes: &[u8]) -> Result<(String, BTreeSet<String>), AppError> {
    let data: Descriptor = serde_json::from_slice(bytes).map_err(|_| error("请选择 Cubism 的 model3.json 文件（需包含 Version 和 FileReferences）。"))?;
    if data.version != 3 || data.file_references.textures.is_empty() { return Err(error("仅支持含纹理的 Cubism model3 格式。")); }
    let r = data.file_references;
    let mut paths = BTreeSet::from([r.moc.clone()]);
    paths.extend(r.textures);
    paths.extend([r.physics, r.pose, r.display_info, r.user_data].into_iter().flatten());
    paths.extend(r.expressions.into_iter().map(|e| e.file));
    for motion in r.motions.into_values().flatten() { paths.insert(motion.file); if let Some(sound) = motion.sound { paths.insert(sound); } }
    if paths.len() > 2048 { return Err(error("模型引用文件过多（最多 2048 个）。")); }
    for path in &paths { relative(path)?; if path.eq_ignore_ascii_case("model.json") { return Err(error("资源文件与导入后的 model.json 名称冲突。")); } }
    Ok((r.moc, paths))
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AppError> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(io_error)?;
    if file.metadata().map_err(io_error)?.len() > limit { return Err(error("模型资源文件超过大小限制。")); }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > limit { return Err(error("模型资源文件超过大小限制。")); }
    Ok(bytes)
}

// Only referenced resources inside the selected model's directory are copied.
pub fn import_package(source: &Path, library: &Path) -> Result<ModelEntry, AppError> {
    let source = source.canonicalize().map_err(io_error)?;
    let base = source.parent().ok_or_else(|| error("无法读取模型目录。"))?;
    let descriptor = read_bounded(&source, 4 * 1024 * 1024)?;
    let (moc, paths) = references(&descriptor)?;
    fs::create_dir_all(library).map_err(io_error)?;
    let staging = tempfile::Builder::new().prefix(".import-").tempdir_in(library).map_err(io_error)?;
    fs::write(staging.path().join("model.json"), &descriptor).map_err(io_error)?;
    let mut hash = Sha256::new(); hash.update((descriptor.len() as u64).to_le_bytes()); hash.update(&descriptor);
    let mut total = descriptor.len();
    for name in paths {
        let resource = base.join(relative(&name)?).canonicalize().map_err(|_| { eprintln!("缺少 Live2D 资源：{name}"); error("模型引用的资源缺失，请保留完整的纹理、动作和物理文件后重试。") })?;
        if !resource.starts_with(base) { return Err(error("模型资源超出所选模型目录，无法导入。")); }
        let bytes = read_bounded(&resource, 128 * 1024 * 1024)?;
        total += bytes.len();
        if total > 512 * 1024 * 1024 { return Err(error("模型总大小超过 512 MB。")); }
        if name == moc && !bytes.starts_with(b"MOC3") { return Err(error("MOC 文件不是受支持的 Cubism 模型。")); }
        hash.update((name.len() as u64).to_le_bytes()); hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes()); hash.update(&bytes);
        let dest = staging.path().join(&name);
        fs::create_dir_all(dest.parent().unwrap()).map_err(io_error)?;
        fs::write(dest, bytes).map_err(io_error)?;
    }
    let id = format!("{:x}", hash.finalize());
    let dest = library.join(&id);
    if !dest.exists() { fs::rename(staging.path(), &dest).map_err(io_error)?; }
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("Live2D").trim_end_matches(".model3");
    let name = if ["model", "model0", "model3"].contains(&stem) { base.file_name().and_then(|s| s.to_str()).unwrap_or(stem) } else { stem };
    Ok(ModelEntry { id, name: name.to_owned() })
}

#[cfg(windows)]
fn choose_file(owner: isize) -> Result<Option<PathBuf>, AppError> {
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    let filter: Vec<u16> = "Live2D 模型 JSON\0*.json\0\0".encode_utf16().collect();
    let title: Vec<u16> = "选择 Live2D 模型 JSON（保持资源目录完整）\0".encode_utf16().collect();
    let mut filename = vec![0u16; 32768];
    let mut dialog: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = owner as _; dialog.lpstrFilter = filter.as_ptr(); dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrFile = filename.as_mut_ptr(); dialog.nMaxFile = filename.len() as u32;
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER | OFN_DONTADDTORECENT;
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        let code = unsafe { CommDlgExtendedError() };
        return if code == 0 { Ok(None) } else { Err(io_error(format!("模型选择器错误 {code}"))) };
    }
    use std::os::windows::ffi::OsStringExt;
    let end = filename.iter().position(|v| *v == 0).unwrap_or(filename.len());
    Ok(Some(std::ffi::OsString::from_wide(&filename[..end]).into()))
}

#[tauri::command]
pub async fn import_live2d(window: WebviewWindow, app: tauri::State<'_, Arc<Application>>) -> Result<Option<ModelEntry>, AppError> {
    main_only(&window)?;
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(io_error)?.0 as isize;
        let root = window.state::<Library>().root.clone();
        let entry = tauri::async_runtime::spawn_blocking(move || {
            choose_file(owner)?.map(|path| import_package(&path, &root)).transpose()
        }).await.map_err(io_error)??;
        if let Some(entry) = entry.clone() { access(&app, move |c| c.store.add_live2d(&entry)).await?; }
        Ok(entry)
    }
    #[cfg(not(windows))]
    { let _ = app; Err(error("当前导入选择器仅支持 Windows。")) }
}
#[tauri::command]
pub fn get_selected_live2d(window: WebviewWindow) -> Result<Selection, AppError> {
    if window.label() != "main" && window.label() != "pet" { return Err(error("此窗口不能读取角色选择。")); }
    Ok(window.state::<Library>().selection.lock().map_err(io_error)?.clone())
}
#[tauri::command]
pub async fn get_live2d_catalog(window: WebviewWindow, app: tauri::State<'_, Arc<Application>>) -> Result<Catalog, AppError> {
    main_only(&window)?;
    let mut models = vec![ModelEntry {id: BUILTIN.into(), name: "ATRI（内置）".into()}];
    models.extend(access(&app, |c| c.store.live2d_models()).await?);
    Ok(Catalog {models, selected: get_selected_live2d(window)?})
}
#[tauri::command]
pub async fn select_live2d(window: WebviewWindow, app: tauri::State<'_, Arc<Application>>, id: String) -> Result<Selection, AppError> {
    main_only(&window)?;
    let handle = window.app_handle().clone();
    // Persist and publish selection under the same lock used by layout registration.
    let selection = access(&app, move |c| {
        let library = handle.state::<Library>();
        let mut current = library.selection.lock().map_err(io_error)?;
        if id != BUILTIN && !c.store.live2d_models()?.iter().any(|m| m.id == id) { return Err(error("所选角色不存在，请重新导入。")); }
        if current.id == id { return Ok(current.clone()); }
        c.store.select_live2d(&id)?;
        current.id = id; current.revision += 1;
        crate::bubble::clear_model(&handle)?;
        Ok(current.clone())
    }).await?;
    let handle = window.app_handle().clone();
    let payload = selection.clone();
    let (done, result) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || {
        let outcome = (|| {
            crate::bubble::selection_changed(&handle)?;
            handle.emit("model-selection", payload).map_err(io_error)
        })();
        let _ = done.send(outcome);
    }).map_err(io_error)?;
    result.await.map_err(io_error)??;
    Ok(selection)
}

pub fn asset(root: &Path, uri: &str) -> Result<(Vec<u8>, &'static str), AppError> {
    let decoded = percent_encoding::percent_decode_str(uri.trim_start_matches('/')).decode_utf8().map_err(io_error)?;
    let (id, name) = decoded.split_once('/').ok_or_else(|| error("无效模型路径。"))?;
    if !valid_id(id) { return Err(error("无效模型编号。")); }
    let relative = relative(name)?;
    let base = root.join(id).canonicalize().map_err(io_error)?;
    let path = base.join(relative).canonicalize().map_err(io_error)?;
    if !path.starts_with(&base) { return Err(error("模型资源越界。")); }
    let descriptor = read_bounded(&base.join("model.json"), 4 * 1024 * 1024)?;
    if name != "model.json" && !references(&descriptor)?.1.contains(name) { return Err(error("未引用的模型资源。")); }
    let mime = match path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "json" => "application/json", "png" => "image/png", "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp", "wav" => "audio/wav", "mp3" => "audio/mpeg", "ogg" => "audio/ogg", _ => "application/octet-stream"
    };
    Ok((read_bounded(&path, 128 * 1024 * 1024)?, mime))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(dir: &Path, moc: &str) -> PathBuf {
        fs::write(dir.join("sample.model3.json"), format!(r#"{{"Version":3,"FileReferences":{{"Moc":"{moc}","Textures":["tex.png"]}}}}"#)).unwrap();
        fs::write(dir.join("sample.moc3"), b"MOC3-test").unwrap(); fs::write(dir.join("tex.png"), b"texture").unwrap();
        dir.join("sample.model3.json")
    }
    #[test]
    fn import_is_deduplicated_copied_and_scoped() {
        let source = tempfile::tempdir().unwrap(); let library = tempfile::tempdir().unwrap();
        let path = fixture(source.path(), "sample.moc3");
        let a = import_package(&path, library.path()).unwrap(); let b = import_package(&path, library.path()).unwrap();
        assert_eq!(a.id, b.id); assert_eq!(a.name, "sample");
        fs::remove_file(source.path().join("sample.moc3")).unwrap();
        assert_eq!(asset(library.path(), &format!("/{}/sample.moc3", a.id)).unwrap().0, b"MOC3-test");
        for path in [format!("/{}/../secret",a.id), format!("/{}/%2e%2e/secret",a.id), format!("/{}/missing",a.id)] {
            assert!(asset(library.path(), &path).is_err());
        }
    }
    #[test]
    fn invalid_or_missing_resources_do_not_publish_packages() {
        let source = tempfile::tempdir().unwrap(); let library = tempfile::tempdir().unwrap();
        for moc in ["missing.moc3", "../sample.moc3", "https://example.com/a", "C:/secret", "/absolute"] {
            let path = fixture(source.path(), moc); assert!(import_package(&path, library.path()).is_err());
            assert_eq!(fs::read_dir(library.path()).unwrap().count(), 0);
        }
        let path = fixture(source.path(), "sample.moc3"); fs::write(source.path().join("sample.moc3"), b"invalid").unwrap();
        assert!(import_package(&path, library.path()).is_err());
    }
}
