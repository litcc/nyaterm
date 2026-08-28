//! 按需加载并在内存中缓存用户 Shell 环境变量。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SHELL_OUTPUT: usize = 64 * 1024;
const SHELL_ENV_READER_MARKER: &str = "NYATERM_SHELL_ENV_READER";

/// Shell 环境查询返回的变量值。
///
/// 最后一个副本释放时会清零内容；Debug 输出刻意脱敏，避免调用方误记录变量值。
#[derive(Clone)]
pub struct EnvironmentValue(Zeroizing<String>);

impl EnvironmentValue {
    pub(crate) fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// 为短暂的使用操作借用变量值。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for EnvironmentValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 解析指定 Shell 环境变量时产生的错误。
#[derive(Debug, Error)]
pub enum ShellEnvironmentError {
    #[error("invalid environment variable name")]
    InvalidVariableName,
    #[error("failed to start the user shell: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("the user shell did not finish before the timeout")]
    Timeout,
    #[error("the user shell produced too much output")]
    OutputTooLarge,
    #[error("the user shell exited unsuccessfully")]
    ShellExit,
    #[error("failed to read the user shell output: {0}")]
    Read(#[source] std::io::Error),
    #[error("failed to decode the user shell output")]
    OutputEncoding,
    #[error("failed to decode an environment variable value")]
    ValueEncoding,
}

struct CacheState {
    values: HashMap<String, EnvironmentValue>,
    missing: HashSet<String>,
}

/// 只从用户默认 Shell 加载请求且已导出的变量。
///
/// 缓存只存在于运行期间，不会持久化、记录日志或序列化。多个缺失变量由同一个
/// Shell 进程批量加载，避免每个变量都重复支付慢速 Shell 启动成本。
pub struct ShellEnvironmentCache {
    values: RwLock<CacheState>,
    load_lock: Mutex<()>,
    timeout: Duration,
    shell_path: Option<PathBuf>,
    detected_shell: OnceLock<PathBuf>,
}

pub(crate) fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC")
            .ok()
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "powershell.exe".to_string())
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string())
    }
}

impl ShellEnvironmentCache {
    /// 使用用户默认 Shell 和十秒超时创建缓存。
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            values: RwLock::new(CacheState {
                values: HashMap::new(),
                missing: HashSet::new(),
            }),
            load_lock: Mutex::new(()),
            timeout: DEFAULT_TIMEOUT,
            shell_path: None,
            detected_shell: OnceLock::new(),
        })
    }

    /// 返回供传输服务共享的进程级运行时缓存。
    pub fn global() -> Arc<Self> {
        static GLOBAL: OnceLock<Arc<ShellEnvironmentCache>> = OnceLock::new();
        GLOBAL.get_or_init(Self::new).clone()
    }

    #[cfg(test)]
    pub(crate) fn with_shell_path(path: PathBuf) -> Arc<Self> {
        let mut cache = Self::new();
        Arc::get_mut(&mut cache)
            .expect("new shell environment cache is not shared")
            .shell_path = Some(path);
        cache
    }

    /// 只读取缓存值，不启动 Shell。
    pub fn cached(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        let values = self
            .values
            .read()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        Ok(values.values.get(&variable).cloned())
    }

    /// 解析一个变量；缓存缺失时从 Shell 加载。
    pub async fn resolve(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        self.resolve_until(variable, Instant::now() + self.timeout)
            .await
    }

    /// 在指定截止时间前解析变量。
    pub(crate) async fn resolve_until(
        &self,
        variable: &str,
        deadline: Instant,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        if let Some(value) = self.cached(&variable)? {
            return Ok(Some(value));
        }
        if self.is_missing_cached(&variable)? {
            return Ok(None);
        }

        // GUI 进程可能从启动器继承已经失效的 Agent Socket。优先读取登录
        // Shell；只有 Shell 无法提供变量时，才回退到进程继承值。
        self.warm_internal(std::slice::from_ref(&variable), false, deadline)
            .await?;
        self.cached(&variable)
    }

    /// 重新运行用户 Shell，强制刷新一个变量。
    pub async fn refresh(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        self.refresh_until(variable, Instant::now() + self.timeout)
            .await
    }

    /// 在指定截止时间前强制刷新变量。
    pub(crate) async fn refresh_until(
        &self,
        variable: &str,
        deadline: Instant,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        self.warm_internal(std::slice::from_ref(&variable), true, deadline)
            .await?;
        self.cached(&variable)
    }

    /// 加载尚未进入缓存的所有请求变量。
    pub async fn warm(&self, variables: &[String]) -> Result<(), ShellEnvironmentError> {
        self.warm_internal(variables, false, Instant::now() + self.timeout)
            .await
    }

    /// 通过一次 Shell 调用强制刷新所有请求变量。
    pub async fn refresh_many(&self, variables: &[String]) -> Result<(), ShellEnvironmentError> {
        self.warm_internal(variables, true, Instant::now() + self.timeout)
            .await
    }

    async fn warm_internal(
        &self,
        variables: &[String],
        force_refresh: bool,
        deadline: Instant,
    ) -> Result<(), ShellEnvironmentError> {
        let variables = normalize_variable_names(variables)?;
        if variables.is_empty() {
            return Ok(());
        }

        let remaining = remaining_until(deadline)?;
        let _load_guard = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::time::timeout(remaining, self.load_lock.lock())
                .await
                .map_err(|_| ShellEnvironmentError::Timeout)?
        } else {
            self.load_lock.lock().await
        };
        if force_refresh {
            for variable in &variables {
                self.clear_cached(variable)?;
            }
        }
        let mut missing = Vec::with_capacity(variables.len());
        for variable in variables {
            if !force_refresh
                && (self.cached(&variable)?.is_some() || self.is_missing_cached(&variable)?)
            {
                continue;
            }
            missing.push(variable.clone());
        }
        if missing.is_empty() {
            return Ok(());
        }

        let loaded = match self.load_from_shell_until(&missing, deadline).await {
            Ok(loaded) => loaded,
            Err(error) if !force_refresh => {
                let mut used_inherited_value = false;
                let mut all_resolved = true;
                for variable in &missing {
                    if let Some(value) = inherited_environment_value(variable) {
                        self.store_value(variable.clone(), value)?;
                        used_inherited_value = true;
                    } else {
                        all_resolved = false;
                    }
                }
                if used_inherited_value && all_resolved {
                    return Ok(());
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        for (variable, value) in &loaded {
            self.store_value(variable.clone(), value.clone())?;
        }
        for variable in missing {
            if loaded.contains_key(&variable) {
                continue;
            }
            if force_refresh {
                self.store_missing(variable)?;
            } else if let Some(value) = inherited_environment_value(&variable) {
                self.store_value(variable, value)?;
            } else {
                self.store_missing(variable)?;
            }
        }
        Ok(())
    }

    async fn load_from_shell_until(
        &self,
        variables: &[String],
        deadline: Instant,
    ) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
        let timeout = remaining_until(deadline)?;
        let shell_path = self
            .shell_path
            .clone()
            .or_else(|| self.detected_shell.get().cloned());
        if tokio::runtime::Handle::try_current().is_err() {
            return self
                .load_from_shell_on_runtime_thread(variables, timeout, shell_path)
                .await;
        }
        let (loaded, detected_shell) =
            load_from_shell_with_fallback(shell_path.as_deref(), timeout, variables).await?;
        if let Some(detected_shell) = detected_shell {
            let _ = self.detected_shell.set(detected_shell);
        }
        Ok(loaded)
    }

    async fn load_from_shell_on_runtime_thread(
        &self,
        variables: &[String],
        timeout: Duration,
        shell_path: Option<PathBuf>,
    ) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
        let variables = variables.to_vec();
        let (sender, receiver) = futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name("nyaterm-shell-environment".to_string())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(ShellEnvironmentError::Spawn)
                    .and_then(|runtime| {
                        runtime.block_on(load_from_shell_with_fallback(
                            shell_path.as_deref(),
                            timeout,
                            &variables,
                        ))
                    });
                let _ = sender.send(result);
            })
            .map_err(ShellEnvironmentError::Spawn)?;
        let (loaded, detected_shell) = receiver
            .await
            .map_err(|_| ShellEnvironmentError::ShellExit)??;
        if let Some(detected_shell) = detected_shell {
            let _ = self.detected_shell.set(detected_shell);
        }
        Ok(loaded)
    }
}

async fn load_from_shell_with_fallback(
    shell_path: Option<&Path>,
    timeout: Duration,
    variables: &[String],
) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
    #[cfg(windows)]
    if shell_path.is_none() {
        let (values, detected_shell) = load_from_windows_shell_candidates(
            windows_environment_shell_candidates(fallback_environment_shell_path()),
            timeout,
            variables,
        )
        .await?;
        return Ok((values, Some(detected_shell)));
    }

    let values = load_from_shell_with_runtime(shell_path, timeout, variables).await?;
    Ok((values, None))
}

#[cfg(windows)]
fn windows_environment_shell_candidates(fallback: PathBuf) -> [PathBuf; 3] {
    [
        PathBuf::from("powershell.exe"),
        PathBuf::from("pwsh.exe"),
        fallback,
    ]
}

#[cfg(windows)]
async fn load_from_windows_shell_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
    timeout: Duration,
    variables: &[String],
) -> Result<(HashMap<String, EnvironmentValue>, PathBuf), ShellEnvironmentError> {
    let mut last_spawn_error = None;
    for shell in candidates {
        match load_from_shell_with_runtime(Some(&shell), timeout, variables).await {
            Ok(values) => return Ok((values, shell)),
            Err(error @ ShellEnvironmentError::Spawn(_)) => last_spawn_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_spawn_error.unwrap_or(ShellEnvironmentError::ShellExit))
}

async fn load_from_shell_with_runtime(
    shell_path: Option<&Path>,
    timeout: Duration,
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let shell = shell_path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_environment_shell_path);
    let marker = format!("__NYATERM_ENV_{}__", uuid::Uuid::new_v4().simple());
    let script = build_shell_script(&shell, &marker, variables);
    let mut command = shell_command(&shell, &script);
    command
        .env(SHELL_ENV_READER_MARKER, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(ShellEnvironmentError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(ShellEnvironmentError::ShellExit)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(ShellEnvironmentError::ShellExit)?;
    let output = async {
        let status = child.wait();
        let stdout = read_limited(stdout);
        let stderr = read_limited(stderr);
        let (status, stdout, stderr) = tokio::join!(status, stdout, stderr);
        (status, stdout, stderr)
    };
    let (status, stdout, _stderr) = match tokio::time::timeout(timeout, output).await {
        Ok(output) => output,
        Err(_) => {
            let _ = child.kill().await;
            return Err(ShellEnvironmentError::Timeout);
        }
    };
    let status = status.map_err(ShellEnvironmentError::Read)?;
    let stdout = stdout?;
    let _stderr = _stderr?;
    if !status.success() {
        return Err(ShellEnvironmentError::ShellExit);
    }
    #[cfg(windows)]
    if !is_powershell_shell(&shell) {
        let parsed = parse_cmd_shell_output(&marker, &stdout, variables);
        #[cfg(test)]
        if parsed.is_err() {
            eprintln!(
                "cmd environment output diagnostics: {}",
                summarize_cmd_shell_output(&marker, &stdout)
            );
        }
        return parsed;
    }
    parse_shell_output(&marker, &stdout, variables)
}

impl ShellEnvironmentCache {
    fn clear_cached(&self, variable: &str) -> Result<(), ShellEnvironmentError> {
        let mut values = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        values.values.remove(variable);
        values.missing.remove(variable);
        Ok(())
    }

    fn store_value(
        &self,
        variable: String,
        value: EnvironmentValue,
    ) -> Result<(), ShellEnvironmentError> {
        let mut values = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        values.missing.remove(&variable);
        values.values.insert(variable, value);
        Ok(())
    }

    fn store_missing(&self, variable: String) -> Result<(), ShellEnvironmentError> {
        let mut values = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        values.values.remove(&variable);
        values.missing.insert(variable);
        Ok(())
    }

    pub(crate) fn is_missing_cached(&self, variable: &str) -> Result<bool, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        let values = self
            .values
            .read()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        Ok(values.missing.contains(&variable))
    }
}

fn normalize_variable_names(variables: &[String]) -> Result<Vec<String>, ShellEnvironmentError> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(variables.len());
    for variable in variables {
        let variable = normalize_environment_variable_name(variable)?;
        if seen.insert(variable.clone()) {
            normalized.push(variable);
        }
    }
    Ok(normalized)
}

/// 规范化并校验 Shell 环境变量名。
pub fn normalize_environment_variable_name(
    variable: &str,
) -> Result<String, ShellEnvironmentError> {
    let variable = variable.trim();
    let variable = variable.strip_prefix('$').unwrap_or(variable);
    let mut chars = variable.chars();
    let Some(first) = chars.next() else {
        return Err(ShellEnvironmentError::InvalidVariableName);
    };
    if !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(ShellEnvironmentError::InvalidVariableName);
    }
    Ok(variable.to_string())
}

fn inherited_environment_value(variable: &str) -> Option<EnvironmentValue> {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
        .map(EnvironmentValue::new)
}

fn remaining_until(deadline: Instant) -> Result<Duration, ShellEnvironmentError> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or(ShellEnvironmentError::Timeout)
}

#[cfg(not(windows))]
fn build_shell_script(_shell_path: &Path, marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let mut script = String::new();
    script.push_str(&format!("printf '\\n%s\\n' '{start}'\n"));
    for variable in variables {
        script.push_str(&format!(
            "  printf '\\n%s%s\\n' '{value_prefix}{variable}:' \"$(command printenv '{variable}' | command awk 'NR > 1 {{ printf \"\\n\" }} {{ printf \"%s\", $0 }}' | command base64 | command tr -d '\\r\\n')\"\n"
        ));
        script.push_str(&format!(
            "  printf '\\n%s\\n' '{value_end_prefix}{variable}'\n"
        ));
    }
    script.push_str(&format!("printf '\\n%s\\n' '{end}'\n"));
    script
}

#[cfg(windows)]
fn build_shell_script(shell_path: &Path, marker: &str, variables: &[String]) -> String {
    if is_powershell_shell(shell_path) {
        return build_powershell_shell_script(marker, variables);
    }

    build_cmd_shell_script(marker, variables)
}

#[cfg(windows)]
fn build_powershell_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let mut script = String::new();
    script.push_str(&format!("Write-Output '{start}'\n"));
    for variable in variables {
        script.push_str(&format!(
            "$value = [Environment]::GetEnvironmentVariable('{variable}', 'Process')\n"
        ));
        script.push_str("if ($null -ne $value) {\n");
        script.push_str(&format!(
            "  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($value))\n  Write-Output ('{value_prefix}{variable}:' + $encoded)\n"
        ));
        script.push_str("} else {\n");
        script.push_str(&format!("  Write-Output '{value_prefix}{variable}:'\n"));
        script.push_str("}\n");
        script.push_str(&format!("Write-Output '{value_end_prefix}{variable}'\n"));
    }
    script.push_str(&format!("Write-Output '{end}'\n"));
    script
}

#[cfg(windows)]
fn build_cmd_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let variable_start_prefix = format!("{marker}:VARIABLE_START:");
    let variable_end_prefix = format!("{marker}:VARIABLE_END:");
    let end = format!("{marker}:END");
    // 将一条命令链传给 `cmd /c`；Windows 命令处理器对内嵌换行的解释不一，
    // 可能截断协议帧。
    let mut commands = vec![String::from("@echo off"), String::from("chcp 65001 >nul")];
    commands.push(format!("echo {start}"));
    for variable in variables {
        commands.push(format!("echo {variable_start_prefix}{variable}"));
        // 查询保持为单个内置命令。缺失变量可能输出本地化诊断；重定向后由帧
        // 解析器忽略，避免使用脆弱的管道表达式。
        commands.push(format!("set {variable} 2>nul"));
        commands.push(format!("echo {variable_end_prefix}{variable}"));
    }
    commands.push(format!("echo {end}"));
    commands.join("&")
}

fn parse_shell_output(
    marker: &str,
    output: &[u8],
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let output = Zeroizing::new(String::from_utf8_lossy(output).into_owned());
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let expected: HashSet<&str> = variables.iter().map(String::as_str).collect();
    let mut result = HashMap::new();
    let mut started = false;
    let mut ended = false;
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line == start {
            started = true;
            continue;
        }
        if !started || ended {
            continue;
        }
        if line == end {
            ended = true;
            break;
        }
        if let Some(value_data) = line.strip_prefix(&value_prefix) {
            let Some((variable, encoded)) = value_data.split_once(':') else {
                continue;
            };
            if !expected.contains(variable) {
                continue;
            }
            let value_end = format!("{value_end_prefix}{variable}");
            let mut found_value_end = false;
            for candidate in lines.by_ref() {
                if candidate == value_end {
                    found_value_end = true;
                    break;
                }
            }
            if !found_value_end {
                return Err(ShellEnvironmentError::OutputEncoding);
            }
            if encoded.is_empty() {
                result.remove(variable);
            } else {
                let decoded = Zeroizing::new(
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                        .map_err(|_| ShellEnvironmentError::ValueEncoding)?,
                );
                let value = String::from_utf8(decoded.to_vec())
                    .map_err(|_| ShellEnvironmentError::ValueEncoding)?;
                result.insert(variable.to_string(), EnvironmentValue::new(value));
            }
        }
    }
    if !started || !ended {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(result)
}

#[cfg(windows)]
fn parse_cmd_shell_output(
    marker: &str,
    output: &[u8],
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    // `cmd.exe` 可能按当前代码页输出本地化诊断。协议标记和赋值名保持 ASCII，
    // 因此保留帧数据并将其他字节视为 Shell 噪声。
    let output = Zeroizing::new(String::from_utf8_lossy(output).into_owned());
    let start = format!("{marker}:START");
    let variable_start_prefix = format!("{marker}:VARIABLE_START:");
    let variable_end_prefix = format!("{marker}:VARIABLE_END:");
    let end = format!("{marker}:END");
    let expected: HashSet<&str> = variables.iter().map(String::as_str).collect();
    let mut result = HashMap::new();
    let mut current_variable = None;
    let mut started = false;
    let mut ended = false;
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        let marker_line = line.trim_end();
        if marker_line == start {
            started = true;
            continue;
        }
        if !started || ended {
            continue;
        }
        if marker_line == end {
            ended = true;
            break;
        }
        if let Some(variable) = marker_line.strip_prefix(&variable_start_prefix) {
            current_variable = expected.contains(variable).then_some(variable);
            continue;
        }
        if let Some(variable) = marker_line.strip_prefix(&variable_end_prefix) {
            if current_variable == Some(variable) {
                current_variable = None;
            } else {
                return Err(ShellEnvironmentError::OutputEncoding);
            }
            continue;
        }
        let Some(variable) = current_variable else {
            continue;
        };
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case(variable) && !value.is_empty() {
            result.insert(
                variable.to_string(),
                EnvironmentValue::new(value.to_string()),
            );
        }
    }
    if !started || !ended || current_variable.is_some() {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(result)
}

#[cfg(all(windows, test))]
fn summarize_cmd_shell_output(marker: &str, output: &[u8]) -> String {
    let count = |needle: &[u8]| {
        output
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    };
    let start = format!("{marker}:START");
    let end = format!("{marker}:END");
    let variable_start = format!("{marker}:VARIABLE_START:");
    let variable_end = format!("{marker}:VARIABLE_END:");
    format!(
        "bytes={}, utf8={}, nul_bytes={}, odd_bytes={}, cr={}, lf={}, start={}, end={}, variable_start={}, variable_end={}",
        output.len(),
        std::str::from_utf8(output).is_ok(),
        output.iter().filter(|byte| **byte == 0).count(),
        output.len() % 2,
        output.iter().filter(|byte| **byte == b'\r').count(),
        output.iter().filter(|byte| **byte == b'\n').count(),
        count(start.as_bytes()),
        count(end.as_bytes()),
        count(variable_start.as_bytes()),
        count(variable_end.as_bytes()),
    )
}

async fn read_limited<R>(reader: R) -> Result<Zeroizing<Vec<u8>>, ShellEnvironmentError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Zeroizing::new(Vec::new());
    reader
        .take((MAX_SHELL_OUTPUT + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(ShellEnvironmentError::Read)?;
    if output.len() > MAX_SHELL_OUTPUT {
        return Err(ShellEnvironmentError::OutputTooLarge);
    }
    Ok(output)
}

fn shell_command(shell_path: &Path, script: &str) -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new(shell_path);
        command.args(["-i", "-l", "-c", script]);
        command
    }
    #[cfg(windows)]
    {
        let mut command = Command::new(shell_path);
        if is_powershell_shell(shell_path) {
            command.args(["-NoLogo", "-Command", script]);
        } else {
            command.args(["/d", "/s", "/c", script]);
        }
        command
    }
    #[cfg(not(any(unix, windows)))]
    {
        let mut command = Command::new(shell_path);
        command.args(["-c", script]);
        command
    }
}

#[cfg(windows)]
fn is_powershell_shell(shell_path: &Path) -> bool {
    matches!(
        shell_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "powershell" | "pwsh"
    )
}

#[cfg(windows)]
fn default_environment_shell_path() -> PathBuf {
    PathBuf::from("powershell.exe")
}

#[cfg(not(windows))]
fn default_environment_shell_path() -> PathBuf {
    PathBuf::from(default_shell())
}

#[cfg(windows)]
fn fallback_environment_shell_path() -> PathBuf {
    std::env::var_os("COMSPEC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cmd.exe"))
}

#[cfg(test)]
mod tests {
    use super::{
        EnvironmentValue, ShellEnvironmentCache, ShellEnvironmentError, parse_shell_output,
    };
    #[cfg(windows)]
    use super::{
        build_shell_script, load_from_windows_shell_candidates, parse_cmd_shell_output,
        shell_command, windows_environment_shell_candidates,
    };
    #[cfg(windows)]
    use std::io::ErrorKind;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(windows)]
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    #[cfg(unix)]
    use std::{fs, path::PathBuf};

    #[test]
    fn variable_name_validation_rejects_shell_code() {
        let cache = ShellEnvironmentCache::new();
        let error = cache.cached("VALUE; touch /tmp/pwned").unwrap_err();
        assert!(matches!(error, ShellEnvironmentError::InvalidVariableName));
    }

    #[test]
    fn parser_ignores_shell_noise_and_reads_marked_values() {
        let output = b"welcome\n__NYATERM_ENV_test__:START\n__NYATERM_ENV_test__:VALUE:PATH:L3RtcA==\nhook-noise\nhook-after-value\n__NYATERM_ENV_test__:VALUE_END:PATH\n__NYATERM_ENV_test__:END\n";
        let variables = vec!["PATH".to_string()];
        let values = parse_shell_output("__NYATERM_ENV_test__", output, &variables).unwrap();
        assert_eq!(values.get("PATH").map(|value| value.as_str()), Some("/tmp"));
    }

    #[tokio::test]
    async fn resolve_caches_a_requested_value() {
        let cache = ShellEnvironmentCache::new();
        let value = cache.refresh("PATH").await.unwrap();
        assert!(value.is_some());
        assert!(cache.cached("PATH").unwrap().is_some());
    }

    #[tokio::test]
    async fn missing_variable_is_cached_until_refresh() {
        let cache = ShellEnvironmentCache::new();
        let variable = format!("NYATERM_TEST_MISSING_{}", uuid::Uuid::new_v4().simple());
        assert!(cache.resolve(&variable).await.unwrap().is_none());
        assert!(cache.is_missing_cached(&variable).unwrap());
        assert!(cache.resolve(&variable).await.unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn powershell_shell_loader_uses_powershell_protocol() {
        let marker = "__NYATERM_ENV_TEST__";
        let variables = vec!["SSH_AUTH_SOCK".to_string()];
        let script = build_shell_script(Path::new("powershell.exe"), marker, &variables);

        assert!(script.contains("Write-Output '__NYATERM_ENV_TEST__:START'"));
        assert!(
            script.contains("[Environment]::GetEnvironmentVariable('SSH_AUTH_SOCK', 'Process')")
        );
        assert!(!script.contains("@echo off"));
        assert!(!script.contains("set SSH_AUTH_SOCK"));

        let command = shell_command(Path::new("powershell.exe"), &script);
        let arguments: Vec<_> = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            vec!["-NoLogo".to_string(), "-Command".to_string(), script]
        );
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shell_loader_uses_cmd_protocol() {
        let marker = "__NYATERM_ENV_TEST__";
        let variables = vec!["SSH_AUTH_SOCK".to_string()];
        let script = build_shell_script(Path::new("cmd.exe"), marker, &variables);

        assert!(script.starts_with("@echo off&chcp 65001 >nul&"));
        assert!(script.contains("set SSH_AUTH_SOCK 2>nul"));
        assert!(!script.contains("Write-Output"));
        assert!(!script.contains("[Environment]::GetEnvironmentVariable"));

        let command = shell_command(Path::new("cmd.exe"), &script);
        let arguments: Vec<_> = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            vec!["/d".to_string(), "/s".to_string(), "/c".to_string(), script,]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_environment_shell_candidates_prioritize_powershell_before_cmd() {
        let fallback = PathBuf::from("C:\\Windows\\System32\\cmd.exe");

        assert_eq!(
            windows_environment_shell_candidates(fallback.clone()),
            [
                PathBuf::from("powershell.exe"),
                PathBuf::from("pwsh.exe"),
                fallback,
            ]
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_shell_loader_falls_back_to_cmd_after_spawn_failure() {
        let missing_shell = PathBuf::from(format!(
            "nyaterm-missing-shell-{}.exe",
            uuid::Uuid::new_v4().simple()
        ));
        let cmd = PathBuf::from("cmd.exe");
        let variables = vec!["PATH".to_string()];

        let (values, detected_shell) = load_from_windows_shell_candidates(
            [missing_shell, cmd.clone()],
            Duration::from_secs(5),
            &variables,
        )
        .await
        .expect("cmd fallback loads PATH");

        assert_eq!(detected_shell, cmd);
        assert!(values.contains_key("PATH"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn powershell_shell_loader_reads_requested_value_when_available() {
        let mut cache = ShellEnvironmentCache::new();
        Arc::get_mut(&mut cache)
            .expect("new cache is not shared")
            .shell_path = Some(PathBuf::from("powershell.exe"));

        match cache.refresh("PATH").await {
            Ok(value) => assert!(value.is_some()),
            Err(ShellEnvironmentError::Spawn(error)) if error.kind() == ErrorKind::NotFound => {
                eprintln!("Windows PowerShell is unavailable; cmd fallback is covered separately");
            }
            Err(error) => panic!("PowerShell environment lookup failed: {error}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shell_output_parser_reads_only_requested_values() {
        let marker = "__NYATERM_ENV_TEST__";
        let output = concat!(
            "__NYATERM_ENV_TEST__:START \r\n",
            "__NYATERM_ENV_TEST__:VARIABLE_START:PATH \r\n",
            "PATH=C:\\Windows\\System32;C:\\Tools=stable\r\n",
            "PATH_EXTRA=must-not-leak\r\n",
            "__NYATERM_ENV_TEST__:VARIABLE_END:PATH \r\n",
            "__NYATERM_ENV_TEST__:VARIABLE_START:MISSING \r\n",
            "Environment variable MISSING is not defined.\r\n",
            "__NYATERM_ENV_TEST__:VARIABLE_END:MISSING \r\n",
            "__NYATERM_ENV_TEST__:END \r\n",
        );
        let variables = vec!["PATH".to_string(), "MISSING".to_string()];
        let values = parse_cmd_shell_output(marker, output.as_bytes(), &variables).unwrap();

        assert_eq!(
            values.get("PATH").map(EnvironmentValue::as_str),
            Some("C:\\Windows\\System32;C:\\Tools=stable")
        );
        assert!(!values.contains_key("MISSING"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cmd_shell_loader_reads_a_requested_environment_value() {
        let mut cache = ShellEnvironmentCache::new();
        Arc::get_mut(&mut cache).unwrap().shell_path = Some(PathBuf::from("cmd.exe"));

        let value = cache.refresh("PATH").await.unwrap();

        assert!(value.is_some());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cmd_shell_loader_caches_a_missing_variable() {
        let mut cache = ShellEnvironmentCache::new();
        Arc::get_mut(&mut cache).unwrap().shell_path = Some(PathBuf::from("cmd.exe"));
        let variable = format!("NYATERM_CMD_MISSING_{}", uuid::Uuid::new_v4().simple());

        assert!(cache.refresh(&variable).await.unwrap().is_none());
        assert!(cache.is_missing_cached(&variable).unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refresh_drops_a_stale_value_when_shell_cannot_start() {
        let mut cache = ShellEnvironmentCache::new();
        cache
            .store_value(
                "SSH_AUTH_SOCK".to_string(),
                EnvironmentValue::new("/stale/agent.sock".to_string()),
            )
            .unwrap();
        Arc::get_mut(&mut cache).unwrap().shell_path =
            Some(std::path::PathBuf::from("/path/that/does/not/exist"));

        assert!(cache.refresh("SSH_AUTH_SOCK").await.is_err());
        assert!(cache.cached("SSH_AUTH_SOCK").unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn shell_loader_works_without_a_tokio_runtime() {
        let cache = ShellEnvironmentCache::new();
        let value = futures::executor::block_on(cache.resolve("PATH")).unwrap();
        assert!(value.is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_loader_reads_a_requested_exported_value() {
        let cache = ShellEnvironmentCache::new();
        let variables = vec!["PATH".to_string()];
        let values = cache
            .load_from_shell_until(&variables, Instant::now() + Duration::from_secs(10))
            .await
            .unwrap();
        assert!(values.contains_key("PATH"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_loader_reads_a_custom_exported_value() {
        let root = std::env::temp_dir().join(format!(
            "nyaterm-shell-environment-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir(&root).expect("create shell test directory");
        let shell = root.join("shell");
        fs::write(
            &shell,
            "#!/bin/sh\nexport NYATERM_TEST_CUSTOM_AGENT=/tmp/nyaterm-agent.sock\nexec /bin/sh \"$@\"\n",
        )
        .expect("write shell test wrapper");
        let mut permissions = fs::metadata(&shell)
            .expect("read shell test wrapper permissions")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&shell, permissions).expect("make shell test wrapper executable");

        let mut cache = ShellEnvironmentCache::new();
        Arc::get_mut(&mut cache)
            .expect("new cache is not shared")
            .shell_path = Some(PathBuf::from(&shell));
        let value = cache
            .refresh("NYATERM_TEST_CUSTOM_AGENT")
            .await
            .expect("load custom exported value")
            .expect("custom exported value is present");

        assert_eq!(value.as_str(), "/tmp/nyaterm-agent.sock");
        fs::remove_dir_all(root).expect("remove shell test directory");
    }

    #[test]
    fn environment_value_debug_output_is_redacted() {
        let value = EnvironmentValue::new("/private/agent.sock".to_string());
        assert_eq!(format!("{value:?}"), "<redacted>");
    }
}
