use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use futures::channel::mpsc::UnboundedSender;

use crate::http::ai::{complete_native_chat, stream_native_chat};
use nyaterm_core::{
    AgentApprovalDecision, AiChatRequest, AiChatStreamDelta, AiCommandCard, AiMessage,
    AiMessageRole, AiMode, AiSettings, CommandObservation, agent_response_action,
    assess_agent_command_risk, decide_agent_command_execution, now_rfc3339,
    parse_agent_model_output, parse_agent_tool_call, parse_model_output, redact_context,
    redact_sensitive_text, truncate_preview, uuid,
};
use nyaterm_store::{ConnectionStore, StoreBlockingClient, StoreDomain};
use nyaterm_transport::RemoteCommandOutput;

use crate::features::{runtime_jobs::AiChatJobOutput, runtime_jobs::AiChatWorkerEvent};

pub(in crate::features) fn is_agent_command_card(card: &AiCommandCard) -> bool {
    card.id.starts_with("agent-")
        || card
            .category
            .as_deref()
            .is_some_and(|category| category == "AI Agent")
}

pub(in crate::features) fn run_ai_ask_job(
    store: StoreBlockingClient,
    settings: AiSettings,
    mut request: AiChatRequest,
    stream_tx: Option<UnboundedSender<AiChatWorkerEvent>>,
    cancel: Arc<AtomicBool>,
    job_id: u64,
) -> Result<AiChatJobOutput, String> {
    if ai_job_cancelled(&cancel) {
        return Err("AI request cancelled".to_string());
    }
    if settings.redaction_enabled {
        redact_context(&mut request.context);
        request.user_input = redact_sensitive_text(&request.user_input);
    }
    let session_id = request
        .session_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("ai-session-{}", uuid()));
    request.session_id = Some(session_id.clone());

    let history = store
        .request_fn(StoreDomain::Ai, |database| database.load_ai_history())
        .map_err(|error| error.to_string())?;
    if settings.record_history {
        let user_session_id = session_id.clone();
        let connection_id = request.connection_id.clone();
        let user_input = request.user_input.clone();
        store
            .request_fn(StoreDomain::Ai, move |database| {
                database.append_ai_user_message(&user_session_id, connection_id, user_input)
            })
            .map_err(|error| error.to_string())?;
    }
    if ai_job_cancelled(&cancel) {
        return Err("AI request cancelled".to_string());
    }

    let completion = if matches!(request.mode, AiMode::Ask | AiMode::Agent) {
        let delta_session_id = session_id.clone();
        let stream_cancel = cancel.clone();
        let stream_mode = request.mode.clone();
        stream_native_chat(&settings, &request, &history.messages, |delta| {
            if ai_job_cancelled(&stream_cancel) {
                return;
            }
            if delta.done {
                return;
            }
            if let Some(tx) = stream_tx.as_ref() {
                let AiChatStreamDelta {
                    text_delta,
                    reasoning_delta,
                    tool_call_deltas,
                    done: _,
                } = delta;
                if !text_delta.is_empty() || reasoning_delta.is_some() {
                    let _ = tx.unbounded_send(AiChatWorkerEvent::Delta {
                        job_id,
                        session_id: delta_session_id.clone(),
                        text_delta,
                        reasoning_delta,
                    });
                }
                if stream_mode == AiMode::Agent {
                    for tool_delta in tool_call_deltas {
                        let _ = tx.unbounded_send(AiChatWorkerEvent::AgentToolCallDelta {
                            job_id,
                            session_id: delta_session_id.clone(),
                            tool_name: tool_delta.name_delta,
                            arguments_delta_len: tool_delta.arguments_delta.len(),
                        });
                    }
                }
            }
        })?
    } else {
        if ai_job_cancelled(&cancel) {
            return Err("AI request cancelled".to_string());
        }
        complete_native_chat(&settings, &request, &history.messages)?
    };
    if ai_job_cancelled(&cancel) {
        return Err("AI request cancelled".to_string());
    }
    let output = if request.mode == AiMode::Agent {
        ai_agent_job_output(
            &settings,
            &request,
            completion.text,
            completion.reasoning_content,
            completion.tool_calls,
        )?
    } else {
        let (text, reasoning, command_cards) =
            parse_model_output(&completion.text, completion.reasoning_content);
        AiChatJobOutput {
            mode: AiMode::Ask,
            text,
            reasoning,
            command_cards,
            auto_execute_first: false,
            approval_note: None,
        }
    };
    if settings.record_history {
        let message = AiMessage {
            id: format!("msg-{}", uuid()),
            session_id,
            role: AiMessageRole::Assistant,
            content: output.text.clone(),
            created_at: now_rfc3339(),
            reasoning_content: output.reasoning.clone(),
            command_cards: output.command_cards.clone(),
        };
        store
            .request_fn(StoreDomain::Ai, move |database| {
                database.append_ai_message(message)
            })
            .map_err(|error| error.to_string())?;
    }

    Ok(output)
}

pub(in crate::features) fn ai_job_cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::Relaxed)
}

pub(in crate::features) fn remote_command_observation(
    output: RemoteCommandOutput,
    started: Instant,
) -> CommandObservation {
    CommandObservation {
        output: merge_command_output(&output.stdout, &output.stderr),
        exit_code: output
            .exit_status
            .and_then(|status| i32::try_from(status).ok()),
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    }
}

pub(in crate::features) fn observation_summary(observation: &CommandObservation) -> String {
    let status = observation
        .exit_code
        .map(|code| format!("exit {code}"))
        .unwrap_or_else(|| "exit unknown".to_string());
    let preview = truncate_preview(observation.output.trim(), 100);
    if preview.is_empty() {
        format!("{status}; {} ms; no output", observation.duration_ms)
    } else {
        format!("{status}; {} ms; {preview}", observation.duration_ms)
    }
}

fn merge_command_output(stdout: &str, stderr: &str) -> String {
    let stdout = stdout.trim_end();
    let stderr = stderr.trim_end();
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("{stdout}\n{stderr}"),
    }
}

fn ai_agent_job_output(
    settings: &AiSettings,
    request: &AiChatRequest,
    text: String,
    reasoning: Option<String>,
    tool_calls: Vec<nyaterm_core::AiToolCall>,
) -> Result<AiChatJobOutput, String> {
    let parsed = if tool_calls.is_empty() {
        parse_agent_model_output(&text).map_err(|error| error.to_string())?
    } else {
        parse_agent_tool_call(&tool_calls)
            .or_else(|_| parse_agent_model_output(&text))
            .map_err(|error| error.to_string())?
    };
    let action = agent_response_action(&parsed);
    if action == "final_answer" {
        let answer = parsed
            .answer
            .as_deref()
            .map(str::trim)
            .filter(|answer| !answer.is_empty())
            .unwrap_or("Agent finished without a final answer")
            .to_string();
        return Ok(AiChatJobOutput {
            mode: AiMode::Agent,
            text: answer,
            reasoning: Some(parsed.thought).or(reasoning),
            command_cards: Vec::new(),
            auto_execute_first: false,
            approval_note: None,
        });
    }

    let command = parsed
        .command
        .as_deref()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "Agent returned execute_command without a command".to_string())?;
    let assessment = assess_agent_command_risk(&parsed, command);
    let (decision, approval_note) = decide_agent_command_execution(settings, &assessment);
    let explanation = if parsed.thought.trim().is_empty() {
        "Agent requested command execution".to_string()
    } else {
        parsed.thought.trim().to_string()
    };
    let card = AiCommandCard {
        id: format!("agent-{}", uuid()),
        title: "Agent Command".to_string(),
        command: command.to_string(),
        explanation,
        risk_level: Some(assessment.effective_risk),
        risk_reason: assessment.risk_reason,
        expected_effect: "Run the next native Agent step in the active terminal".to_string(),
        rollback: Some("Review terminal output before running additional Agent steps".to_string()),
        category: Some("AI Agent".to_string()),
        references: request
            .terminal_session_id
            .as_ref()
            .map(|id| vec![format!("terminal:{id}")])
            .unwrap_or_default(),
    };
    let auto_execute_first = decision == AgentApprovalDecision::Auto;
    let approval_note = if auto_execute_first {
        Some("agent policy allows automatic execution".to_string())
    } else {
        approval_note
    };
    let text = approval_note
        .as_deref()
        .map(|note| format!("Agent proposed `{}`; {note}", card.command))
        .unwrap_or_else(|| format!("Agent proposed `{}`", card.command));

    Ok(AiChatJobOutput {
        mode: AiMode::Agent,
        text,
        reasoning: Some(parsed.thought).or(reasoning),
        command_cards: vec![card],
        auto_execute_first,
        approval_note,
    })
}

pub(in crate::features) fn ai_active_profile_drafts(settings: &AiSettings) -> (String, String) {
    settings
        .provider_profiles
        .iter()
        .find(|profile| profile.id == settings.active_profile_id)
        .map(|profile| {
            (
                profile.model.clone(),
                profile.base_url.clone().unwrap_or_default(),
            )
        })
        .unwrap_or_default()
}

pub(in crate::features) fn ai_usage_counts(store: &ConnectionStore) -> (usize, usize, usize) {
    let history = store.load_ai_history().unwrap_or_default();
    let audit_count = store
        .list_ai_audit_logs(None)
        .map(|logs| logs.len())
        .unwrap_or_default();
    (history.sessions.len(), history.messages.len(), audit_count)
}
