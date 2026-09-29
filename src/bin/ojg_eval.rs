use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use ojg_core::backend::openjev::OpenJevBackend;
use ojg_core::config::BackendConfig;
use ojg_core::eval::{calculate_metrics, EvalTask};
use ojg_core::search::{search, BackendMetadata, SearchOptions, SearchRequest};

#[derive(Debug, Parser)]
#[command(
    name = "ojg-eval",
    about = "Measure retrieval on annotated repository questions"
)]
struct Arguments {
    #[arg(long, default_value = "fixtures/sample-repo")]
    root: PathBuf,
    #[arg(long, default_value = "eval/questions.json")]
    questions: PathBuf,
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    endpoint: String,
    #[arg(long, default_value = "verdict-1.4")]
    model: String,
}

fn main() -> ExitCode {
    match tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(run())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ojg-eval: {error}");
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let tasks: Vec<EvalTask> =
        serde_json::from_str(&std::fs::read_to_string(arguments.questions)?)?;
    let backend = std::sync::Arc::new(OpenJevBackend::new(
        BackendConfig {
            endpoint: arguments.endpoint.clone(),
            model: arguments.model.clone(),
            timeout_seconds: 30,
            concurrency: 4,
        },
        reqwest::Client::new(),
    )?);
    let mut rankings = Vec::with_capacity(tasks.len());
    for task in &tasks {
        let response = search(
            SearchRequest {
                query: task.query.clone(),
                root: arguments.root.clone(),
                scopes: Vec::new(),
                backend: BackendMetadata {
                    endpoint: arguments.endpoint.clone(),
                    model: arguments.model.clone(),
                },
                options: SearchOptions::default(),
            },
            backend.clone(),
        )
        .await?;
        rankings.push(
            response
                .results
                .into_iter()
                .map(|result| result.path)
                .collect(),
        );
    }
    let metrics = calculate_metrics(&tasks, &rankings);
    println!("Recall@1: {:.3}", metrics.recall_at_1);
    println!("Recall@5: {:.3}", metrics.recall_at_5);
    println!("Recall@10: {:.3}", metrics.recall_at_10);
    println!("MRR: {:.3}", metrics.mrr);
    Ok(())
}
