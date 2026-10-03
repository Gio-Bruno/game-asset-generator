use clap::{Parser, Subcommand};
use forge_core::{
    Service,
    contract::{AssistantSession, AssistantStatus, Job, JobStatus},
    default_data_dir,
    error::{ApiError, Result},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

#[derive(Parser)]
#[command(
    name = "asset-forge",
    version,
    about = "A local 2D game asset workshop powered by your Codex subscription"
)]
struct Cli {
    #[arg(long, global = true, env = "ASSET_FORGE_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Internal restart/install helper; runs outside the application folder.
    #[command(hide = true)]
    ApplyUpdate {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long, hide = true)]
        no_relaunch: bool,
    },
    /// Start the newline-delimited JSON API on stdin/stdout.
    Serve,
    /// Call any v1 API method with a JSON params object.
    Call {
        method: String,
        #[arg(default_value = "{}")]
        params: String,
    },
    /// Verify Codex authentication and native image generation availability.
    Doctor,
    /// Connect your ChatGPT account in the browser and wait for completion.
    Login {
        #[arg(long)]
        no_browser: bool,
    },
    /// Generate an asset from a JSON request file; defaults to waiting for the image.
    Generate {
        #[arg(long)]
        request: PathBuf,
    },
    /// Generate a sprite animation from a JSON request file, waiting for extracted frames.
    Animate {
        #[arg(long)]
        request: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = run(cli).await;
    if let Err(error) = result {
        println!("{}", json!({"error":error}));
        std::process::exit(1);
    }
}
async fn run(cli: Cli) -> Result<()> {
    // No workspace lease, Codex startup or data access in the detached installer.
    if let Commands::ApplyUpdate { plan, no_relaunch } = &cli.command {
        return forge_core::updates::apply_update(plan, !no_relaunch);
    }
    let root = cli.data_dir.unwrap_or_else(default_data_dir);
    let service = Service::open(root)?;
    match cli.command {
        Commands::ApplyUpdate { .. } => unreachable!("Handled before workspace startup"),
        Commands::Serve => serve(service).await,
        Commands::Call { method, params } => {
            let params =
                serde_json::from_str(&params).map_err(|e| ApiError::validation(e.to_string()))?;
            // `call jobs/create` also waits, because a one-shot process owns its worker.
            let value = service.dispatch(&method, params).await?;
            if method == "animations/create" {
                let id = value["id"].as_str().unwrap();
                wait_job(&service, id).await?;
                println!(
                    "{}",
                    json!({"result":service.dispatch("animations/get",json!({"id":id})).await?})
                );
            } else if method == "jobs/batch/create" || method == "animations/sets/create" {
                let ids: Vec<String> =
                    serde_json::from_value(value["jobIds"].clone()).map_err(ApiError::storage)?;
                let jobs = wait_jobs(&service, &ids).await?;
                println!("{}", json!({"result":{"batch":value,"jobs":jobs}}));
            } else if method == "jobs/create" {
                let job: Job = serde_json::from_value(value).map_err(ApiError::storage)?;
                println!("{}", json!({"result":wait_job(&service,&job.id).await?}));
            } else if method == "assistant/message" {
                let session: AssistantSession =
                    serde_json::from_value(value).map_err(ApiError::storage)?;
                println!(
                    "{}",
                    json!({"result":wait_guide(&service,&session.id).await?})
                );
            } else {
                println!("{}", json!({"result":value}));
            }
            Ok(())
        }
        Commands::Doctor => {
            let info = service.dispatch("system/info", json!({})).await?;
            let account = service.dispatch("account/read", json!({})).await?;
            println!("{}", json!({"result":{"system":info,"account":account}}));
            Ok(())
        }
        Commands::Login { no_browser } => {
            let mut events = service.subscribe();
            let login = service.dispatch("account/login/start", json!({})).await?;
            println!("{}", json!({"result":login}));
            if !no_browser && let Some(url) = login["authUrl"].as_str() {
                let _ = open::that(url);
            }
            loop {
                let event = tokio::time::timeout(Duration::from_secs(600), events.recv())
                    .await
                    .map_err(|_| {
                        ApiError::new(
                            "LOGIN_TIMEOUT",
                            "Sign-in timed out. Run asset-forge login to try again.",
                        )
                    })?
                    .map_err(|_| ApiError::new("LOGIN_FAILED", "Sign-in progress was lost."))?;
                if event.kind == "ACCOUNT_CONNECTED" {
                    println!(
                        "{}",
                        json!({"result":service.dispatch("account/read",json!({})).await?})
                    );
                    return Ok(());
                }
                if event.kind == "LOGIN_FAILED" {
                    return Err(ApiError::new("LOGIN_FAILED", event.message));
                }
            }
        }
        Commands::Generate { request } => {
            let bytes = tokio::fs::read(request)
                .await
                .map_err(|_| ApiError::validation("Could not open the request JSON file."))?;
            let params: Value =
                serde_json::from_slice(&bytes).map_err(|e| ApiError::validation(e.to_string()))?;
            let job = service.dispatch("jobs/create", params).await?;
            let id = job["id"].as_str().unwrap();
            let result = wait_job(&service, id).await?;
            println!("{}", json!({"result":result}));
            Ok(())
        }
        Commands::Animate { request } => {
            let bytes = tokio::fs::read(request).await.map_err(|_| {
                ApiError::validation("Could not open the animation request JSON file.")
            })?;
            let params: Value =
                serde_json::from_slice(&bytes).map_err(|e| ApiError::validation(e.to_string()))?;
            let clip = service.dispatch("animations/create", params).await?;
            let id = clip["id"].as_str().unwrap();
            wait_job(&service, id).await?;
            println!(
                "{}",
                json!({"result":service.dispatch("animations/get",json!({"id":id})).await?})
            );
            Ok(())
        }
    }
}
async fn wait_job(service: &Service, id: &str) -> Result<Value> {
    loop {
        let value = service.dispatch("jobs/get", json!({"id":id})).await?;
        let job: Job = serde_json::from_value(value.clone()).map_err(ApiError::storage)?;
        if job.status.is_terminal() {
            if job.status != JobStatus::Succeeded {
                return Err(job.error.unwrap_or_else(|| {
                    ApiError::new("GENERATION_FAILED", "Generation did not succeed.")
                }));
            }
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_guide(service: &Service, id: &str) -> Result<Value> {
    loop {
        let value = service.dispatch("assistant/get", json!({"id":id})).await?;
        let session: AssistantSession =
            serde_json::from_value(value.clone()).map_err(ApiError::storage)?;
        if session.status != AssistantStatus::Thinking {
            if let Some(error) = session.error {
                return Err(error);
            }
            if session.turn_job_count > 0 {
                let start = session
                    .generated_job_ids
                    .len()
                    .saturating_sub(session.turn_job_count as usize);
                let jobs = wait_jobs(service, &session.generated_job_ids[start..]).await?;
                if let Some(job) = jobs.into_iter().find(|j| j.status != JobStatus::Succeeded) {
                    return Err(job.error.unwrap_or_else(|| {
                        ApiError::new(
                            "GENERATION_FAILED",
                            "An asset in the batch did not succeed.",
                        )
                    }));
                }
            }
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}
async fn wait_jobs(service: &Service, ids: &[String]) -> Result<Vec<Job>> {
    loop {
        let mut jobs = Vec::with_capacity(ids.len());
        for id in ids {
            let job = service.dispatch("jobs/get", json!({"id":id})).await?;
            jobs.push(serde_json::from_value::<Job>(job).map_err(ApiError::storage)?);
        }
        if jobs.iter().all(|j| j.status.is_terminal()) {
            return Ok(jobs);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: Value,
    method: String,
    #[serde(default = "empty")]
    params: Value,
}
fn empty() -> Value {
    json!({})
}
async fn write(stdout: &Mutex<tokio::io::Stdout>, value: Value) {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    let mut stdout = stdout.lock().await;
    let _ = stdout.write_all(&bytes).await;
    let _ = stdout.flush().await;
}
async fn serve(service: Service) -> Result<()> {
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut events = service.subscribe();
    let event_out = stdout.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await{Ok(event)=>write(&event_out,json!({"method":"events/notification","params":event})).await,Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>write(&event_out,json!({"method":"events/notification","params":{"kind":"RESYNC_REQUIRED","message":"Refresh jobs and assets."}})).await,Err(_)=>break}
        }
    });
    let mut reader = BufReader::new(tokio::io::stdin());
    loop {
        let mut bytes = Vec::new();
        let read = (&mut reader)
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .await
            .map_err(ApiError::storage)?;
        if read == 0 {
            return Ok(());
        }
        if bytes.len() > 1024 * 1024 {
            return Err(ApiError::validation(
                "API requests must be smaller than 1 MB.",
            ));
        }
        let request: Request = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                write(
                    &stdout,
                    json!({"id":null,"error":ApiError::validation(e.to_string())}),
                )
                .await;
                continue;
            }
        };
        if !(request.id.is_string() || request.id.is_number()) {
            write(
                &stdout,
                json!({"id":null,"error":ApiError::validation("id must be a string or number.")}),
            )
            .await;
            continue;
        }
        let response = match service.dispatch(&request.method, request.params).await {
            Ok(result) => json!({"id":request.id,"result":result}),
            Err(error) => json!({"id":request.id,"error":error}),
        };
        write(&stdout, response).await;
    }
}
