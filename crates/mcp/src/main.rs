//! `Celesta-mcp`: an MCP server on stdio that lets coding agents inspect,
//! render, and export Celesta videos and read the Celesta docs.

mod args;
mod docs;
mod export;
mod frames;
mod inspect;
mod server;
mod summary;

use rmcp::ServiceExt;

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Celesta MCP server: {error}");
            1
        }
    };
    // Exit here: shutting the runtime down would wait for the blocking
    // stdin read, which never ends while the client keeps stdin open.
    std::process::exit(code)
}

async fn run() -> anyhow::Result<()> {
    let service = server::CelestaServer::new()?
        .serve(rmcp::transport::stdio())
        .await?;
    let stop = service.cancellation_token();
    tokio::spawn(async move {
        terminated().await;
        stop.cancel();
    });
    // Returning drops the server, which removes its temporary outputs.
    service.waiting().await?;
    Ok(())
}

/// Resolves on Ctrl-C, or SIGTERM where there is one, which is how MCP
/// clients usually stop a server.
async fn terminated() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
