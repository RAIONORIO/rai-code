use serde::Serialize;
use serde_json::Value;
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
    line: Option<usize>,
    matched_text: Option<String>,
}

#[derive(Clone)]
struct RuleMatch {
    line: Option<usize>,
    matched_text: Option<String>,
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

fn infer_project_root(file_path: &str, relative_path: &str) -> PathBuf {
    let full_path = PathBuf::from(file_path);

    if relative_path.trim().is_empty() {
        return full_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
    }

    let relative_components = Path::new(relative_path).components().count();
    let mut root = full_path.clone();

    for _ in 0..relative_components {
        root.pop();
    }

    if root.as_os_str().is_empty() {
        full_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        root
    }
}

fn has_knowledge_index(path: &Path) -> bool {
    path.join("knowledge").join("index.json").exists()
}

fn find_root_with_knowledge(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    for candidate in candidates {
        let start = if candidate.is_file() {
            candidate
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        } else {
            candidate
        };

        for ancestor in start.ancestors() {
            if has_knowledge_index(ancestor) {
                return Some(ancestor.to_path_buf());
            }
        }
    }

    None
}

fn resolve_project_root(project_root: &str, file_path: &str, relative_path: &str) -> PathBuf {
    let mut candidates = Vec::new();

    if !project_root.trim().is_empty() {
        candidates.push(PathBuf::from(project_root));
    }

    candidates.push(infer_project_root(file_path, relative_path));

    let file_path_buf = PathBuf::from(file_path);

    if let Some(parent) = file_path_buf.parent() {
        candidates.push(parent.to_path_buf());
    }

    find_root_with_knowledge(candidates).unwrap_or_else(|| {
        if !project_root.trim().is_empty() {
            PathBuf::from(project_root)
        } else {
            infer_project_root(file_path, relative_path)
        }
    })
}

fn string_from_value(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(boolean)) => Some(boolean.to_string()),
        _ => None,
    }
}

fn strings_from_value(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) => vec![text.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| string_from_value(Some(item)))
            .collect(),
        _ => Vec::new(),
    }
}

fn strings_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let mut result = Vec::new();

    for key in keys {
        result.extend(strings_from_value(value.get(*key)));
    }

    result
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("Não foi possível ler {}: {}", path.display(), error))?;

    serde_json::from_str::<Value>(&content)
        .map_err(|error| format!("JSON inválido em {}: {}", path.display(), error))
}

fn fallback_language_by_extension(extension: &str) -> Option<String> {
    match extension {
        ".js" | ".mjs" | ".cjs" | ".jsx" => Some("javascript".to_string()),
        ".ts" | ".tsx" => Some("typescript".to_string()),
        ".html" | ".htm" => Some("html".to_string()),
        ".css" => Some("css".to_string()),
        ".rs" => Some("rust".to_string()),
        ".py" => Some("python".to_string()),
        ".java" => Some("java".to_string()),
        _ => None,
    }
}

fn language_from_index(index_json: &Value, extension: &str) -> Option<(String, Value)> {
    let languages = index_json.get("languages")?.as_object()?;

    for (language_name, config) in languages {
        let extensions = strings_from_value(config.get("extensions"));

        if extensions
            .iter()
            .any(|item| item.eq_ignore_ascii_case(extension))
        {
            return Some((language_name.clone(), config.clone()));
        }
    }

    None
}

fn resolve_knowledge_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);

    if path.is_absolute() {
        return path;
    }

    let normalized = raw_path.replace('\\', "/");

    if normalized.starts_with("knowledge/") {
        project_root.join(raw_path)
    } else {
        project_root.join("knowledge").join(raw_path)
    }
}

fn fallback_rules_path(project_root: &Path, language: &str) -> PathBuf {
    project_root
        .join("knowledge")
        .join("rules")
        .join(format!("{}-rules.json", language))
}

fn extract_rules(rules_json: &Value) -> Vec<Value> {
    match rules_json {
        Value::Array(items) => items.clone(),
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("rules") {
                return items.clone();
            }

            if let Some(Value::Array(items)) = map.get("items") {
                return items.clone();
            }

            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn suggestion_from_rule(rule: &Value) -> String {
    match rule.get("suggestion") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(suggestion)) => string_from_value(
            suggestion
                .get("message")
                .or_else(|| suggestion.get("text"))
                .or_else(|| suggestion.get("description")),
        )
        .unwrap_or_else(|| "Revise o trecho indicado antes de alterar o arquivo.".to_string()),
        _ => string_from_value(rule.get("fix"))
            .or_else(|| string_from_value(rule.get("recommendation")))
            .unwrap_or_else(|| "Revise o trecho indicado antes de alterar o arquivo.".to_string()),
    }
}

fn rule_id(rule: &Value) -> String {
    string_from_value(rule.get("id"))
        .or_else(|| string_from_value(rule.get("code")))
        .or_else(|| string_from_value(rule.get("name")))
        .unwrap_or_else(|| "regra-sem-id".to_string())
}

fn issue_from_rule(rule: &Value, matched_rule: &str, rule_match: RuleMatch) -> AnalysisIssue {
    AnalysisIssue {
        id: rule_id(rule),
        title: string_from_value(rule.get("title"))
            .or_else(|| string_from_value(rule.get("name")))
            .unwrap_or_else(|| "Problema encontrado".to_string()),
        severity: string_from_value(rule.get("severity"))
            .or_else(|| string_from_value(rule.get("level")))
            .unwrap_or_else(|| "info".to_string()),
        description: string_from_value(rule.get("description"))
            .or_else(|| string_from_value(rule.get("explanation")))
            .or_else(|| string_from_value(rule.get("message")))
            .unwrap_or_else(|| "Uma regra local encontrou um possível problema.".to_string()),
        suggestion: suggestion_from_rule(rule),
        matched_rule: matched_rule.to_string(),
        line: rule_match.line,
        matched_text: rule_match.matched_text,
    }
}

fn internal_issue(
    id: &str,
    title: &str,
    severity: &str,
    description: &str,
    suggestion: &str,
    matched_rule: &str,
) -> AnalysisIssue {
    AnalysisIssue {
        id: id.to_string(),
        title: title.to_string(),
        severity: severity.to_string(),
        description: description.to_string(),
        suggestion: suggestion.to_string(),
        matched_rule: matched_rule.to_string(),
        line: None,
        matched_text: None,
    }
}

fn find_line_info(file_content: &str, pattern: &str) -> Option<RuleMatch> {
    if pattern.trim().is_empty() {
        return None;
    }

    file_content
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(pattern))
        .map(|(index, line)| RuleMatch {
            line: Some(index + 1),
            matched_text: Some(line.trim().to_string()),
        })
}

fn find_first_existing_pattern(file_content: &str, patterns: &[String]) -> Option<RuleMatch> {
    for pattern in patterns {
        if let Some(found) = find_line_info(file_content, pattern) {
            return Some(found);
        }
    }

    None
}

fn first_missing_pattern(file_content: &str, patterns: &[String]) -> Option<String> {
    patterns
        .iter()
        .find(|pattern| !file_content.contains(pattern.as_str()))
        .cloned()
}

fn rule_matches(rule: &Value, extension: &str, file_content: &str) -> Option<RuleMatch> {
    let detect = rule.get("detect").unwrap_or(rule);

    let extensions = strings_from_value(detect.get("extensions"));

    if !extensions.is_empty()
        && !extensions
            .iter()
            .any(|item| item.eq_ignore_ascii_case(extension))
    {
        return None;
    }

    let contains_any = strings_from_keys(
        detect,
        &[
            "contains",
            "containsAny",
            "contains_any",
            "pattern",
            "patterns",
        ],
    );

    let contains_all = strings_from_keys(
        detect,
        &[
            "containsAll",
            "contains_all",
            "mustContain",
            "must_contain",
            "required",
        ],
    );

    let not_contains_any = strings_from_keys(
        detect,
        &[
            "notContainsAny",
            "not_contains_any",
            "notContains",
            "not_contains",
        ],
    );

    let mut has_content_condition = false;
    let mut captured_match: Option<RuleMatch> = None;

    if !contains_any.is_empty() {
        has_content_condition = true;

        match find_first_existing_pattern(file_content, &contains_any) {
            Some(found) => {
                captured_match = Some(found);
            }
            None => return None,
        }
    }

    if !contains_all.is_empty() {
        has_content_condition = true;

        if first_missing_pattern(file_content, &contains_all).is_some() {
            return None;
        }

        if captured_match.is_none() {
            captured_match = find_first_existing_pattern(file_content, &contains_all);
        }
    }

    if !not_contains_any.is_empty() {
        has_content_condition = true;

        if find_first_existing_pattern(file_content, &not_contains_any).is_some() {
            return None;
        }

        if captured_match.is_none() {
            captured_match = Some(RuleMatch {
                line: None,
                matched_text: Some(format!(
                    "Ausente: {}",
                    not_contains_any.join(" | ")
                )),
            });
        }
    }

    if has_content_condition {
        return Some(captured_match.unwrap_or(RuleMatch {
            line: None,
            matched_text: None,
        }));
    }

    None
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
    project_root: String,
    file_path: String,
    relative_path: String,
    file_content: String,
) -> Result<Vec<AnalysisIssue>, String> {
    let extension = file_extension(&file_path);
    let resolved_root = resolve_project_root(&project_root, &file_path, &relative_path);
    let index_path = resolved_root.join("knowledge").join("index.json");

    if !index_path.exists() {
        return Ok(vec![internal_issue(
            "knowledge-index-not-found",
            "Base local não encontrada",
            "warning",
            &format!(
                "O Raí Code procurou a base local em: {}",
                index_path.display()
            ),
            "Confirme se a pasta knowledge está na raiz do projeto aberto ou em uma pasta acima do arquivo selecionado.",
            "knowledge/index.json",
        )]);
    }

    let index_json = read_json_file(&index_path)?;

    let language_result = language_from_index(&index_json, &extension).or_else(|| {
        fallback_language_by_extension(&extension).map(|language| (language, Value::Null))
    });

    let (language, language_config) = match language_result {
        Some(result) => result,
        None => {
            return Ok(vec![internal_issue(
                "language-not-supported",
                "Linguagem não suportada",
                "info",
                &format!(
                    "Ainda não existe mapeamento local para arquivos com extensão {}.",
                    extension
                ),
                "Adicione essa extensão em knowledge/index.json e crie o arquivo de regras correspondente.",
                "knowledge/index.json",
            )]);
        }
    };

    let rules_path = string_from_value(language_config.get("rules"))
        .map(|raw_path| resolve_knowledge_path(&resolved_root, &raw_path))
        .unwrap_or_else(|| fallback_rules_path(&resolved_root, &language));

    if !rules_path.exists() {
        return Ok(vec![internal_issue(
            "rules-file-not-found",
            "Arquivo de regras não encontrado",
            "warning",
            &format!("O arquivo de regras não foi encontrado em: {}", rules_path.display()),
            "Crie o arquivo de regras dessa linguagem dentro de knowledge/rules ou ajuste o caminho em knowledge/index.json.",
            &path_to_string(&rules_path),
        )]);
    }

    let rules_json = read_json_file(&rules_path)?;
    let rules = extract_rules(&rules_json);
    let rules_file_label = path_to_string(&rules_path);

    if rules.is_empty() {
        return Ok(vec![internal_issue(
            "rules-empty",
            "Nenhuma regra cadastrada",
            "info",
            &format!(
                "O arquivo de regras existe, mas ainda não possui regras em uma lista chamada rules. Linguagem identificada: {}.",
                language
            ),
            "Adicione regras no formato { \"rules\": [ ... ] }.",
            &rules_file_label,
        )]);
    }

    let issues = rules
        .iter()
        .filter_map(|rule| {
            rule_matches(rule, &extension, &file_content)
                .map(|rule_match| issue_from_rule(rule, &rules_file_label, rule_match))
        })
        .collect();

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