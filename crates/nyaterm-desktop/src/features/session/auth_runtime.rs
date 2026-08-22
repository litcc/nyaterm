use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nyaterm_core::DecryptedOtpEntry;

use crate::models::event_wake::{ANY_INTEREST, EventWake};
use nyaterm_store::{KnownHostCheck, StoreBlockingClient, StoreDomain};
use nyaterm_transport::{
    SftpDuplicateDecision, SftpDuplicateRequest, SftpDuplicateResolver, SshAgentPrompt,
    SshAgentPromptAction, SshAgentPromptProvider, SshCredentialPrompt, SshCredentialProvider,
    SshHostKey, SshHostKeyDecision, SshHostKeyVerifier, SshKeyboardInteractiveRequest,
    SshOtpProvider,
};

use super::{
    credential_prompt_id, keyboard_interactive_prompt_id, sftp_duplicate_prompt_id,
    uuid_like_prompt_id,
};

pub(in crate::features) struct NativeHostKeyVerifier {
    pub(in crate::features) store: StoreBlockingClient,
    pub(in crate::features) policy: String,
    pub(in crate::features) prompt_broker: Arc<HostKeyPromptBroker>,
}

impl SshHostKeyVerifier for NativeHostKeyVerifier {
    fn verify(&self, host_key: &SshHostKey) -> Result<SshHostKeyDecision, String> {
        let line = format!(
            "{} {} {}",
            host_key.host_identifier, host_key.key_type, host_key.key_base64
        );
        let host_identifier = host_key.host_identifier.clone();
        let key_type = host_key.key_type.clone();
        let key_base64 = host_key.key_base64.clone();
        match self
            .store
            .request_fn(StoreDomain::Security, move |store| {
                store.check_known_host(&host_identifier, &key_type, &key_base64)
            })
            .map_err(|error| error.to_string())?
        {
            KnownHostCheck::Match => Ok(SshHostKeyDecision::Accept),
            KnownHostCheck::UnknownHost if self.policy == "strict" => {
                Ok(SshHostKeyDecision::Reject(format!(
                    "unknown SSH host key for {} ({})",
                    host_key.host_identifier, host_key.fingerprint
                )))
            }
            KnownHostCheck::UnknownHost if self.policy == "prompt" => {
                match self
                    .prompt_broker
                    .request_decision(host_key.clone(), HostKeyPromptIssue::Unknown)
                {
                    Ok(HostKeyPromptChoice::Accept) => {
                        self.store
                            .request_fn(StoreDomain::Security, move |store| {
                                store.upsert_known_host(&line)
                            })
                            .map_err(|error| error.to_string())?;
                        Ok(SshHostKeyDecision::Accept)
                    }
                    Ok(HostKeyPromptChoice::Reject) => Ok(SshHostKeyDecision::Reject(format!(
                        "unknown SSH host key rejected for {} ({})",
                        host_key.host_identifier, host_key.fingerprint
                    ))),
                    Err(error) => Ok(SshHostKeyDecision::Reject(error)),
                }
            }
            KnownHostCheck::UnknownHost => {
                self.store
                    .request_fn(StoreDomain::Security, move |store| {
                        store.upsert_known_host(&line)
                    })
                    .map_err(|error| error.to_string())?;
                Ok(SshHostKeyDecision::Accept)
            }
            KnownHostCheck::HostSeen if self.policy == "accept" => {
                let host_identifier = host_key.host_identifier.clone();
                self.store
                    .request_fn(StoreDomain::Security, move |store| {
                        store.replace_known_host_for_host(&host_identifier, &line)
                    })
                    .map_err(|error| error.to_string())?;
                Ok(SshHostKeyDecision::Accept)
            }
            KnownHostCheck::HostSeen if self.policy == "prompt" => {
                match self
                    .prompt_broker
                    .request_decision(host_key.clone(), HostKeyPromptIssue::Changed)
                {
                    Ok(HostKeyPromptChoice::Accept) => {
                        let host_identifier = host_key.host_identifier.clone();
                        self.store
                            .request_fn(StoreDomain::Security, move |store| {
                                store.replace_known_host_for_host(&host_identifier, &line)
                            })
                            .map_err(|error| error.to_string())?;
                        Ok(SshHostKeyDecision::Accept)
                    }
                    Ok(HostKeyPromptChoice::Reject) => Ok(SshHostKeyDecision::Reject(format!(
                        "changed SSH host key rejected for {} ({})",
                        host_key.host_identifier, host_key.fingerprint
                    ))),
                    Err(error) => Ok(SshHostKeyDecision::Reject(error)),
                }
            }
            KnownHostCheck::HostSeen => Ok(SshHostKeyDecision::Reject(format!(
                "SSH host key changed for {} ({})",
                host_key.host_identifier, host_key.fingerprint
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TotpUseRecord {
    code: String,
    time_step: u64,
}

pub(in crate::features) struct NativeOtpProvider {
    store: StoreBlockingClient,
    used_totp_codes: Mutex<HashMap<String, TotpUseRecord>>,
}

impl NativeOtpProvider {
    pub(in crate::features) fn new(store: StoreBlockingClient) -> Self {
        Self {
            store,
            used_totp_codes: Mutex::new(HashMap::new()),
        }
    }

    fn load_entry(&self, otp_id: &str) -> Result<Option<DecryptedOtpEntry>, String> {
        let otp_id = otp_id.to_string();
        self.store
            .request_fn(StoreDomain::Security, move |store| {
                store.load_decrypted_otp_entry_by_id(&otp_id)
            })
            .map_err(|error| error.to_string())
    }

    fn generate_totp_code(&self, entry: &DecryptedOtpEntry, now: u64) -> Result<TotpCode, String> {
        let (algorithm, secret, digits) = otp_material(entry)?;
        let period = if entry.period > 0 { entry.period } else { 30 };
        let totp = nyaterm_otp::Totp::new(
            algorithm,
            entry.issuer.clone(),
            entry.username.clone(),
            digits,
            period,
            secret,
        );
        let raw = totp.generate_at(now);
        Ok(TotpCode {
            code: format!("{:0>width$}", raw, width = digits as usize),
            time_step: now / period,
            period,
        })
    }

    fn generate_hotp_code(&self, entry: &DecryptedOtpEntry) -> Result<String, String> {
        let (algorithm, secret, digits) = otp_material(entry)?;
        let mut hotp = nyaterm_otp::Hotp::new(
            algorithm,
            entry.issuer.clone(),
            entry.username.clone(),
            digits,
            entry.counter,
            secret,
        );
        let raw = hotp.generate();
        Ok(format!("{:0>width$}", raw, width = digits as usize))
    }

    fn increment_counter(&self, otp_id: &str) -> Result<(), String> {
        let otp_id = otp_id.to_string();
        self.store
            .request_fn(StoreDomain::Security, move |store| {
                store.increment_otp_counter(&otp_id)
            })
            .map_err(|error| error.to_string())
    }

    fn has_used_totp_code(&self, otp_id: &str, candidate: &TotpCode) -> Result<bool, String> {
        let used = self
            .used_totp_codes
            .lock()
            .map_err(|_| "TOTP use cache is poisoned".to_string())?;
        Ok(used.get(otp_id).is_some_and(|record| {
            record.code == candidate.code && record.time_step == candidate.time_step
        }))
    }

    fn record_totp_code(&self, otp_id: &str, candidate: &TotpCode) -> Result<(), String> {
        let mut used = self
            .used_totp_codes
            .lock()
            .map_err(|_| "TOTP use cache is poisoned".to_string())?;
        used.insert(
            otp_id.to_string(),
            TotpUseRecord {
                code: candidate.code.clone(),
                time_step: candidate.time_step,
            },
        );
        Ok(())
    }
}

impl fmt::Debug for NativeOtpProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let used_code_count = self
            .used_totp_codes
            .lock()
            .map(|codes| codes.len())
            .unwrap_or_default();
        formatter
            .debug_struct("NativeOtpProvider")
            .field("used_code_count", &used_code_count)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct TotpCode {
    code: String,
    time_step: u64,
    period: u64,
}

#[derive(Debug, Clone)]
pub(in crate::features) struct NativeOtpCodePreview {
    pub(in crate::features) code: String,
    pub(in crate::features) otp_type: String,
    pub(in crate::features) period: u64,
    pub(in crate::features) time_step: Option<u64>,
}

impl NativeOtpProvider {
    pub(in crate::features) fn preview_otp_code(
        &self,
        otp_id: &str,
    ) -> Result<Option<NativeOtpCodePreview>, String> {
        let Some(entry) = self.load_entry(otp_id)? else {
            return Ok(None);
        };
        if entry.otp_type.eq_ignore_ascii_case("hotp") {
            let code = self.generate_hotp_code(&entry)?;
            self.increment_counter(otp_id)?;
            return Ok(Some(NativeOtpCodePreview {
                code,
                otp_type: "hotp".to_string(),
                period: 0,
                time_step: None,
            }));
        }

        let now = unix_seconds_now();
        let code = self.generate_totp_code(&entry, now)?;
        Ok(Some(NativeOtpCodePreview {
            code: code.code,
            otp_type: "totp".to_string(),
            period: code.period,
            time_step: Some(code.time_step),
        }))
    }
}

impl SshOtpProvider for NativeOtpProvider {
    fn request_otp_code(&self, otp_id: &str) -> Result<Option<String>, String> {
        let Some(entry) = self.load_entry(otp_id)? else {
            return Ok(None);
        };
        if entry.otp_type.eq_ignore_ascii_case("hotp") {
            let code = self.generate_hotp_code(&entry)?;
            self.increment_counter(otp_id)?;
            return Ok(Some(code));
        }

        let mut now = unix_seconds_now();
        let mut code = self.generate_totp_code(&entry, now)?;
        if self.has_used_totp_code(otp_id, &code)? {
            let wait = seconds_until_next_totp_step(now, code.period);
            std::thread::sleep(Duration::from_secs(wait));
            now = unix_seconds_now();
            code = self.generate_totp_code(&entry, now)?;
        }
        self.record_totp_code(otp_id, &code)?;
        Ok(Some(code.code))
    }
}

fn otp_material(
    entry: &DecryptedOtpEntry,
) -> Result<(nyaterm_otp::Algorithm, nyaterm_otp::Secret, u8), String> {
    let secret = entry
        .secret
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("OTP entry '{}' has no secret", entry.id))?;
    let algorithm = match entry.algorithm.as_str() {
        "SHA256" => nyaterm_otp::Algorithm::SHA256,
        "SHA512" => nyaterm_otp::Algorithm::SHA512,
        _ => nyaterm_otp::Algorithm::SHA1,
    };
    let secret = nyaterm_otp::Secret::from_base32(secret)
        .map_err(|error| format!("invalid OTP secret for '{}': {error:?}", entry.id))?;
    let digits = if entry.digits > 0 { entry.digits } else { 6 };
    Ok((algorithm, secret, digits))
}

pub(in crate::features) fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn seconds_until_next_totp_step(now: u64, period: u64) -> u64 {
    let period = period.max(1);
    let remaining = period - (now % period);
    remaining.max(1)
}

#[derive(Debug)]
pub(in crate::features) struct SftpDuplicatePromptRequest {
    pub(in crate::features) id: String,
    pub(in crate::features) request: SftpDuplicateRequest,
    pub(in crate::features) response_tx: mpsc::Sender<SftpDuplicateDecision>,
}

#[derive(Debug, Clone)]
pub(in crate::features) struct SftpDuplicatePromptState {
    pub(in crate::features) id: String,
    pub(in crate::features) request: SftpDuplicateRequest,
    pub(in crate::features) response_tx: mpsc::Sender<SftpDuplicateDecision>,
}

#[derive(Debug, Default)]
pub(in crate::features) struct SftpDuplicatePromptBroker {
    pending: Mutex<VecDeque<SftpDuplicatePromptRequest>>,
    /// Signalled after a transport thread enqueues, so activation does not
    /// have to be polled. `Option` because the brokers are `Default`-built,
    /// including in tests that drive them without a wake at all.
    wake: Mutex<Option<EventWake>>,
}

impl SftpDuplicatePromptBroker {
    pub(in crate::features) fn set_wake(&self, wake: EventWake) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    fn signal_wake(&self) {
        if let Ok(slot) = self.wake.lock()
            && let Some(wake) = slot.as_ref()
        {
            wake.signal(ANY_INTEREST);
        }
    }

    fn request_decision(
        &self,
        request: SftpDuplicateRequest,
    ) -> Result<SftpDuplicateDecision, String> {
        let (response_tx, response_rx) = mpsc::channel();
        let request = SftpDuplicatePromptRequest {
            id: sftp_duplicate_prompt_id(&request),
            request,
            response_tx,
        };
        self.pending
            .lock()
            .map_err(|_| "remote transfer duplicate prompt queue is poisoned".to_string())?
            .push_back(request);
        self.signal_wake();

        response_rx
            .recv_timeout(Duration::from_secs(300))
            .map_err(|_| "remote transfer duplicate prompt timed out".to_string())
    }

    pub(in crate::features) fn pop_pending(&self) -> Option<SftpDuplicatePromptRequest> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.pop_front())
    }

    pub(in crate::features) fn has_pending(&self) -> bool {
        self.pending
            .lock()
            .ok()
            .is_some_and(|pending| !pending.is_empty())
    }
}

impl SftpDuplicateResolver for SftpDuplicatePromptBroker {
    fn resolve_duplicate(
        &self,
        request: &SftpDuplicateRequest,
    ) -> Result<SftpDuplicateDecision, String> {
        self.request_decision(request.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum HostKeyPromptIssue {
    Unknown,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum HostKeyPromptChoice {
    Accept,
    Reject,
}

#[derive(Debug, Clone)]
pub(in crate::features) struct HostKeyPromptRequest {
    pub(in crate::features) id: String,
    pub(in crate::features) host_key: SshHostKey,
    pub(in crate::features) issue: HostKeyPromptIssue,
    pub(in crate::features) response_tx: mpsc::Sender<HostKeyPromptChoice>,
}

#[derive(Debug, Default)]
pub(in crate::features) struct HostKeyPromptBroker {
    pending: Mutex<VecDeque<HostKeyPromptRequest>>,
    /// Signalled after a transport thread enqueues, so activation does not
    /// have to be polled. `Option` because the brokers are `Default`-built,
    /// including in tests that drive them without a wake at all.
    wake: Mutex<Option<EventWake>>,
}

impl HostKeyPromptBroker {
    pub(in crate::features) fn set_wake(&self, wake: EventWake) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    fn signal_wake(&self) {
        if let Ok(slot) = self.wake.lock()
            && let Some(wake) = slot.as_ref()
        {
            wake.signal(ANY_INTEREST);
        }
    }

    fn request_decision(
        &self,
        host_key: SshHostKey,
        issue: HostKeyPromptIssue,
    ) -> Result<HostKeyPromptChoice, String> {
        let (response_tx, response_rx) = mpsc::channel();
        let request = HostKeyPromptRequest {
            id: uuid_like_prompt_id(&host_key),
            host_key,
            issue,
            response_tx,
        };
        self.pending
            .lock()
            .map_err(|_| "host-key prompt queue is poisoned".to_string())?
            .push_back(request);
        self.signal_wake();

        response_rx
            .recv_timeout(Duration::from_secs(300))
            .map_err(|_| "SSH host-key prompt timed out".to_string())
    }

    pub(in crate::features) fn pop_pending(&self) -> Option<HostKeyPromptRequest> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.pop_front())
    }

    pub(in crate::features) fn has_pending(&self) -> bool {
        self.pending
            .lock()
            .ok()
            .is_some_and(|pending| !pending.is_empty())
    }
}

#[derive(Debug)]
pub(in crate::features) enum CredentialPromptRequest {
    Secret {
        id: String,
        prompt: SshCredentialPrompt,
        response_tx: mpsc::Sender<Option<String>>,
    },
    KeyboardInteractive {
        id: String,
        request: SshKeyboardInteractiveRequest,
        response_tx: mpsc::Sender<Option<Vec<String>>>,
    },
}

#[derive(Clone)]
pub(in crate::features) struct CredentialPromptState {
    pub(in crate::features) id: String,
    pub(in crate::features) prompt: SshCredentialPrompt,
    pub(in crate::features) response_tx: mpsc::Sender<Option<String>>,
    pub(in crate::features) value: String,
}

impl std::fmt::Debug for CredentialPromptState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialPromptState")
            .field("id", &self.id)
            .field("prompt", &self.prompt)
            .field("value", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub(in crate::features) struct KeyboardInteractivePromptState {
    pub(in crate::features) id: String,
    pub(in crate::features) request: SshKeyboardInteractiveRequest,
    pub(in crate::features) response_tx: mpsc::Sender<Option<Vec<String>>>,
    pub(in crate::features) responses: Vec<String>,
    pub(in crate::features) focused_index: usize,
    pub(in crate::features) otp_code: Option<String>,
    pub(in crate::features) otp_type: Option<String>,
    pub(in crate::features) otp_period: u64,
    pub(in crate::features) otp_time_step: Option<u64>,
    pub(in crate::features) otp_error: Option<String>,
}

impl std::fmt::Debug for KeyboardInteractivePromptState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeyboardInteractivePromptState")
            .field("id", &self.id)
            .field("request", &self.request)
            .field("response_count", &self.responses.len())
            .field("focused_index", &self.focused_index)
            .field("otp_type", &self.otp_type)
            .field("otp_period", &self.otp_period)
            .field("otp_time_step", &self.otp_time_step)
            .field("otp_error", &self.otp_error)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(in crate::features) struct CredentialPromptBroker {
    pending: Mutex<VecDeque<CredentialPromptRequest>>,
    /// Signalled after a transport thread enqueues, so activation does not
    /// have to be polled. `Option` because the brokers are `Default`-built,
    /// including in tests that drive them without a wake at all.
    wake: Mutex<Option<EventWake>>,
}

impl CredentialPromptBroker {
    pub(in crate::features) fn set_wake(&self, wake: EventWake) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    fn signal_wake(&self) {
        if let Ok(slot) = self.wake.lock()
            && let Some(wake) = slot.as_ref()
        {
            wake.signal(ANY_INTEREST);
        }
    }

    fn request_secret(&self, prompt: SshCredentialPrompt) -> Result<Option<String>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        let request = CredentialPromptRequest::Secret {
            id: credential_prompt_id(&prompt),
            prompt,
            response_tx,
        };
        self.pending
            .lock()
            .map_err(|_| "credential prompt queue is poisoned".to_string())?
            .push_back(request);
        self.signal_wake();

        response_rx
            .recv_timeout(Duration::from_secs(300))
            .map_err(|_| "SSH credential prompt timed out".to_string())
    }

    fn request_keyboard_interactive(
        &self,
        request: SshKeyboardInteractiveRequest,
    ) -> Result<Option<Vec<String>>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        let queued = CredentialPromptRequest::KeyboardInteractive {
            id: keyboard_interactive_prompt_id(&request),
            request,
            response_tx,
        };
        self.pending
            .lock()
            .map_err(|_| "credential prompt queue is poisoned".to_string())?
            .push_back(queued);
        self.signal_wake();

        response_rx
            .recv_timeout(Duration::from_secs(300))
            .map_err(|_| "SSH keyboard-interactive prompt timed out".to_string())
    }

    pub(in crate::features) fn pop_pending(&self) -> Option<CredentialPromptRequest> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.pop_front())
    }

    pub(in crate::features) fn has_pending(&self) -> bool {
        self.pending
            .lock()
            .ok()
            .is_some_and(|pending| !pending.is_empty())
    }
}

impl SshCredentialProvider for CredentialPromptBroker {
    fn request_secret(&self, prompt: &SshCredentialPrompt) -> Result<Option<String>, String> {
        CredentialPromptBroker::request_secret(self, prompt.clone())
    }

    fn request_keyboard_interactive(
        &self,
        request: &SshKeyboardInteractiveRequest,
    ) -> Result<Option<Vec<String>>, String> {
        CredentialPromptBroker::request_keyboard_interactive(self, request.clone())
    }
}

#[derive(Debug, Clone)]
pub(in crate::features) struct AgentPromptRequest {
    pub(in crate::features) id: String,
    pub(in crate::features) prompt: SshAgentPrompt,
    pub(in crate::features) response_tx: mpsc::Sender<SshAgentPromptAction>,
}

#[derive(Debug, Default)]
pub(in crate::features) struct AgentPromptBroker {
    pending: Mutex<VecDeque<AgentPromptRequest>>,
    /// Signalled after a transport thread enqueues, so activation does not
    /// have to be polled. `Option` because the brokers are `Default`-built,
    /// including in tests that drive them without a wake at all.
    wake: Mutex<Option<EventWake>>,
}

impl AgentPromptBroker {
    pub(in crate::features) fn set_wake(&self, wake: EventWake) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    fn signal_wake(&self) {
        if let Ok(slot) = self.wake.lock()
            && let Some(wake) = slot.as_ref()
        {
            wake.signal(ANY_INTEREST);
        }
    }

    fn request_action(&self, prompt: SshAgentPrompt) -> Result<SshAgentPromptAction, String> {
        self.request_action_with_timeout(prompt, Duration::from_secs(300))
    }

    fn request_action_with_timeout(
        &self,
        prompt: SshAgentPrompt,
        timeout: Duration,
    ) -> Result<SshAgentPromptAction, String> {
        let (response_tx, response_rx) = mpsc::channel();
        let id = agent_prompt_id(&prompt);
        let request = AgentPromptRequest {
            id: id.clone(),
            prompt,
            response_tx,
        };
        self.pending
            .lock()
            .map_err(|_| "SSH Agent prompt queue is poisoned".to_string())?
            .push_back(request);
        self.signal_wake();
        match response_rx.recv_timeout(timeout) {
            Ok(action) => Ok(action),
            Err(_) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.retain(|request| request.id != id);
                }
                Err("SSH Agent prompt timed out".to_string())
            }
        }
    }

    pub(in crate::features) fn pop_pending(&self) -> Option<AgentPromptRequest> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.pop_front())
    }

    pub(in crate::features) fn has_pending(&self) -> bool {
        self.pending
            .lock()
            .ok()
            .is_some_and(|pending| !pending.is_empty())
    }
}

impl SshAgentPromptProvider for AgentPromptBroker {
    fn request_action(&self, prompt: &SshAgentPrompt) -> Result<SshAgentPromptAction, String> {
        AgentPromptBroker::request_action(self, prompt.clone())
    }
}

fn agent_prompt_id(prompt: &SshAgentPrompt) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    prompt.connection_name.hash(&mut hasher);
    prompt.host.hash(&mut hasher);
    prompt.port.hash(&mut hasher);
    prompt.username.hash(&mut hasher);
    prompt.phase.hash(&mut hasher);
    prompt.attempt.hash(&mut hasher);
    format!("agent-{:016x}", hasher.finish())
}

#[cfg(test)]
mod prompt_state_debug_tests {
    use super::{
        AgentPromptBroker, CredentialPromptState, KeyboardInteractivePromptState,
        NativeOtpProvider, TotpUseRecord,
    };
    use nyaterm_store::{StoreConfig, StoreRuntime};
    use nyaterm_transport::{
        SshAgentPrompt, SshAgentPromptAction, SshAgentPromptPhase, SshCredentialPrompt,
        SshCredentialPromptKind, SshCredentialPromptReason, SshKeyboardInteractivePrompt,
        SshKeyboardInteractiveRequest,
    };
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    fn agent_prompt() -> SshAgentPrompt {
        SshAgentPrompt {
            host: "example.com".to_string(),
            port: 22,
            username: "alice".to_string(),
            connection_name: "Example".to_string(),
            phase: SshAgentPromptPhase::Sign,
            attempt: 1,
            message: "approve hardware key".to_string(),
        }
    }

    #[test]
    fn agent_prompt_broker_delivers_retry_action() {
        let broker = Arc::new(AgentPromptBroker::default());
        let worker = {
            let broker = Arc::clone(&broker);
            std::thread::spawn(move || broker.request_action(agent_prompt()))
        };
        let request = loop {
            if let Some(request) = broker.pop_pending() {
                break request;
            }
            std::thread::yield_now();
        };
        request
            .response_tx
            .send(SshAgentPromptAction::Retry)
            .expect("send retry action");
        assert_eq!(
            worker.join().expect("join prompt worker"),
            Ok(SshAgentPromptAction::Retry)
        );
    }

    #[test]
    fn agent_prompt_broker_removes_timed_out_request() {
        let broker = AgentPromptBroker::default();
        let result = broker.request_action_with_timeout(agent_prompt(), Duration::from_millis(1));
        assert_eq!(result, Err("SSH Agent prompt timed out".to_string()));
        assert!(!broker.has_pending());
    }

    #[test]
    fn credential_prompt_debug_redacts_the_response() {
        let (response_tx, _) = mpsc::channel();
        let state = CredentialPromptState {
            id: "credential-1".to_string(),
            prompt: SshCredentialPrompt {
                host: "example.com".to_string(),
                port: 22,
                username: "alice".to_string(),
                connection_name: "Example".to_string(),
                kind: SshCredentialPromptKind::Password,
                reason: SshCredentialPromptReason::MissingPassword,
                attempt: 1,
                prompt_text: None,
                echo: false,
            },
            response_tx,
            value: "credential-secret".to_string(),
        };

        let debug = format!("{state:?}");
        assert!(!debug.contains("credential-secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn keyboard_interactive_debug_redacts_responses_and_otp_codes() {
        let (response_tx, _) = mpsc::channel();
        let state = KeyboardInteractivePromptState {
            id: "interactive-1".to_string(),
            request: SshKeyboardInteractiveRequest {
                host: "example.com".to_string(),
                port: 22,
                username: "alice".to_string(),
                connection_name: "Example".to_string(),
                name: "Verification".to_string(),
                instructions: String::new(),
                round: 1,
                prompts: vec![SshKeyboardInteractivePrompt {
                    prompt: "Code:".to_string(),
                    echo: false,
                }],
                otp_id: None,
            },
            response_tx,
            responses: vec!["interactive-secret".to_string()],
            focused_index: 0,
            otp_code: Some("otp-secret".to_string()),
            otp_type: Some("totp".to_string()),
            otp_period: 30,
            otp_time_step: Some(42),
            otp_error: None,
        };

        let debug = format!("{state:?}");
        assert!(!debug.contains("interactive-secret"));
        assert!(!debug.contains("otp-secret"));
        assert!(debug.contains("response_count: 1"));
    }

    #[test]
    fn native_otp_provider_debug_redacts_used_codes() {
        let config_dir = std::env::temp_dir().join(format!(
            "nyaterm-otp-debug-test-{}-{}",
            std::process::id(),
            nyaterm_core::uuid()
        ));
        let store = StoreRuntime::spawn(StoreConfig {
            config_dir,
            portable_key_path: None,
        })
        .expect("spawn test store")
        .blocking_client();
        let provider = NativeOtpProvider::new(store);
        provider
            .used_totp_codes
            .lock()
            .expect("lock used codes")
            .insert(
                "otp-1".to_string(),
                TotpUseRecord {
                    code: "123456".to_string(),
                    time_step: 42,
                },
            );

        let debug = format!("{provider:?}");
        assert!(debug.contains("used_code_count: 1"));
        assert!(!debug.contains("123456"));
        assert!(!debug.contains("otp-1"));
    }
}
