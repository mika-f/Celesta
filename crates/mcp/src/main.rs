//! `Celesta-mcp`: an MCP server on stdio that lets coding agents inspect,
//! render, and export Celesta videos and read the Celesta docs.

mod args;
mod comments;
mod docs;
mod export;
mod frames;
mod inspect;
mod server;
mod summary;

use rmcp::ServiceExt;

fn main() {
    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next()) {
        (Some(flag), None) if flag == "-h" || flag == "--help" => {
            println!(
                "celesta-mcp — Celesta's Model Context Protocol (MCP) server\n\
                 \n\
                 An MCP client launches this server to inspect, render, and export Celesta\n\
                 compositions. It exchanges MCP messages over stdin/stdout.\n\
                 \n\
                 For command-line rendering, use celesta-exporter (Celesta-export in desktop\n\
                 packages). Run celesta-exporter --help for export options.\n\
                 \n\
                 Options:\n  -h, --help     Print help\n  -V, --version  Print version"
            );
        }
        (Some(flag), None) if flag == "-V" || flag == "--version" => {
            println!("celesta-mcp {}", env!("CARGO_PKG_VERSION"));
        }
        _ => start_server(),
    }
}

#[tokio::main]
async fn start_server() {
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
    // The registry the tools share until one discovers other plugins.
    let media = celesta_exporter::use_installed_media();
    let service = server::CelestaServer::new(media)?
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
