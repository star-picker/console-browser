//! 传输层：Windows 命名管道 / Unix 域套接字，换行分隔 JSON。
//!
//! 一个连接一次请求-响应；daemon 内多连接共享会话表（用 Mutex 串行化）。

use crate::{Daemon, Request, Response};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

#[cfg(windows)]
pub fn endpoint(name: &str) -> String {
    let name = name.trim();
    if name.starts_with("\\\\.\\pipe\\") {
        name.to_string()
    } else {
        format!("\\\\.\\pipe\\{name}")
    }
}

/// 人能读的端点名字（CLI 打印用）。
#[cfg(windows)]
pub fn endpoint_label(name: &str) -> String {
    endpoint(name)
}

#[cfg(unix)]
pub fn endpoint(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cb-{name}.sock"))
}

#[cfg(unix)]
pub fn endpoint_label(name: &str) -> String {
    endpoint(name).display().to_string()
}

#[cfg(windows)]
pub async fn serve(name: &str, daemon: Arc<Mutex<Daemon>>) -> anyhow::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let path = endpoint(name);
    let mut first = true;
    loop {
        // 同一个 pipe 名可以有多个实例；第一个实例负责「已在运行」检测。
        let server = ServerOptions::new()
            .first_pipe_instance(first)
            .create(&path)?;
        first = false;
        server.connect().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_conn(server, daemon).await {
                tracing::debug!(%err, "session pipe connection ended");
            }
        });
    }
}

#[cfg(unix)]
pub async fn serve(name: &str, daemon: Arc<Mutex<Daemon>>) -> anyhow::Result<()> {
    use tokio::net::UnixListener;
    let path = endpoint(name);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    loop {
        let (stream, _) = listener.accept().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_conn(stream, daemon).await {
                tracing::debug!(%err, "session socket connection ended");
            }
        });
    }
}

async fn serve_conn<S>(stream: S, daemon: Arc<Mutex<Daemon>>) -> anyhow::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(line) {
            Ok(req) => {
                let mut daemon = daemon.lock().await;
                daemon.call(req).await
            }
            Err(err) => Response::usage(0, format!("bad request json: {err}")),
        };
        let mut payload = serde_json::to_string(&response)?;
        payload.push('\n');
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;
    }
    Ok(())
}

/// 单次请求：连上 daemon、发一行、读一行。
pub async fn call(name: &str, req: Request) -> anyhow::Result<Response> {
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        let stream = ClientOptions::new().open(endpoint(name))?;
        call_io(stream, req).await
    }
    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(endpoint(name)).await?;
        call_io(stream, req).await
    }
}

async fn call_io<S>(stream: S, req: Request) -> anyhow::Result<Response>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut payload = serde_json::to_string(&req)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    let mut lines = BufReader::new(reader).lines();
    let line = lines
        .next_line()
        .await?
        .ok_or_else(|| anyhow::anyhow!("daemon closed the connection without a response"))?;
    let response: Response = serde_json::from_str(&line)?;
    Ok(response)
}
