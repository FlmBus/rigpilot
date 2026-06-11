mod definition;
mod export;
mod project;

use definition::DefinitionInfo;
use export::ExportOptions;
use project::Project;

#[tauri::command]
fn list_definitions() -> Result<Vec<DefinitionInfo>, String> {
    Ok(definition::bundled()?.iter().map(definition::info).collect())
}

#[tauri::command]
fn save_project(path: String, project: Project) -> Result<(), String> {
    project::save(&path, &project)
}

#[tauri::command]
fn load_project(path: String) -> Result<Project, String> {
    project::load(&path)
}

/// Copies an audio file into the project's assets folder (always-copy-on-import,
/// see docs/plan.md §1.1). Returns the path relative to the project file.
#[tauri::command]
fn import_audio(project_path: String, source: String) -> Result<String, String> {
    let project = std::path::Path::new(&project_path);
    let dir = project
        .parent()
        .ok_or("project file has no parent directory")?;
    let file_name = std::path::Path::new(&source)
        .file_name()
        .ok_or("source has no file name")?
        .to_string_lossy()
        .to_string();

    let assets = dir.join("assets");
    std::fs::create_dir_all(&assets).map_err(|e| format!("could not create assets dir: {e}"))?;

    // Find a free name if a different file with the same name is already there.
    let src_len = std::fs::metadata(&source).map_err(|e| e.to_string())?.len();
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (file_name.clone(), String::new()),
    };
    let mut candidate = file_name;
    let mut n = 2;
    loop {
        let target = assets.join(&candidate);
        match std::fs::metadata(&target) {
            Err(_) => {
                std::fs::copy(&source, &target).map_err(|e| format!("copy failed: {e}"))?;
                break;
            }
            Ok(meta) if meta.len() == src_len => break, // same file already imported
            Ok(_) => {
                candidate = format!("{stem} ({n}){ext}");
                n += 1;
            }
        }
    }
    Ok(format!("assets/{candidate}"))
}

#[tauri::command]
fn export_midi(
    project: Project,
    out_dir: String,
    options: Option<ExportOptions>,
) -> Result<Vec<String>, String> {
    export::export(&project, &out_dir, &options.unwrap_or_default())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_definitions,
            save_project,
            load_project,
            import_audio,
            export_midi
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
