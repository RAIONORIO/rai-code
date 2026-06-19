import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type ChatMessage = {
  role: "user" | "assistant";
  content: string;
};

type FileKind = "file" | "folder";

type FileItem = {
  name: string;
  path: string;
  relative_path: string;
  kind: FileKind;
  depth: number;
};

type AnalysisIssue = {
  id: string;
  title: string;
  severity: string;
  description: string;
  suggestion: string;
  matched_rule: string;
  line?: number | null;
  matched_text?: string | null;
};

function formatAnalysisMessage(
  issues: AnalysisIssue[],
  file: FileItem,
  fileSize: number,
  userRequest: string
) {
  if (issues.length === 0) {
    return `Análise local concluída.

Arquivo analisado:
${file.relative_path}

Tamanho:
${fileSize} caracteres

Pedido recebido:
${userRequest}

Resultado:
Nenhum problema foi encontrado pelas regras locais atuais.

Observação:
Isso não garante que o código esteja perfeito. Só significa que nenhuma regra cadastrada encontrou problema. A máquina obedeceu, não virou oráculo.

Nenhuma alteração foi aplicada no arquivo.`;
  }

  const issuesText = issues
    .map((issue, index) => {
      const lineText = issue.line ? String(issue.line) : "não identificada";
      const matchedText = issue.matched_text || "não informado";

      return `Problema ${index + 1}

Título:
${issue.title}

Severidade:
${issue.severity}

Linha:
${lineText}

Trecho encontrado:
${matchedText}

Descrição:
${issue.description}

Sugestão:
${issue.suggestion}

Regra consultada:
${issue.matched_rule}`;
    })
    .join("\n\n---\n\n");

  return `Análise local concluída.

Arquivo analisado:
${file.relative_path}

Tamanho:
${fileSize} caracteres

Pedido recebido:
${userRequest}

Problemas encontrados:
${issues.length}

${issuesText}

Nenhuma alteração foi aplicada no arquivo.`;
}

function App() {
  const [projectPath, setProjectPath] = useState("");
  const [files, setFiles] = useState<FileItem[]>([]);
  const [selectedFile, setSelectedFile] = useState<FileItem | null>(null);
  const [fileContent, setFileContent] = useState("");
  const [chatText, setChatText] = useState("");
  const [isLoadingProject, setIsLoadingProject] = useState(false);
  const [isLoadingFile, setIsLoadingFile] = useState(false);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  const [messages, setMessages] = useState<ChatMessage[]>([
    {
      role: "assistant",
      content:
        "Interface pronta. Clique em Abrir Projeto para escolher uma pasta real do computador.",
    },
  ]);

  const projectName = useMemo(() => {
    if (!projectPath) {
      return "Nenhum projeto aberto";
    }

    const normalizedPath = projectPath.replace(/\\/g, "/");
    const parts = normalizedPath.split("/").filter(Boolean);

    return parts[parts.length - 1] ?? projectPath;
  }, [projectPath]);

  async function handleOpenProject() {
    try {
      setErrorMessage("");
      setIsLoadingProject(true);

      const selectedPath = await open({
        directory: true,
        multiple: false,
      });

      if (!selectedPath || Array.isArray(selectedPath)) {
        return;
      }

      const projectFiles = await invoke<FileItem[]>("list_project_files", {
        rootPath: selectedPath,
      });

      setProjectPath(selectedPath);
      setFiles(projectFiles);
      setSelectedFile(null);
      setFileContent("");

      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content: `Projeto aberto:
${selectedPath}

Encontrei ${projectFiles.length} itens.
Clique em um arquivo para visualizar e analisar com as regras locais.`,
        },
      ]);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setIsLoadingProject(false);
    }
  }

  async function handleSelectFile(file: FileItem) {
    if (file.kind === "folder") {
      return;
    }

    try {
      setErrorMessage("");
      setIsLoadingFile(true);
      setSelectedFile(file);
      setFileContent("");

      const content = await invoke<string>("read_project_file", {
        filePath: file.path,
      });

      setFileContent(content);
    } catch (error) {
      setFileContent("");
      setErrorMessage(String(error));
    } finally {
      setIsLoadingFile(false);
    }
  }

  async function handleSendMessage() {
    const text = chatText.trim();

    if (!text || isAnalyzing) {
      return;
    }

    setMessages((currentMessages) => [
      ...currentMessages,
      {
        role: "user",
        content: text,
      },
    ]);

    setChatText("");

    if (!selectedFile) {
      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content:
            "Você ainda não selecionou nenhum arquivo. Abra um projeto e clique em um arquivo para eu analisar. O motor local precisa de um alvo, infelizmente.",
        },
      ]);

      return;
    }

    if (!projectPath) {
      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content:
            "Nenhum projeto está aberto. Abra uma pasta antes de pedir análise.",
        },
      ]);

      return;
    }

    if (!fileContent) {
      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content: `Arquivo selecionado:
${selectedFile.relative_path}

Esse arquivo não tem conteúdo de texto carregado no momento. Pode ser uma imagem, um arquivo binário ou um arquivo com codificação incompatível.`,
        },
      ]);

      return;
    }

    try {
      setErrorMessage("");
      setIsAnalyzing(true);

      const issues = await invoke<AnalysisIssue[]>("analyze_project_file", {
        projectRoot: projectPath,
        filePath: selectedFile.path,
        relativePath: selectedFile.relative_path,
        fileContent,
      });

      const assistantContent = formatAnalysisMessage(
        issues,
        selectedFile,
        fileContent.length,
        text
      );

      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content: assistantContent,
        },
      ]);
    } catch (error) {
      const message = String(error);

      setErrorMessage(message);

      setMessages((currentMessages) => [
        ...currentMessages,
        {
          role: "assistant",
          content: `Erro ao executar a análise local:

${message}`,
        },
      ]);
    } finally {
      setIsAnalyzing(false);
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-header">
          <span className="app-logo">RC</span>

          <div>
            <h1>Raí Code</h1>
            <p>Assistente desktop local</p>
          </div>
        </div>

        <button
          className="open-project-button"
          type="button"
          onClick={handleOpenProject}
          disabled={isLoadingProject}
        >
          {isLoadingProject ? "Abrindo..." : "Abrir Projeto"}
        </button>

        <section className="project-info">
          <strong>{projectName}</strong>
          {projectPath && <span>{projectPath}</span>}
        </section>

        {errorMessage && <div className="error-box">{errorMessage}</div>}

        <section className="file-tree">
          <h2>Arquivos</h2>

          {files.length === 0 ? (
            <p className="empty-state">
              Nenhum projeto aberto. Clique em Abrir Projeto.
            </p>
          ) : (
            <ul>
              {files.map((file) => (
                <li key={file.path}>
                  <button
                    className={
                      selectedFile?.path === file.path
                        ? "file-item active"
                        : "file-item"
                    }
                    style={{ paddingLeft: `${10 + file.depth * 14}px` }}
                    type="button"
                    onClick={() => handleSelectFile(file)}
                  >
                    <span>{file.kind === "folder" ? "📁" : "📄"}</span>
                    <span>{file.name}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </aside>

      <section className="editor-panel">
        <header className="panel-header">
          <div>
            <h2>{selectedFile?.name ?? "Nenhum arquivo selecionado"}</h2>
            <p>
              {selectedFile?.relative_path ??
                "Abra um projeto e selecione um arquivo"}
            </p>
          </div>

          <button className="secondary-button" type="button" disabled>
            Mostrar diff
          </button>
        </header>

        <pre className="code-view">
          <code>
            {isLoadingFile
              ? "Carregando arquivo..."
              : fileContent ||
                "O conteúdo do arquivo selecionado aparecerá aqui."}
          </code>
        </pre>
      </section>

      <aside className="chat-panel">
        <header className="panel-header">
          <div>
            <h2>Chat</h2>
            <p>Análise local por regras</p>
          </div>
        </header>

        <section className="messages">
          {messages.map((message, index) => (
            <div
              key={`${message.role}-${index}`}
              className={
                message.role === "user"
                  ? "message user-message"
                  : "message assistant-message"
              }
            >
              <strong>{message.role === "user" ? "Você" : "Raí Code"}</strong>

              <p style={{ whiteSpace: "pre-wrap" }}>{message.content}</p>
            </div>
          ))}
        </section>

        <footer className="chat-input-area">
          <textarea
            value={chatText}
            onChange={(event) => setChatText(event.target.value)}
            placeholder="Ex: analise esse arquivo, verifique erro, explique esse código..."
            disabled={isAnalyzing}
          />

          <button
            type="button"
            onClick={handleSendMessage}
            disabled={isAnalyzing}
          >
            {isAnalyzing ? "Analisando..." : "Enviar"}
          </button>
        </footer>
      </aside>
    </main>
  );
}

export default App;