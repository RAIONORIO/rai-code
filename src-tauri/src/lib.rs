use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct FileEntry {
    name: String,
    path: String,
    relative_path: String,
    kind: String,
    depth: usize,
}

const MAX_ITEMS: usize = 800;
const MAX_DEPTH: usize = 6;
const MAX_FILE_SIZE_BYTES: u64 = 800_000;

fn should_ignore(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".git"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".vite"
            | ".turbo"
            | "__pycache__"
            | ".venv"
            | "venv"
    )
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn relative_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => relative.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

fn collect_entries(
    root: &Path,
    current_path: &Path,
    depth: usize,
    entries: &mut Vec<FileEntry>,
) -> Result<(), String> {
    if depth > MAX_DEPTH || entries.len() >= MAX_ITEMS {
        return Ok(());
    }

    let read_dir = fs::read_dir(current_path)
        .map_err(|error| format!("Não foi possível ler a pasta: {}", error))?;

    let mut items: Vec<PathBuf> = read_dir
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();

    items.sort_by(|a, b| {
        let a_is_dir = a.is_dir();
        let b_is_dir = b.is_dir();

        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .cmp(
                    &b.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase(),
                ),
        }
    });

    for path in items {
        if entries.len() >= MAX_ITEMS {
            break;
        }

        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if should_ignore(&name) {
            continue;
        }

        let kind = if path.is_dir() { "folder" } else { "file" };

        entries.push(FileEntry {
            name,
            path: path_to_string(&path),
            relative_path: relative_path(root, &path),
            kind: kind.to_string(),
            depth,
        });

        if path.is_dir() {
            collect_entries(root, &path, depth + 1, entries)?;
        }
    }

    Ok(())
}

#[tauri::command]
fn list_project_files(root_path: String) -> Result<Vec<FileEntry>, String> {
    let root = PathBuf::from(root_path);

    if !root.exists() {
        return Err("A pasta selecionada não existe.".to_string());
    }

    if !root.is_dir() {
        return Err("O caminho selecionado não é uma pasta.".to_string());
    }

    let mut entries = Vec::new();

    collect_entries(&root, &root, 0, &mut entries)?;

    Ok(entries)
}

#[tauri::command]
fn read_project_file(file_path: String) -> Result<String, String> {
    let path = PathBuf::from(file_path);

    if !path.exists() {
        return Err("O arquivo selecionado não existe.".to_string());
    }

    if !path.is_file() {
        return Err("O caminho selecionado não é um arquivo.".to_string());
    }

    let metadata =
        fs::metadata(&path).map_err(|error| format!("Erro ao ler metadados: {}", error))?;

    if metadata.len() > MAX_FILE_SIZE_BYTES {
        return Err("Arquivo muito grande para abrir nesta versão inicial.".to_string());
    }

    fs::read_to_string(&path).map_err(|_| {
        "Não foi possível ler esse arquivo como texto. Talvez seja binário ou tenha codificação incompatível."
            .to_string()
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_project_files,
            read_project_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}