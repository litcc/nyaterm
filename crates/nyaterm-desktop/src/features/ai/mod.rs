//! AI chat, agent loop, background jobs and AI settings runtime.

use std::time::Duration;

mod ai_agent_runtime;
mod ai_jobs;
mod ai_runtime;
mod panel;
mod state;

pub(in crate::features) use ai_jobs::{ai_active_profile_drafts, is_agent_command_card};
pub(in crate::features) use panel::AiPanel;
pub(in crate::features) use state::{
    AiFeatureFocus, AiFeatureInit, AiFeatureState, AiSettingsMutation,
};

const AGENT_OBSERVATION_MIN_WAIT: Duration = Duration::from_millis(700);
const AGENT_OBSERVATION_QUIET: Duration = Duration::from_millis(900);
/// How often to look for the terminal having fallen quiet, while an agent loop is
/// running. Comfortably finer than `AGENT_OBSERVATION_QUIET`, so the 900ms threshold
/// is what decides when the loop advances rather than this interval.
const AGENT_OBSERVATION_POLL_INTERVAL: Duration = Duration::from_millis(150);
const AGENT_DEFAULT_STEP_TIMEOUT: Duration = Duration::from_millis(30_000);
