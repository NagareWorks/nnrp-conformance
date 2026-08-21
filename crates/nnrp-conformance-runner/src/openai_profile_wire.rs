use std::time::Instant;

use anyhow::{Context, Result, bail};
use nnrp_conformance::wire_endpoint::{ReferenceTransport, WireReferenceEndpoint};
use nnrp_conformance::wire_external::{
    WireExternalCaseReport, WireExternalDirection, WireExternalFrame, WireExternalMode,
    WireExternalObservedFrame, WireExternalTerminal,
};
use nnrp_core::{
    BODY_REGION_PRELUDE_LEN, BodyRegionPrelude, MessageType, OperationState, PartialResultMetadata,
    PayloadKind, PayloadKindBitmap, ResultClass, ResultPushMetadata, TypedPayloadDescriptor,
    TypedPayloadRegion,
};
use nnrp_runtime::{
    NnrpClientRoleEvent, NnrpRuntimeEvent, NnrpRuntimeEventMetadata, NnrpRuntimeEventTail,
    NnrpServer, NnrpServerEvent, NnrpSubmitHeaderContext, NnrpSubmitIdentity, NnrpSubmitPolicy,
    NnrpSubmitRequest, NnrpTerminalEvent, NnrpTypedPayloadInputFrame, NnrpTypedPayloadSubmitInput,
};
use serde_json::{Value, json};

pub const SCENARIO_ID: &str = "wire.profile.openai-compatible.level1";
pub const CAPABILITY: &str = "profile.openai-compatible.level1.wire";

const OPERATION_ID: u64 = 901;
const FRAME_ID: u32 = 1;
const OPENAI_SCHEMA_VERSION: &str = "openai-compatible/1";
const STREAM_SEMANTICS_SNAPSHOT: u16 = 1;
const REQUEST_BODY: &[u8] = br#"{"schema_version":"openai-compatible/1","operation":"chat.completions.create","request_id":"req_wire_1","body":{"model":"reference-model","messages":[{"role":"user","content":"ping"}],"stream":true}}"#;
const PARTIAL_BODY: &[u8] = br#"{"type":"response.output_text.delta","index":0,"delta":"pong"}"#;
const TERMINAL_BODY: &[u8] = br#"{"type":"response.completed","body":{"id":"chatcmpl_wire_1","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}]}}"#;

pub async fn run_client(endpoint: &WireReferenceEndpoint) -> Result<WireExternalCaseReport> {
    endpoint.validate()?;
    if endpoint.transport != ReferenceTransport::Tcp {
        bail!("OpenAI profile wire scenario requires the TCP reference endpoint");
    }

    let started = Instant::now();
    let mut observed_frames = Vec::new();
    let mut session = endpoint.connect().await?.open_session().await?;
    let request = typed_request()?;
    session.submit_nowait(request).await?;
    observed_frames.push(observed(
        started,
        WireExternalDirection::SuiteToTarget,
        WireExternalFrame::Request,
        typed_detail("request", Some(OPENAI_SCHEMA_VERSION)),
    ));

    let partial = expect_runtime_event(session.await_event().await?)?;
    let partial_body = match (&partial.metadata, &partial.tail) {
        (NnrpRuntimeEventMetadata::PartialResult(metadata), NnrpRuntimeEventTail::Body(body))
            if partial.header.message_type == MessageType::PartialResult
                && metadata.operation_id == OPERATION_ID
                && metadata.body_bytes as usize == body.len() =>
        {
            body
        }
        _ => bail!("OpenAI profile target returned an invalid PARTIAL_RESULT"),
    };
    let partial_event = parse_json(partial_body, "PARTIAL_RESULT")?;
    require_event_type(
        &partial_event,
        "response.output_text.delta",
        "PARTIAL_RESULT",
    )?;
    observed_frames.push(observed(
        started,
        WireExternalDirection::TargetToSuite,
        WireExternalFrame::PartialResult,
        json!({
            "body_encoding": "utf-8-json-event",
            "body_framing": "raw",
            "events_per_frame": 1,
            "typed_payload_envelope": false,
            "sse_delimiters_allowed": false,
            "event_type": "response.output_text.delta",
        }),
    ));

    let result = session.await_result().await?;
    if result.operation_id != OPERATION_ID {
        bail!("OpenAI profile target returned a result for another operation");
    }
    let (metadata, body) = match &result.event {
        NnrpTerminalEvent::Runtime(NnrpRuntimeEvent {
            header,
            metadata: NnrpRuntimeEventMetadata::ResultPush(metadata),
            tail: NnrpRuntimeEventTail::Body(body),
        }) if header.message_type == MessageType::ResultPush => (metadata, body),
        _ => bail!("OpenAI profile target did not return a runtime RESULT_PUSH"),
    };
    let terminal_event = decode_profile_body(
        metadata.payload_kind_bitmap,
        metadata.payload_frame_count,
        body,
    )?;
    require_event_type(&terminal_event, "response.completed", "RESULT_PUSH")?;
    observed_frames.push(observed(
        started,
        WireExternalDirection::TargetToSuite,
        WireExternalFrame::ResultPush,
        typed_detail("response.completed", None),
    ));
    session.close().await?;

    Ok(WireExternalCaseReport {
        scenario_id: SCENARIO_ID,
        mode: WireExternalMode::SuiteAsClient,
        transport: ReferenceTransport::Tcp,
        terminal: WireExternalTerminal::Success,
        elapsed_us: started.elapsed().as_micros(),
        observed_frames,
        result_drop_reason: None,
        trace_context: None,
        cache_miss: None,
    })
}

pub async fn serve_target(server: &NnrpServer) -> Result<()> {
    let mut session = server.accept().await?;
    let submit = session.receive_submit().await?;
    if submit.operation_id() != OPERATION_ID || submit.frame_id() != FRAME_ID {
        bail!("OpenAI profile target received an unexpected operation identity");
    }
    let request = decode_profile_body(
        submit.metadata().payload_kind_bitmap,
        submit.metadata().payload_frame_count,
        submit.body(),
    )?;
    if request.get("schema_version").and_then(Value::as_str) != Some(OPENAI_SCHEMA_VERSION) {
        bail!("OpenAI profile request has the wrong schema_version");
    }

    submit
        .send_partial_result(
            &mut session,
            PartialResultMetadata {
                operation_id: OPERATION_ID,
                result_sequence: 1,
                object_id: 0,
                delta_sequence: 0,
                body_bytes: u32::try_from(PARTIAL_BODY.len())?,
                flags: 0,
            },
            PARTIAL_BODY.to_vec(),
        )
        .await?;

    let (metadata, body) = typed_terminal_result(TERMINAL_BODY)?;
    submit.send_result(&mut session, metadata, body).await?;
    match session.await_event().await? {
        NnrpServerEvent::Lifecycle(event)
            if event.operation_id == OPERATION_ID && event.state == OperationState::Completed => {}
        event => bail!("OpenAI profile target expected completed lifecycle, got {event:?}"),
    }
    let close = session.receive_close().await?;
    session.ack_close(&close).await?;
    session.close_in_place().await?;
    Ok(())
}

fn typed_request() -> Result<NnrpSubmitRequest> {
    Ok(NnrpSubmitRequest::typed_payload(
        NnrpTypedPayloadSubmitInput {
            identity: NnrpSubmitIdentity {
                operation_id: OPERATION_ID,
                frame_id: FRAME_ID,
                header: NnrpSubmitHeaderContext::default(),
            },
            policy: NnrpSubmitPolicy::default(),
            frames: vec![NnrpTypedPayloadInputFrame {
                profile_id: 0,
                payload_kind: PayloadKind::StructuredEvent,
                descriptor_flags: 0,
                schema_id: 0,
                schema_version: 0,
                stream_semantics: STREAM_SEMANTICS_SNAPSHOT,
                payload: REQUEST_BODY.to_vec(),
            }],
        },
    )?)
}

fn typed_terminal_result(payload: &[u8]) -> Result<(ResultPushMetadata, Vec<u8>)> {
    let descriptor = TypedPayloadDescriptor {
        profile_id: 0,
        payload_kind: PayloadKind::StructuredEvent,
        descriptor_flags: 0,
        schema_id: 0,
        schema_version: 0,
        stream_semantics: STREAM_SEMANTICS_SNAPSHOT,
        offset: 0,
        length: u32::try_from(payload.len())?,
    };
    let descriptor_bytes = descriptor.to_bytes()?;
    let prelude = BodyRegionPrelude {
        inline_object_bytes: 0,
        object_reference_bytes: 0,
        typed_payload_descriptor_bytes: u32::try_from(descriptor_bytes.len())?,
        typed_payload_frame_bytes: u32::try_from(payload.len())?,
        extension_descriptor_bytes: 0,
        extension_payload_bytes: 0,
    };
    let mut body =
        Vec::with_capacity(BODY_REGION_PRELUDE_LEN + descriptor_bytes.len() + payload.len());
    body.extend_from_slice(&prelude.to_bytes()?);
    body.extend_from_slice(&descriptor_bytes);
    body.extend_from_slice(payload);
    let metadata = ResultPushMetadata {
        status_code: 200,
        result_flags: 0,
        section_count: 0,
        tile_count: 0,
        active_profile_id: 0,
        inference_ms: 0,
        queue_ms: 0,
        server_total_ms: 0,
        tile_base_id: 0,
        tile_index_bytes: 0,
        result_class: ResultClass::Complete,
        applied_budget_policy: 0,
        reused_frame_id: 0,
        covered_tile_count: 0,
        dropped_tile_count: 0,
        payload_kind_bitmap: PayloadKindBitmap(PayloadKindBitmap::STRUCTURED_EVENT),
        payload_frame_count: 1,
    };
    metadata.to_bytes()?;
    Ok((metadata, body))
}

fn decode_profile_body(
    payload_kind_bitmap: PayloadKindBitmap,
    payload_frame_count: u16,
    body: &[u8],
) -> Result<Value> {
    if payload_kind_bitmap != PayloadKindBitmap(PayloadKindBitmap::STRUCTURED_EVENT)
        || payload_frame_count != 1
    {
        bail!("OpenAI profile body must declare exactly one STRUCTURED_EVENT frame");
    }
    if body.len() < BODY_REGION_PRELUDE_LEN {
        bail!("OpenAI profile body is shorter than its body-region prelude");
    }
    let prelude = BodyRegionPrelude::parse(&body[..BODY_REGION_PRELUDE_LEN])?;
    if prelude.inline_object_bytes != 0
        || prelude.object_reference_bytes != 0
        || prelude.extension_descriptor_bytes != 0
        || prelude.extension_payload_bytes != 0
    {
        bail!("OpenAI profile body must contain only typed descriptor and payload regions");
    }
    let descriptor_bytes = usize::try_from(prelude.typed_payload_descriptor_bytes)?;
    let payload_bytes = usize::try_from(prelude.typed_payload_frame_bytes)?;
    let descriptor_end = BODY_REGION_PRELUDE_LEN
        .checked_add(descriptor_bytes)
        .context("OpenAI profile descriptor length overflow")?;
    let body_end = descriptor_end
        .checked_add(payload_bytes)
        .context("OpenAI profile payload length overflow")?;
    if body_end != body.len() {
        bail!("OpenAI profile body-region lengths do not match the body length");
    }
    let region = TypedPayloadRegion::parse(
        payload_kind_bitmap,
        payload_frame_count,
        &body[BODY_REGION_PRELUDE_LEN..descriptor_end],
        &body[descriptor_end..body_end],
    )?;
    let frames = region.frame_views()?;
    let frame = frames
        .first()
        .context("OpenAI profile body did not contain its declared frame")?;
    let descriptor = frame.descriptor;
    if descriptor.profile_id != 0
        || descriptor.payload_kind != PayloadKind::StructuredEvent
        || descriptor.descriptor_flags != 0
        || descriptor.schema_id != 0
        || descriptor.schema_version != 0
        || descriptor.stream_semantics != STREAM_SEMANTICS_SNAPSHOT
    {
        bail!("OpenAI profile typed descriptor does not match the frozen mapping");
    }
    parse_json(frame.payload, "typed profile body")
}

fn parse_json(payload: &[u8], context: &str) -> Result<Value> {
    let text =
        std::str::from_utf8(payload).with_context(|| format!("{context} is not valid UTF-8"))?;
    serde_json::from_str(text).with_context(|| format!("{context} is not valid JSON"))
}

fn require_event_type(value: &Value, expected: &str, context: &str) -> Result<()> {
    if value.get("type").and_then(Value::as_str) != Some(expected) {
        bail!("{context} does not carry event type {expected}");
    }
    Ok(())
}

fn expect_runtime_event(event: NnrpClientRoleEvent) -> Result<NnrpRuntimeEvent> {
    match event {
        NnrpClientRoleEvent::Runtime(event) => Ok(event),
        NnrpClientRoleEvent::Lifecycle(event) => {
            bail!("expected an OpenAI profile runtime event, got lifecycle {event:?}")
        }
    }
}

fn typed_detail(event_type: &str, envelope_schema_version: Option<&str>) -> Value {
    let mut detail = json!({
        "payload_frame_count": 1,
        "payload_kind": "structured_event",
        "payload_kind_bitmap": PayloadKindBitmap::STRUCTURED_EVENT,
        "profile_id": 0,
        "schema_id": 0,
        "schema_version": 0,
        "stream_semantics": "snapshot",
        "stream_semantics_value": STREAM_SEMANTICS_SNAPSHOT,
        "payload_encoding": "utf-8-json",
        "event_type": event_type,
    });
    if let Some(schema_version) = envelope_schema_version {
        detail["envelope_schema_version"] = json!(schema_version);
    }
    detail
}

fn observed(
    started: Instant,
    direction: WireExternalDirection,
    frame: WireExternalFrame,
    detail: Value,
) -> WireExternalObservedFrame {
    WireExternalObservedFrame {
        direction,
        frame,
        timestamp_us: started.elapsed().as_micros(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OPENAI_SCHEMA_VERSION, REQUEST_BODY, TERMINAL_BODY, decode_profile_body, typed_request,
        typed_terminal_result,
    };
    use nnrp_core::{PayloadKindBitmap, TYPED_PAYLOAD_DESCRIPTOR_LEN};

    #[test]
    fn request_uses_the_frozen_single_frame_mapping() {
        let request = typed_request().expect("request should encode");
        let decoded = decode_profile_body(
            request.metadata.payload_kind_bitmap,
            request.metadata.payload_frame_count,
            &request.body,
        )
        .expect("request should decode");
        assert_eq!(
            decoded
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some(OPENAI_SCHEMA_VERSION)
        );
        assert_eq!(
            request.body.len(),
            32 + TYPED_PAYLOAD_DESCRIPTOR_LEN + REQUEST_BODY.len()
        );
    }

    #[test]
    fn terminal_uses_the_same_typed_mapping() {
        let (metadata, body) =
            typed_terminal_result(TERMINAL_BODY).expect("terminal should encode");
        let decoded = decode_profile_body(
            metadata.payload_kind_bitmap,
            metadata.payload_frame_count,
            &body,
        )
        .expect("terminal should decode");
        assert_eq!(
            decoded.get("type").and_then(serde_json::Value::as_str),
            Some("response.completed")
        );
    }

    #[test]
    fn raw_json_cannot_masquerade_as_a_typed_terminal_body() {
        let error = decode_profile_body(
            PayloadKindBitmap(PayloadKindBitmap::STRUCTURED_EVENT),
            1,
            TERMINAL_BODY,
        )
        .expect_err("raw JSON must be rejected");
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn wrong_payload_metadata_is_rejected() {
        let (_, body) = typed_terminal_result(TERMINAL_BODY).expect("terminal should encode");
        let error =
            decode_profile_body(PayloadKindBitmap(PayloadKindBitmap::TOKEN_CHUNK), 1, &body)
                .expect_err("wrong payload kind must be rejected");
        assert!(error.to_string().contains("STRUCTURED_EVENT"));
    }

    #[test]
    fn invalid_json_is_rejected() {
        let (metadata, body) = typed_terminal_result(b"not-json").expect("body should encode");
        let error = decode_profile_body(
            metadata.payload_kind_bitmap,
            metadata.payload_frame_count,
            &body,
        )
        .expect_err("invalid JSON must be rejected");
        assert!(error.to_string().contains("valid JSON"));
    }
}
