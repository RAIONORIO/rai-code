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

#[derive(Serialize)]
struct AnalysisIssue {
    id: String,
    title: String,
    severity: String,
    description: String,
    suggestion: String,
    matched_rule: String,
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

fn file_extension(file_path: &str) -> String {
    Path::new(file_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{}", extension.to_lowercase()))
        .unwrap_or_default()
}

fn has_react_named_import(content: &str, name: &str) -> bool {
    content.lines().any(|line| {
        let clean_line = line.trim();

        clean_line.starts_with("import")
            && (clean_line.contains("from \"react\"") || clean_line.contains("from 'react'"))
            && clean_line.contains(name)
    })
}

fn add_analysis_issue(
    issues: &mut Vec<AnalysisIssue>,
    id: &str,
    title: &str,
    severity: &str,
    description: &str,
    suggestion: &str,
    matched_rule: &str,
) {
    issues.push(AnalysisIssue {
        id: id.to_string(),
        title: title.to_string(),
        severity: severity.to_string(),
        description: description.to_string(),
        suggestion: suggestion.to_string(),
        matched_rule: matched_rule.to_string(),
    });
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

#[tauri::command]
fn analyze_project_file(
    file_path: String,
    relative_path: String,
    file_content: String,
) -> Result<Vec<AnalysisIssue>, String> {
    let extension = file_extension(&file_path);
    let analyzed_file = if relative_path.trim().is_empty() {
        file_path
    } else {
        relative_path
    };

    let mut issues = Vec::new();

    if extension == ".ts" || extension == ".tsx" {
        if file_content.contains("useState(") && !has_react_named_import(&file_content, "useState")
        {
            add_analysis_issue(
                &mut issues,
                "typescript-react-usestate-missing-import",
                "useState usado sem importação",
                "error",
                "O arquivo usa useState, mas não encontrei useState importado do React.",
                "Adicione useState no import do React.",
                "knowledge/rules/typescript-rules.json",
            );
        }

        if file_content.contains("useMemo(") && !has_react_named_import(&file_content, "useMemo") {
            add_analysis_issue(
                &mut issues,
                "typescript-react-usememo-missing-import",
                "useMemo usado sem importação",
                "error",
                "O arquivo usa useMemo, mas não encontrei useMemo importado do React.",
                "Adicione useMemo no import do React.",
                "knowledge/rules/typescript-rules.json",
            );
        }

        if file_content.contains("invoke(") || file_content.contains("invoke<") {
            let description = format!(
                "O arquivo {} chama invoke do Tauri. O comando chamado precisa existir no backend Rust e estar registrado no invoke_handler.",
                analyzed_file
            );

            add_analysis_issue(
                &mut issues,
                "typescript-tauri-invoke-check",
                "Chamada invoke precisa existir no backend Rust",
                "warning",
                &description,
                "Confira se o comando existe em src-tauri/src/lib.rs e se está dentro de tauri::generate_handler!.",
                "knowledge/rules/typescript-rules.json",
            );
        }
    }

    if extension == ".css" {
        if file_content.contains(".chat-panel") && file_content.contains("display: none") {
            add_analysis_issue(
                &mut issues,
                "css-chat-panel-hidden",
                "Chat pode estar sendo escondido pelo CSS",
                "warning",
                "O CSS contém .chat-panel junto com display: none. Isso pode esconder o painel de chat.",
                "Remova display: none do .chat-panel ou revise a media query responsável.",
                "knowledge/rules/css-rules.json",
            );
        }

        if file_content.contains("display: flex")
            && !file_content.contains("min-width: 0")
            && file_content.contains("overflow")
        {
            add_analysis_issue(
                &mut issues,
                "css-flex-overflow-min-width",
                "Layout flex pode quebrar com conteúdo longo",
                "info",
                "O arquivo usa flex e overflow, mas pode precisar de min-width: 0 em painéis internos.",
                "Verifique os containers flex principais e adicione min-width: 0 onde o conteúdo estiver empurrando o layout.",
                "knowledge/rules/css-rules.json",
            );
        }
    }

    Ok(issues)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_project_files,
            read_project_file,
            analyze_project_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}