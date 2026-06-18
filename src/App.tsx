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
};

function App() {
  const [projectPath, setProjectPath] = useState("");
  const [files, setFiles] = useState<FileItem[]>([]);
  const [selectedFile, setSelectedFile] = useState<FileItem | null>(null);
  const [fileContent, setFileContent] = useState("");
  const [chatText, setChatText] = useState("");
  const [isLoadingProject, setIsLoadingProject] = useState(false);
  const [isLoadingFile, setIsLoadingFile] = useState(false);
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
Clique em um arquivo para visualizar.`,
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

    if (!text) {
      return;
    }

    let assistantContent = "";

    if (!selectedFile) {
      assistantContent =
        "Você ainda não selecionou nenhum arquivo. Abra um projeto e clique em um arquivo para eu analisar. Porque aparentemente até uma IA precisa que alguém aponte para o objeto antes de comentar sobre ele.";
    } else if (!fileContent) {
      assistantContent = `Arquivo selecionado: ${selectedFile.relative_path}

Esse arquivo não tem conteúdo de texto carregado no momento. Pode ser uma imagem, um arquivo binário ou um arquivo que não foi lido como texto.`;
    } else {
      const previewLimit = 1200;
      const preview =
        fileContent.length > previewLimit
          ? `${fileContent.slice(
              0,
              previewLimit
            )}\n\n...conteúdo cortado para visualização inicial...`
          : fileContent;

      let analysisResult = "";

      try {
        const issues = await invoke<AnalysisIssue[]>("analyze_project_file", {
          filePath: selectedFile.path,
          relativePath: selectedFile.relative_path,
          fileContent,
        });

        if (issues.length === 0) {
          analysisResult =
            "Análise local: nenhum problema conhecido foi encontrado pelas regras atuais.";
        } else {
          analysisResult = issues
            .map(
              (issue, index) => `${index + 1}. ${issue.title}
Severidade: ${issue.severity}
Descrição: ${issue.description}
Sugestão: ${issue.suggestion}
Regra consultada: ${issue.matched_rule}`
            )
            .join("\n\n");
        }
      } catch (error) {
        analysisResult = `Não consegui executar a análise local: ${String(
          error
        )}`;
      }

      assistantContent = `Arquivo atual: ${selectedFile.relative_path}

Tamanho do conteúdo: ${fileContent.length} caracteres.

Pedido recebido:
${text}

Resultado da análise local:

${analysisResult}

Prévia do arquivo:

${preview}

O Raí Code agora já consegue consultar o primeiro motor local de regras. Ainda não aplica alterações automaticamente. Isso é bom, porque programa mexendo em pasta sem confirmação é basicamente um guaxinim com acesso de administrador.`;
    }

    setMessages((currentMessages) => [
      ...currentMessages,
      {
        role: "user",
        content: text,
      },
      {
        role: "assistant",
        content: assistantContent,
      },
    ]);

    setChatText("");
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-header">
          <span className="app-logo">RC</span>

          <div>
            <h1>Raí Code</h1>
            <p>Assistente desktop para dev</p>
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
            <p>Peça análise, correção ou explicação</p>
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
            placeholder="Ex: explique esse arquivo, corrija o erro, crie uma função..."
          />

          <button type="button" onClick={handleSendMessage}>
            Enviar
          </button>
        </footer>
      </aside>
    </main>
  );
}

export default App;