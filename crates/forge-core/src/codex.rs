use crate::{
    contract::{AccountStatus, Login},
    error::{ApiError, Result},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, oneshot},
};

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
pub struct Codex {
    writer: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: Pending,
    pub notifications: broadcast::Sender<Value>,
    sequence: AtomicU64,
    alive: Arc<AtomicBool>,
    guide_threads: Arc<Mutex<std::collections::HashSet<String>>>,
    _child: Child,
}

pub fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("ASSET_FORGE_CODEX") {
        return path.into();
    }
    if let Some(home) = dirs::home_dir() {
        for path in [
            home.join(".local/bin/codex"),
            home.join(".npm-global/bin/codex"),
            home.join("AppData/Roaming/npm/codex.cmd"),
        ] {
            if path.is_file() && path.extension().is_none_or(|x| x != "cmd") {
                return path;
            }
        }
        // npm installs a .cmd launcher on Windows; use its native Codex binary.
        #[cfg(windows)]
        for prefix in [
            home.join("AppData/Roaming/npm"),
            home.join("AppData/Local/npm"),
        ] {
            for arch in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
                for package in [
                    "@openai/codex",
                    "@openai/codex/node_modules/@openai/codex-win32-x64",
                    "@openai/codex/node_modules/@openai/codex-win32-arm64",
                ] {
                    let path = prefix
                        .join("node_modules")
                        .join(package)
                        .join("vendor")
                        .join(arch)
                        .join("codex/codex.exe");
                    if path.is_file() {
                        return path;
                    }
                }
            }
        }
    }
    for path in ["/opt/homebrew/bin/codex", "/usr/local/bin/codex"] {
        if Path::new(path).is_file() {
            return path.into();
        }
    }
    "codex".into()
}

impl Codex {
    pub async fn start(cwd: &Path) -> Result<Arc<Self>> {
        let binary = executable();
        // Process-local overrides keep the art director inside the app's tool boundary.
        // Inspect metadata through Codex, never read or rewrite its credential/config files.
        let mut inventory = Command::new(&binary);
        inventory
            .args(["mcp", "list", "--json"])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        inventory.creation_flags(0x08000000);
        let listed = tokio::time::timeout(Duration::from_secs(10), inventory.output())
            .await
            .map_err(|_| {
                ApiError::new("CODEX_CONFIG_ERROR", "Codex tool configuration timed out.")
            })?
            .map_err(|_| {
                ApiError::new(
                    "CODEX_NOT_FOUND",
                    "Install Codex CLI, or set ASSET_FORGE_CODEX to its native executable.",
                )
            })?;
        if !listed.status.success() {
            return Err(ApiError::new(
                "CODEX_CONFIG_ERROR",
                "Could not inspect Codex's configured tools. Update the Codex CLI and try again.",
            ));
        }
        let servers: Vec<Value> = serde_json::from_slice(&listed.stdout).map_err(|_| {
            ApiError::new(
                "CODEX_CONFIG_ERROR",
                "Codex returned invalid tool configuration metadata.",
            )
        })?;
        let mut command = Command::new(binary);
        let mut disabled = Vec::new();
        for server in servers {
            let name = server["name"].as_str().ok_or_else(|| {
                ApiError::new(
                    "CODEX_CONFIG_ERROR",
                    "Codex returned a tool configuration without a name.",
                )
            })?;
            let quoted = serde_json::to_string(name).unwrap();
            disabled.push(format!("{quoted}={{enabled=false}}"));
        }
        command.args(["-c", &format!("mcp_servers={{{}}}", disabled.join(","))]);
        command
            .args([
                "app-server",
                "--listen",
                "stdio://",
                "-c",
                "features.image_generation=true",
                "-c",
                "features.shell_tool=false",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "features.multi_agent=false",
            ])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|_| {
            ApiError::new(
                "CODEX_NOT_FOUND",
                "Install the Codex CLI, or set ASSET_FORGE_CODEX to its executable path.",
            )
        })?;
        let writer = Arc::new(tokio::sync::Mutex::new(child.stdin.take().unwrap()));
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let pending: Pending = Default::default();
        let alive = Arc::new(AtomicBool::new(true));
        let read_alive = alive.clone();
        let (notifications, _) = broadcast::channel(64);
        let read_pending = pending.clone();
        let read_notifications = notifications.clone();
        let read_writer = writer.clone();
        let guide_threads: Arc<Mutex<std::collections::HashSet<String>>> = Default::default();
        let read_guide_threads = guide_threads.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let read = read_frame(&mut reader, &mut line).await;
                if !matches!(read, Ok(true)) {
                    break;
                }
                let message: Value = match serde_json::from_slice(&line) {
                    Ok(x) => x,
                    Err(_) => break,
                };
                if !message.is_object() {
                    break;
                }
                if message.get("method").is_some() {
                    if message.get("id").is_some() {
                        // Image-only sessions never grant unrelated tools or permissions.
                        let method = message["method"].as_str().unwrap_or("");
                        if method == "item/tool/call"
                            && message["params"]["threadId"]
                                .as_str()
                                .is_some_and(|id| read_guide_threads.lock().unwrap().contains(id))
                        {
                            let _ = read_notifications.send(message);
                            continue;
                        }
                        let response = match method {
                            "item/commandExecution/requestApproval"
                            | "item/fileChange/requestApproval" => {
                                json!({"id":message["id"],"result":{"decision":"decline"}})
                            }
                            "item/permissions/requestApproval" => {
                                json!({"id":message["id"],"result":{"permissions":{},"scope":"turn"}})
                            }
                            _ => {
                                json!({"id":message["id"],"error":{"code":-32601,"message":"Asset Forge does not grant this tool."}})
                            }
                        };
                        let _ = send(&read_writer, &response).await;
                    } else {
                        let _ = read_notifications.send(message);
                    }
                } else if let Some(id) = message["id"].as_u64() {
                    let sender = read_pending.lock().unwrap().remove(&id);
                    if let Some(sender) = sender {
                        let result = if message.get("error").is_some() {
                            Err(ApiError::new(
                                "CODEX_ERROR",
                                message["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Codex rejected the request."),
                            ))
                        } else if let Some(result) = message.get("result") {
                            Ok(result.clone())
                        } else {
                            Err(ApiError::new(
                                "PROTOCOL_ERROR",
                                "Codex returned an invalid response.",
                            ))
                        };
                        let _ = sender.send(result);
                    }
                }
            }
            read_alive.store(false, Ordering::Release);
            for (_, sender) in read_pending.lock().unwrap().drain() {
                let _ = sender.send(Err(ApiError::new(
                    "CODEX_DISCONNECTED",
                    "Codex stopped unexpectedly. The generation outcome is unknown.",
                )));
            }
            let _ = read_notifications.send(json!({"method":"forge/disconnected","params":{}}));
        });
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.contains("token") && !line.contains("Authorization") {
                    eprintln!("Codex: {}", line.chars().take(500).collect::<String>());
                }
            }
        });
        let client = Arc::new(Self {
            writer,
            pending,
            notifications,
            sequence: AtomicU64::new(1),
            alive,
            guide_threads,
            _child: child,
        });
        client.request("initialize",json!({"clientInfo":{"name":"asset_forge","title":"Asset Forge","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        send(&client.writer, &json!({"method":"initialized"})).await?;
        Ok(client)
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        if !self.is_alive() {
            return Err(ApiError::new("CODEX_DISCONNECTED", "Codex disconnected."));
        }
        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(error) = send(
            &self.writer,
            &json!({"id":id,"method":method,"params":params}),
        )
        .await
        {
            self.pending.lock().unwrap().remove(&id);
            return Err(error);
        }
        let result = tokio::time::timeout(Duration::from_secs(60), rx).await;
        self.pending.lock().unwrap().remove(&id);
        match result {
            Ok(Ok(value)) => value,
            Ok(Err(_)) => Err(ApiError::new("CODEX_DISCONNECTED", "Codex disconnected.")),
            Err(_) => Err(ApiError::new(
                "CODEX_TIMEOUT",
                "Codex did not answer in time. The request outcome may be unknown.",
            )),
        }
    }
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub fn register_guide(&self, id: &str) {
        self.guide_threads.lock().unwrap().insert(id.into());
    }
    pub fn unregister_guide(&self, id: &str) {
        self.guide_threads.lock().unwrap().remove(id);
    }
    pub async fn tool_reply(&self, id: Value, result: Result<Value>) -> Result<()> {
        let (success, text) = match result {
            Ok(v) => (true, serde_json::to_string(&v).unwrap()),
            Err(e) => (
                false,
                serde_json::to_string(&serde_json::json!({"error":e})).unwrap(),
            ),
        };
        send(&self.writer,&json!({"id":id,"result":{"success":success,"contentItems":[{"type":"inputText","text":text}]}})).await
    }

    pub async fn account(&self) -> Result<AccountStatus> {
        let result = self
            .request("account/read", json!({"refreshToken":false}))
            .await?;
        if !result["requiresOpenaiAuth"].is_boolean() {
            return Err(ApiError::new(
                "PROTOCOL_ERROR",
                "Codex returned invalid account information.",
            ));
        }
        let account = &result["account"];
        let logged_in = account["type"].as_str() == Some("chatgpt");
        let capabilities = self
            .request("modelProvider/capabilities/read", json!({}))
            .await;
        let can_generate = capabilities
            .as_ref()
            .ok()
            .and_then(|x| x["imageGeneration"].as_bool())
            .unwrap_or(false);
        let message = if !logged_in {
            Some("Connect your ChatGPT account to use your Codex subscription.".into())
        } else if !can_generate {
            Some("Image generation is unavailable from this Codex provider. Update Codex or check your account access.".into())
        } else {
            None
        };
        Ok(AccountStatus {
            is_logged_in: logged_in,
            email: account["email"].as_str().map(String::from),
            plan: account["planType"].as_str().map(String::from),
            can_generate_images: logged_in && can_generate,
            message,
        })
    }

    pub async fn login(&self) -> Result<Login> {
        let r = self
            .request("account/login/start", json!({"type":"chatgpt"}))
            .await?;
        let login_id = r["loginId"]
            .as_str()
            .ok_or_else(|| {
                ApiError::new("PROTOCOL_ERROR", "Codex did not return a login identifier.")
            })?
            .to_string();
        let auth_url = r["authUrl"]
            .as_str()
            .filter(|s| {
                s.starts_with("https://auth.openai.com/") || s.starts_with("https://chatgpt.com/")
            })
            .ok_or_else(|| {
                ApiError::new(
                    "PROTOCOL_ERROR",
                    "Codex returned an unexpected sign-in URL.",
                )
            })?
            .to_string();
        Ok(Login { login_id, auth_url })
    }
}

async fn send(writer: &tokio::sync::Mutex<ChildStdin>, message: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(message)
        .map_err(|_| ApiError::new("PROTOCOL_ERROR", "Could not encode a Codex message."))?;
    bytes.push(b'\n');
    let mut writer = writer.lock().await;
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| ApiError::new("CODEX_DISCONNECTED", "Could not send a request to Codex."))?;
    writer
        .flush()
        .await
        .map_err(|_| ApiError::new("CODEX_DISCONNECTED", "Could not flush a request to Codex."))
}

async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<bool> {
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(false);
        }
        let end = chunk.iter().position(|b| *b == b'\n');
        let n = end.map(|x| x + 1).unwrap_or(chunk.len());
        if line.len() + n > 128 * 1024 * 1024 {
            return Err(std::io::Error::other("Codex frame exceeded limit"));
        }
        line.extend_from_slice(&chunk[..n]);
        reader.consume(n);
        if end.is_some() {
            return Ok(true);
        }
    }
}
