//! Platform SSH Agent adapters used for authentication and interactive forwarding.

use std::time::Duration;

use russh::keys::agent::client::{AgentClient, AgentStream};

use crate::SshAgentEndpoint;

pub(super) type DynamicAgentStream = Box<dyn AgentStream + Send + Unpin + 'static>;
pub(super) type DynamicAgentClient = AgentClient<DynamicAgentStream>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(windows)]
const WINDOWS_OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

pub(crate) async fn connect_agent_client(
    endpoint: &SshAgentEndpoint,
) -> anyhow::Result<DynamicAgentClient> {
    Ok(AgentClient::connect(connect_agent_stream(endpoint).await?))
}

pub(crate) async fn connect_agent_stream(
    endpoint: &SshAgentEndpoint,
) -> anyhow::Result<DynamicAgentStream> {
    match endpoint {
        SshAgentEndpoint::Auto => connect_auto().await,
        SshAgentEndpoint::Environment { variable } => {
            #[cfg(unix)]
            {
                let variable = variable.trim().trim_start_matches('$');
                anyhow::ensure!(
                    !variable.is_empty(),
                    "SSH Agent environment variable is empty"
                );
                let path = std::env::var_os(variable)
                    .ok_or_else(|| anyhow::anyhow!("SSH Agent environment variable is not set"))?;
                connect_unix(std::path::Path::new(&path)).await
            }
            #[cfg(not(unix))]
            {
                let _ = variable;
                anyhow::bail!("environment SSH Agent endpoints require Unix")
            }
        }
        SshAgentEndpoint::UnixSocket { path } => {
            #[cfg(unix)]
            {
                anyhow::ensure!(!path.trim().is_empty(), "SSH Agent socket path is empty");
                connect_unix(std::path::Path::new(path)).await
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                anyhow::bail!("Unix SSH Agent sockets require Unix")
            }
        }
        SshAgentEndpoint::Pageant => connect_pageant().await,
        SshAgentEndpoint::WindowsOpenSsh => connect_windows_openssh().await,
    }
}

#[cfg(unix)]
async fn connect_auto() -> anyhow::Result<DynamicAgentStream> {
    let path = std::env::var_os("SSH_AUTH_SOCK")
        .ok_or_else(|| anyhow::anyhow!("SSH_AUTH_SOCK is not set"))?;
    connect_unix(std::path::Path::new(&path)).await
}

#[cfg(windows)]
async fn connect_auto() -> anyhow::Result<DynamicAgentStream> {
    match connect_windows_openssh().await {
        Ok(stream) => Ok(stream),
        Err(_) => connect_pageant().await,
    }
}

#[cfg(not(any(unix, windows)))]
async fn connect_auto() -> anyhow::Result<DynamicAgentStream> {
    anyhow::bail!("SSH Agent is unsupported on this platform")
}

#[cfg(unix)]
async fn connect_unix(path: &std::path::Path) -> anyhow::Result<DynamicAgentStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::UnixStream::connect(path))
        .await
        .map_err(|_| anyhow::anyhow!("SSH Agent connection timed out"))??;
    Ok(Box::new(stream))
}

#[cfg(windows)]
async fn connect_windows_openssh() -> anyhow::Result<DynamicAgentStream> {
    use std::ffi::OsStr;
    let client = tokio::time::timeout(
        CONNECT_TIMEOUT,
        AgentClient::connect_named_pipe(OsStr::new(WINDOWS_OPENSSH_AGENT_PIPE)),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Windows OpenSSH Agent connection timed out"))??;
    Ok(Box::new(client.into_inner()))
}

#[cfg(not(windows))]
async fn connect_windows_openssh() -> anyhow::Result<DynamicAgentStream> {
    anyhow::bail!("Windows OpenSSH Agent is available only on Windows")
}

#[cfg(windows)]
async fn connect_pageant() -> anyhow::Result<DynamicAgentStream> {
    let client = tokio::time::timeout(CONNECT_TIMEOUT, AgentClient::connect_pageant())
        .await
        .map_err(|_| anyhow::anyhow!("Pageant connection timed out"))??;
    Ok(Box::new(client.into_inner()))
}

#[cfg(not(windows))]
async fn connect_pageant() -> anyhow::Result<DynamicAgentStream> {
    anyhow::bail!("Pageant is available only on Windows")
}
