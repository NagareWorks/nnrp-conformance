use crate::FixtureError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub protocol_version: String,
    #[serde(default)]
    pub generator: Option<String>,
    #[serde(default)]
    pub generated_from: Option<String>,
    pub vectors: Vec<GoldenVector>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenVector {
    pub name: String,
    pub kind: String,
    pub hex: String,
    pub bytes: usize,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticVectorManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub vectors: Vec<SemanticVectorRecipe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "recipe_type", rename_all = "snake_case")]
pub enum SemanticVectorRecipe {
    Header {
        name: String,
        description: String,
        version_major: u8,
        wire_format: u8,
        message_type: MessageTypeName,
        flags: Vec<HeaderFlagName>,
        meta_len: u32,
        body_len: u32,
        session_id: u32,
        frame_id: u32,
        view_id: u16,
        route_id: u16,
        trace_id: u64,
    },
    ClientHelloMetadata {
        name: String,
        description: String,
        min_version_major: u8,
        max_version_major: u8,
        supported_wire_format_bitmap: u16,
        supported_profile_bitmap: u32,
        supported_payload_kind_bitmap: Vec<PayloadKindName>,
        supported_codec_bitmap: u32,
        supported_compression_bitmap: u32,
        supported_dtype_bitmap: u32,
        supported_layout_bitmap: u32,
        cache_digest_bitmap: u16,
        cache_object_bitmap: u16,
        cache_namespace_count: u16,
        max_lane_count: u16,
        max_cache_entries: u32,
        max_cache_bytes: u32,
        target_cadence_x100: u16,
        latency_budget_ms: u16,
        quality_tier: u16,
        degrade_policy: u16,
        requested_session_id: u32,
        auth_bytes: u32,
        control_extension_bytes: u32,
    },
    SessionPatchAckMetadata {
        name: String,
        description: String,
        ack_status: SessionPatchAckStatusName,
        reject_reason: SessionPatchRejectReasonName,
        applied_patch_mask: Vec<SessionPatchFieldName>,
        rejected_patch_mask: Vec<SessionPatchFieldName>,
        retry_after_ms: u32,
        effective_profile_id: u16,
        effective_target_cadence_x100: u32,
        effective_quality_tier: u16,
        effective_degrade_policy: u16,
        effective_lane_mask: u64,
        effective_codec_bitmap: u32,
        effective_compression_bitmap: u32,
        profile_patch_ack_bytes: u32,
        reserved0: u16,
    },
    FlowUpdatePacket {
        name: String,
        description: String,
        version_major: u8,
        wire_format: u8,
        flags: Vec<HeaderFlagName>,
        session_id: u32,
        route_id: u16,
        trace_id: u64,
        scope_kind: FlowUpdateScopeKindName,
        update_reason: FlowUpdateReasonName,
        backpressure_level: FlowUpdateBackpressureLevelName,
        connection_credit: u16,
        session_credit: u16,
        operation_credit: u16,
        operation_id: u64,
        retry_after_ms: u32,
        credit_epoch: u32,
        flow_update_flags: Vec<FlowUpdateFlagName>,
    },
    SessionOpenMetadata {
        name: String,
        description: String,
        requested_session_id: u32,
        profile_id: u16,
        priority_class: SessionPriorityClassName,
        session_flags: u8,
        schema_id: u32,
        schema_version: u32,
        default_deadline_ms: u32,
        max_in_flight_operations: u16,
        lease_ttl_hint_ms: u32,
        resume_token_bytes: u32,
        auth_bytes: u32,
        session_extension_bytes: u32,
        client_session_tag: u64,
    },
    SessionOpenAckMetadata {
        name: String,
        description: String,
        session_id: u32,
        accepted_profile_id: u16,
        accepted_priority_class: SessionPriorityClassName,
        session_status: SessionStatusName,
        schema_id: u32,
        schema_version: u32,
        granted_operation_credit: u16,
        max_in_flight_operations: u16,
        lease_ttl_ms: u32,
        resume_window_ms: u32,
        resume_token_bytes: u32,
        session_extension_bytes: u32,
        server_session_tag: u64,
        route_scope_id: u32,
        session_error_code: u32,
        session_flags_ack: u32,
    },
    SessionCloseMetadata {
        name: String,
        description: String,
        close_reason: SessionCloseReasonName,
        in_flight_policy: InFlightPolicyName,
        drain_timeout_ms: u32,
        last_operation_id: u64,
        session_error_code: u32,
        session_close_tag: u32,
    },
    SessionCloseAckMetadata {
        name: String,
        description: String,
        close_status: SessionCloseStatusName,
        last_operation_id: u64,
        session_error_code: u32,
    },
    SchemaDescriptorHeader {
        name: String,
        description: String,
        schema_id: u32,
        schema_version: u32,
        profile_id: u16,
        schema_flags: u16,
        min_version_major: u8,
        max_version_major: u8,
        body_bytes: u32,
        dependency_count: u16,
        default_stream_semantics: u16,
        schema_hash: u64,
    },
    U32Value {
        name: String,
        description: String,
        kind: String,
        value: u32,
    },
    TypedPayloadDescriptorV3 {
        name: String,
        description: String,
        profile_id: u16,
        descriptor_flags: u16,
        schema_id: u32,
        schema_version: u32,
        stream_semantics: u16,
        offset: u32,
        length: u32,
    },
    ResultHintPacket {
        name: String,
        description: String,
        version_major: u8,
        wire_format: u8,
        flags: Vec<HeaderFlagName>,
        session_id: u32,
        frame_id: u32,
        route_id: u16,
        trace_id: u64,
        applied_budget_policy: ResultHintBudgetPolicyName,
        congestion_state: ResultHintCongestionStateName,
        reason: ResultHintReasonName,
        retry_after_ms: u32,
    },
    FrameSubmitMetadata {
        name: String,
        description: String,
        src_width: u16,
        src_height: u16,
        tile_width: u16,
        tile_height: u16,
        tile_count: u16,
        section_count: u16,
        frame_class: u8,
        input_profile: InputProfileName,
        tile_index_mode: TileIndexModeName,
        reserved0: u8,
        latency_budget_ms: u16,
        target_fps_x100: u16,
        retry_of_frame: u32,
        tile_base_id: u32,
        camera_bytes: u32,
        tile_index_bytes: u32,
        submit_mode: SubmitModeName,
        budget_policy: Vec<BudgetPolicyName>,
        loss_tolerance_policy: LossTolerancePolicyName,
        object_ref_mask: u32,
        dependency_frame_id: u32,
        payload_kind_bitmap: Vec<PayloadKindName>,
        payload_frame_count: u16,
    },
    ResultPushMetadata {
        name: String,
        description: String,
        status_code: u16,
        result_flags: Vec<ResultFlagName>,
        section_count: u16,
        tile_count: u16,
        active_profile_id: u16,
        reserved0: u16,
        inference_ms: u16,
        queue_ms: u16,
        server_total_ms: u16,
        reserved1: u16,
        tile_base_id: u32,
        tile_index_bytes: u32,
        result_class: ResultClassName,
        applied_budget_policy: Vec<BudgetPolicyName>,
        reused_frame_id: u32,
        covered_tile_count: u16,
        dropped_tile_count: u16,
        payload_kind_bitmap: Vec<PayloadKindName>,
        payload_frame_count: u16,
    },
    BodyRegionPrelude {
        name: String,
        description: String,
        inline_object_bytes: u32,
        object_reference_bytes: u32,
        typed_payload_descriptor_bytes: u32,
        typed_payload_frame_bytes: u32,
        extension_descriptor_bytes: u32,
        extension_payload_bytes: u32,
    },
    ObjectReferenceBlock {
        name: String,
        description: String,
        object_kind: CacheObjectKindName,
        ref_flags: u16,
        cache_namespace: u32,
        cache_key_hi: u32,
        cache_key_lo: u32,
    },
    TypedPayloadDescriptor {
        name: String,
        description: String,
        payload_kind: PayloadKindName,
        descriptor_flags: u8,
        profile_id: u16,
        payload_offset: u32,
        payload_length: u32,
    },
    TypedPayloadDescriptorRegion {
        name: String,
        description: String,
        frames: Vec<TypedPayloadFrameRecipe>,
    },
    TypedPayloadFrameRegion {
        name: String,
        description: String,
        frames: Vec<TypedPayloadFrameRecipe>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedPayloadFrameRecipe {
    pub payload_kind: PayloadKindName,
    pub profile_id: u16,
    pub payload_utf8: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTypeName {
    ClientHello,
    SessionPatchAck,
    FlowUpdate,
    ResultHint,
    FrameSubmit,
    ResultPush,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderFlagName {
    AckRequired,
    CanDrop,
    Stale,
    Eos,
    Retransmit,
    Keyframe,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKindName {
    Tensor,
    TokenChunk,
    AudioChunk,
    VideoChunk,
    StructuredEvent,
    ToolDelta,
    OpaqueBytes,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPatchFieldName {
    TargetCadence,
    QualityTier,
    DegradePolicy,
    ActiveLaneMask,
    PreferredCodec,
    PreferredCompression,
    ProfilePatch,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPatchAckStatusName {
    Accepted,
    PartiallyApplied,
    Rejected,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPatchRejectReasonName {
    None,
    UnsupportedField,
    InvalidRange,
    UnsupportedStrategy,
    InvalidLaneMask,
    RateLimited,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowUpdateScopeKindName {
    Connection,
    Session,
    Operation,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowUpdateReasonName {
    Grant,
    Reduce,
    Pause,
    Resume,
    Congestion,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowUpdateBackpressureLevelName {
    None,
    Soft,
    Hard,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowUpdateFlagName {
    CreditValid,
    RetryAfterValid,
    BackgroundOnly,
    DrainInFlightOnly,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPriorityClassName {
    Interactive,
    Balanced,
    Background,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatusName {
    Opened,
    Rejected,
    RetryLater,
    Resumed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCloseReasonName {
    Normal,
    ClientShutdown,
    ServerShutdown,
    IdleTimeout,
    ProtocolError,
    AuthRevoked,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InFlightPolicyName {
    Drain,
    Abort,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCloseStatusName {
    Acknowledged,
    Draining,
    Closed,
    Rejected,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultHintBudgetPolicyName {
    None,
    Full,
    Partial,
    StaleReuse,
    Drop,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultHintCongestionStateName {
    None,
    Steady,
    Elevated,
    Saturated,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultHintReasonName {
    None,
    QueueFull,
    ServerBusy,
    BudgetExceeded,
    Superseded,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheObjectKindName {
    CameraBlock,
    TileIndexBlock,
    TensorSectionTable,
    CodecTable,
    ReusableResultObject,
    PayloadLayoutTemplate,
    PromptSegment,
    ToolSchema,
    StructuredEventSchema,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputProfileName {
    Unspecified,
    ChangedTilesLuma,
    DenseLumaFrame,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileIndexModeName {
    DenseRange,
    RawU16,
    DeltaU16,
    Bitset,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmitModeName {
    Inline,
    Reference,
    Mixed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPolicyName {
    #[serde(rename = "allow_partial")]
    Partial,
    #[serde(rename = "allow_stale_reuse")]
    StaleReuse,
    #[serde(rename = "allow_degraded")]
    Degraded,
    #[serde(rename = "allow_drop")]
    Drop,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossTolerancePolicyName {
    Strict,
    BestEffort,
    LowLatency,
    FireAndForget,
    InheritSession,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultFlagName {
    Stale,
    Fallback,
    Partial,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultClassName {
    Complete,
    Partial,
    StaleReuse,
    Degraded,
}

pub fn build_vector_manifest(
    semantic_manifest: &SemanticVectorManifest,
    generated_from: &str,
) -> Result<VectorManifest, FixtureError> {
    if semantic_manifest.protocol_version.trim().is_empty() {
        return Err(FixtureError::Validation {
            message: "semantic vector manifest must declare a protocol_version".to_string(),
        });
    }

    let mut vectors = Vec::with_capacity(semantic_manifest.vectors.len());
    for recipe in &semantic_manifest.vectors {
        let (name, kind, description, payload) = render_recipe(recipe)?;
        vectors.push(GoldenVector {
            name,
            kind,
            hex: encode_hex(&payload),
            bytes: payload.len(),
            description,
        });
    }

    Ok(VectorManifest {
        schema: Some("../../schemas/vector-manifest.schema.json".to_string()),
        protocol_version: semantic_manifest.protocol_version.clone(),
        generator: Some("nnrp-conformance semantic vector generator".to_string()),
        generated_from: Some(generated_from.to_string()),
        vectors,
    })
}

pub fn verify_vector_manifest(
    semantic_manifest: &SemanticVectorManifest,
    checked_in_manifest: &VectorManifest,
    generated_from: &str,
) -> Result<(), FixtureError> {
    let generated_manifest = build_vector_manifest(semantic_manifest, generated_from)?;
    if &generated_manifest != checked_in_manifest {
        return Err(FixtureError::Validation {
            message: "vector manifest does not match the semantic recipes".to_string(),
        });
    }

    Ok(())
}

fn render_recipe(
    recipe: &SemanticVectorRecipe,
) -> Result<(String, String, String, Vec<u8>), FixtureError> {
    match recipe {
        SemanticVectorRecipe::Header {
            name,
            description,
            version_major,
            wire_format,
            message_type,
            flags,
            meta_len,
            body_len,
            session_id,
            frame_id,
            view_id,
            route_id,
            trace_id,
        } => Ok((
            name.clone(),
            "header".to_string(),
            description.clone(),
            pack_header(HeaderFields {
                version_major: *version_major,
                wire_format: *wire_format,
                message_type: message_type.as_u8(),
                flags: header_flags(flags),
                meta_len: *meta_len,
                body_len: *body_len,
                session_id: *session_id,
                frame_id: *frame_id,
                view_id: *view_id,
                route_id: *route_id,
                trace_id: *trace_id,
            }),
        )),
        SemanticVectorRecipe::ClientHelloMetadata {
            name,
            description,
            min_version_major,
            max_version_major,
            supported_wire_format_bitmap,
            supported_profile_bitmap,
            supported_payload_kind_bitmap,
            supported_codec_bitmap,
            supported_compression_bitmap,
            supported_dtype_bitmap,
            supported_layout_bitmap,
            cache_digest_bitmap,
            cache_object_bitmap,
            cache_namespace_count,
            max_lane_count,
            max_cache_entries,
            max_cache_bytes,
            target_cadence_x100,
            latency_budget_ms,
            quality_tier,
            degrade_policy,
            requested_session_id,
            auth_bytes,
            control_extension_bytes,
        } => {
            let mut payload = Vec::new();
            push_u8(&mut payload, *min_version_major);
            push_u8(&mut payload, *max_version_major);
            push_u16(&mut payload, *supported_wire_format_bitmap);
            push_u32(&mut payload, *supported_profile_bitmap);
            push_u32(&mut payload, payload_kinds(supported_payload_kind_bitmap));
            push_u32(&mut payload, *supported_codec_bitmap);
            push_u32(&mut payload, *supported_compression_bitmap);
            push_u32(&mut payload, *supported_dtype_bitmap);
            push_u32(&mut payload, *supported_layout_bitmap);
            push_u16(&mut payload, *cache_digest_bitmap);
            push_u16(&mut payload, *cache_object_bitmap);
            push_u16(&mut payload, *cache_namespace_count);
            push_u16(&mut payload, *max_lane_count);
            push_u32(&mut payload, *max_cache_entries);
            push_u32(&mut payload, *max_cache_bytes);
            push_u16(&mut payload, *target_cadence_x100);
            push_u16(&mut payload, *latency_budget_ms);
            push_u16(&mut payload, *quality_tier);
            push_u16(&mut payload, *degrade_policy);
            push_u32(&mut payload, *requested_session_id);
            push_u32(&mut payload, *auth_bytes);
            push_u32(&mut payload, *control_extension_bytes);
            Ok((
                name.clone(),
                "metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::SessionPatchAckMetadata {
            name,
            description,
            ack_status,
            reject_reason,
            applied_patch_mask,
            rejected_patch_mask,
            retry_after_ms,
            effective_profile_id,
            effective_target_cadence_x100,
            effective_quality_tier,
            effective_degrade_policy,
            effective_lane_mask,
            effective_codec_bitmap,
            effective_compression_bitmap,
            profile_patch_ack_bytes,
            reserved0,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, ack_status.as_u16());
            push_u16(&mut payload, reject_reason.as_u16());
            push_u32(&mut payload, session_patch_fields(applied_patch_mask));
            push_u32(&mut payload, session_patch_fields(rejected_patch_mask));
            push_u32(&mut payload, *retry_after_ms);
            push_u16(&mut payload, *effective_profile_id);
            push_u16(&mut payload, *reserved0);
            push_u32(&mut payload, *effective_target_cadence_x100);
            push_u16(&mut payload, *effective_quality_tier);
            push_u16(&mut payload, *effective_degrade_policy);
            push_u64(&mut payload, *effective_lane_mask);
            push_u32(&mut payload, *effective_codec_bitmap);
            push_u32(&mut payload, *effective_compression_bitmap);
            push_u32(&mut payload, *profile_patch_ack_bytes);
            Ok((
                name.clone(),
                "metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::FlowUpdatePacket {
            name,
            description,
            version_major,
            wire_format,
            flags,
            session_id,
            route_id,
            trace_id,
            scope_kind,
            update_reason,
            backpressure_level,
            connection_credit,
            session_credit,
            operation_credit,
            operation_id,
            retry_after_ms,
            credit_epoch,
            flow_update_flags,
        } => {
            let mut metadata = Vec::new();
            push_u8(&mut metadata, scope_kind.as_u8());
            push_u8(&mut metadata, update_reason.as_u8());
            push_u8(&mut metadata, backpressure_level.as_u8());
            push_u8(&mut metadata, 0);
            push_u16(&mut metadata, *connection_credit);
            push_u16(&mut metadata, *session_credit);
            push_u16(&mut metadata, *operation_credit);
            push_u16(&mut metadata, 0);
            push_u64(&mut metadata, *operation_id);
            push_u32(&mut metadata, *retry_after_ms);
            push_u32(&mut metadata, *credit_epoch);
            push_u32(&mut metadata, flow_update_flags_bits(flow_update_flags));
            let packet = build_packet(
                HeaderFields {
                    version_major: *version_major,
                    wire_format: *wire_format,
                    message_type: MessageTypeName::FlowUpdate.as_u8(),
                    flags: header_flags(flags),
                    meta_len: 0,
                    body_len: 0,
                    session_id: *session_id,
                    frame_id: 0,
                    view_id: 0,
                    route_id: *route_id,
                    trace_id: *trace_id,
                },
                metadata,
                Vec::new(),
            );
            Ok((
                name.clone(),
                "flow_update_packet".to_string(),
                description.clone(),
                packet,
            ))
        }
        SemanticVectorRecipe::SessionOpenMetadata {
            name,
            description,
            requested_session_id,
            profile_id,
            priority_class,
            session_flags,
            schema_id,
            schema_version,
            default_deadline_ms,
            max_in_flight_operations,
            lease_ttl_hint_ms,
            resume_token_bytes,
            auth_bytes,
            session_extension_bytes,
            client_session_tag,
        } => {
            let mut payload = Vec::new();
            push_u32(&mut payload, *requested_session_id);
            push_u16(&mut payload, *profile_id);
            push_u8(&mut payload, priority_class.as_u8());
            push_u8(&mut payload, *session_flags);
            push_u32(&mut payload, *schema_id);
            push_u32(&mut payload, *schema_version);
            push_u32(&mut payload, *default_deadline_ms);
            push_u16(&mut payload, *max_in_flight_operations);
            push_u16(&mut payload, 0);
            push_u32(&mut payload, *lease_ttl_hint_ms);
            push_u32(&mut payload, *resume_token_bytes);
            push_u32(&mut payload, *auth_bytes);
            push_u32(&mut payload, *session_extension_bytes);
            push_u64(&mut payload, *client_session_tag);
            Ok((
                name.clone(),
                "session_open_metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::SessionOpenAckMetadata {
            name,
            description,
            session_id,
            accepted_profile_id,
            accepted_priority_class,
            session_status,
            schema_id,
            schema_version,
            granted_operation_credit,
            max_in_flight_operations,
            lease_ttl_ms,
            resume_window_ms,
            resume_token_bytes,
            session_extension_bytes,
            server_session_tag,
            route_scope_id,
            session_error_code,
            session_flags_ack,
        } => {
            let mut payload = Vec::new();
            push_u32(&mut payload, *session_id);
            push_u16(&mut payload, *accepted_profile_id);
            push_u8(&mut payload, accepted_priority_class.as_u8());
            push_u8(&mut payload, session_status.as_u8());
            push_u32(&mut payload, *schema_id);
            push_u32(&mut payload, *schema_version);
            push_u16(&mut payload, *granted_operation_credit);
            push_u16(&mut payload, *max_in_flight_operations);
            push_u32(&mut payload, *lease_ttl_ms);
            push_u32(&mut payload, *resume_window_ms);
            push_u32(&mut payload, *resume_token_bytes);
            push_u32(&mut payload, *session_extension_bytes);
            push_u64(&mut payload, *server_session_tag);
            push_u32(&mut payload, *route_scope_id);
            push_u32(&mut payload, *session_error_code);
            push_u32(&mut payload, *session_flags_ack);
            Ok((
                name.clone(),
                "session_open_ack_metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::SessionCloseMetadata {
            name,
            description,
            close_reason,
            in_flight_policy,
            drain_timeout_ms,
            last_operation_id,
            session_error_code,
            session_close_tag,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, close_reason.as_u16());
            push_u8(&mut payload, in_flight_policy.as_u8());
            push_u8(&mut payload, 0);
            push_u32(&mut payload, *drain_timeout_ms);
            push_u64(&mut payload, *last_operation_id);
            push_u32(&mut payload, *session_error_code);
            push_u32(&mut payload, *session_close_tag);
            Ok((
                name.clone(),
                "session_close_metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::SessionCloseAckMetadata {
            name,
            description,
            close_status,
            last_operation_id,
            session_error_code,
        } => {
            let mut payload = Vec::new();
            push_u8(&mut payload, close_status.as_u8());
            push_u8(&mut payload, 0);
            push_u16(&mut payload, 0);
            push_u64(&mut payload, *last_operation_id);
            push_u32(&mut payload, *session_error_code);
            Ok((
                name.clone(),
                "session_close_ack_metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::SchemaDescriptorHeader {
            name,
            description,
            schema_id,
            schema_version,
            profile_id,
            schema_flags,
            min_version_major,
            max_version_major,
            body_bytes,
            dependency_count,
            default_stream_semantics,
            schema_hash,
        } => {
            let mut payload = Vec::new();
            push_u32(&mut payload, *schema_id);
            push_u32(&mut payload, *schema_version);
            push_u16(&mut payload, *profile_id);
            push_u16(&mut payload, *schema_flags);
            push_u8(&mut payload, *min_version_major);
            push_u8(&mut payload, *max_version_major);
            push_u16(&mut payload, 0);
            push_u32(&mut payload, *body_bytes);
            push_u16(&mut payload, *dependency_count);
            push_u16(&mut payload, *default_stream_semantics);
            push_u64(&mut payload, *schema_hash);
            Ok((
                name.clone(),
                "schema_descriptor_header".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::U32Value {
            name,
            description,
            kind,
            value,
        } => {
            let mut payload = Vec::new();
            push_u32(&mut payload, *value);
            Ok((name.clone(), kind.clone(), description.clone(), payload))
        }
        SemanticVectorRecipe::TypedPayloadDescriptorV3 {
            name,
            description,
            profile_id,
            descriptor_flags,
            schema_id,
            schema_version,
            stream_semantics,
            offset,
            length,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, *profile_id);
            push_u16(&mut payload, *descriptor_flags);
            push_u32(&mut payload, *schema_id);
            push_u32(&mut payload, *schema_version);
            push_u16(&mut payload, *stream_semantics);
            push_u16(&mut payload, 0);
            push_u32(&mut payload, *offset);
            push_u32(&mut payload, *length);
            Ok((
                name.clone(),
                "typed_payload_descriptor".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::ResultHintPacket {
            name,
            description,
            version_major,
            wire_format,
            flags,
            session_id,
            frame_id,
            route_id,
            trace_id,
            applied_budget_policy,
            congestion_state,
            reason,
            retry_after_ms,
        } => {
            let mut metadata = Vec::new();
            push_u32(&mut metadata, applied_budget_policy.as_u32());
            push_u32(&mut metadata, congestion_state.as_u32());
            push_u32(&mut metadata, reason.as_u32());
            push_u32(&mut metadata, *retry_after_ms);
            let packet = build_packet(
                HeaderFields {
                    version_major: *version_major,
                    wire_format: *wire_format,
                    message_type: MessageTypeName::ResultHint.as_u8(),
                    flags: header_flags(flags),
                    meta_len: 0,
                    body_len: 0,
                    session_id: *session_id,
                    frame_id: *frame_id,
                    view_id: 0,
                    route_id: *route_id,
                    trace_id: *trace_id,
                },
                metadata,
                Vec::new(),
            );
            Ok((
                name.clone(),
                "packet".to_string(),
                description.clone(),
                packet,
            ))
        }
        SemanticVectorRecipe::FrameSubmitMetadata {
            name,
            description,
            src_width,
            src_height,
            tile_width,
            tile_height,
            tile_count,
            section_count,
            frame_class,
            input_profile,
            tile_index_mode,
            reserved0,
            latency_budget_ms,
            target_fps_x100,
            retry_of_frame,
            tile_base_id,
            camera_bytes,
            tile_index_bytes,
            submit_mode,
            budget_policy,
            loss_tolerance_policy,
            object_ref_mask,
            dependency_frame_id,
            payload_kind_bitmap,
            payload_frame_count,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, *src_width);
            push_u16(&mut payload, *src_height);
            push_u16(&mut payload, *tile_width);
            push_u16(&mut payload, *tile_height);
            push_u16(&mut payload, *tile_count);
            push_u16(&mut payload, *section_count);
            push_u8(&mut payload, *frame_class);
            push_u8(&mut payload, input_profile.as_u8());
            push_u8(&mut payload, tile_index_mode.as_u8());
            push_u8(&mut payload, *reserved0);
            push_u16(&mut payload, *latency_budget_ms);
            push_u16(&mut payload, *target_fps_x100);
            push_u32(&mut payload, *retry_of_frame);
            push_u32(&mut payload, *tile_base_id);
            push_u32(&mut payload, *camera_bytes);
            push_u32(&mut payload, *tile_index_bytes);
            push_u64(&mut payload, 0);
            push_u64(&mut payload, 0);
            push_u8(&mut payload, submit_mode.as_u8());
            push_u8(&mut payload, budget_policy_bits(budget_policy));
            push_u8(&mut payload, loss_tolerance_policy.as_u8());
            push_u8(&mut payload, 0);
            push_u32(&mut payload, *object_ref_mask);
            push_u32(&mut payload, *dependency_frame_id);
            push_u32(&mut payload, payload_kinds(payload_kind_bitmap));
            push_u16(&mut payload, *payload_frame_count);
            push_u16(&mut payload, 0);
            Ok((
                name.clone(),
                "metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::ResultPushMetadata {
            name,
            description,
            status_code,
            result_flags,
            section_count,
            tile_count,
            active_profile_id,
            reserved0,
            inference_ms,
            queue_ms,
            server_total_ms,
            reserved1,
            tile_base_id,
            tile_index_bytes,
            result_class,
            applied_budget_policy,
            reused_frame_id,
            covered_tile_count,
            dropped_tile_count,
            payload_kind_bitmap,
            payload_frame_count,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, *status_code);
            push_u16(&mut payload, result_flags_bits(result_flags));
            push_u16(&mut payload, *section_count);
            push_u16(&mut payload, *tile_count);
            push_u16(&mut payload, *active_profile_id);
            push_u16(&mut payload, *reserved0);
            push_u16(&mut payload, *inference_ms);
            push_u16(&mut payload, *queue_ms);
            push_u16(&mut payload, *server_total_ms);
            push_u16(&mut payload, *reserved1);
            push_u32(&mut payload, *tile_base_id);
            push_u32(&mut payload, *tile_index_bytes);
            push_u64(&mut payload, 0);
            push_u64(&mut payload, 0);
            push_u8(&mut payload, result_class.as_u8());
            push_u8(&mut payload, budget_policy_bits(applied_budget_policy));
            push_u16(&mut payload, 0);
            push_u32(&mut payload, *reused_frame_id);
            push_u16(&mut payload, *covered_tile_count);
            push_u16(&mut payload, *dropped_tile_count);
            push_u32(&mut payload, payload_kinds(payload_kind_bitmap));
            push_u16(&mut payload, *payload_frame_count);
            push_u16(&mut payload, 0);
            Ok((
                name.clone(),
                "metadata".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::BodyRegionPrelude {
            name,
            description,
            inline_object_bytes,
            object_reference_bytes,
            typed_payload_descriptor_bytes,
            typed_payload_frame_bytes,
            extension_descriptor_bytes,
            extension_payload_bytes,
        } => {
            let mut payload = Vec::new();
            push_u32(&mut payload, *inline_object_bytes);
            push_u32(&mut payload, *object_reference_bytes);
            push_u32(&mut payload, *typed_payload_descriptor_bytes);
            push_u32(&mut payload, *typed_payload_frame_bytes);
            push_u32(&mut payload, *extension_descriptor_bytes);
            push_u32(&mut payload, *extension_payload_bytes);
            push_u32(&mut payload, 0);
            push_u32(&mut payload, 0);
            Ok((
                name.clone(),
                "body_region".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::ObjectReferenceBlock {
            name,
            description,
            object_kind,
            ref_flags,
            cache_namespace,
            cache_key_hi,
            cache_key_lo,
        } => {
            let mut payload = Vec::new();
            push_u16(&mut payload, object_kind.as_u16());
            push_u16(&mut payload, *ref_flags);
            push_u32(&mut payload, *cache_namespace);
            push_u32(&mut payload, *cache_key_hi);
            push_u32(&mut payload, *cache_key_lo);
            Ok((
                name.clone(),
                "object_reference".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::TypedPayloadDescriptor {
            name,
            description,
            payload_kind,
            descriptor_flags,
            profile_id,
            payload_offset,
            payload_length,
        } => {
            let mut payload = Vec::new();
            push_u8(&mut payload, payload_kind.as_u8());
            push_u8(&mut payload, *descriptor_flags);
            push_u16(&mut payload, *profile_id);
            push_u32(&mut payload, *payload_offset);
            push_u32(&mut payload, *payload_length);
            push_u32(&mut payload, 0);
            Ok((
                name.clone(),
                "typed_payload_descriptor".to_string(),
                description.clone(),
                payload,
            ))
        }
        SemanticVectorRecipe::TypedPayloadDescriptorRegion {
            name,
            description,
            frames,
        } => Ok((
            name.clone(),
            "typed_payload_descriptor_region".to_string(),
            description.clone(),
            pack_typed_payload_descriptor_region(frames),
        )),
        SemanticVectorRecipe::TypedPayloadFrameRegion {
            name,
            description,
            frames,
        } => Ok((
            name.clone(),
            "typed_payload_frame_region".to_string(),
            description.clone(),
            pack_typed_payload_frame_region(frames),
        )),
    }
}

#[derive(Debug, Copy, Clone)]
struct HeaderFields {
    version_major: u8,
    wire_format: u8,
    message_type: u8,
    flags: u32,
    meta_len: u32,
    body_len: u32,
    session_id: u32,
    frame_id: u32,
    view_id: u16,
    route_id: u16,
    trace_id: u64,
}

fn pack_header(fields: HeaderFields) -> Vec<u8> {
    let mut payload = Vec::with_capacity(40);
    payload.extend_from_slice(b"NNRP");
    push_u8(&mut payload, fields.version_major);
    push_u8(&mut payload, fields.wire_format);
    push_u8(&mut payload, fields.message_type);
    push_u8(&mut payload, 40);
    push_u32(&mut payload, fields.flags);
    push_u32(&mut payload, fields.meta_len);
    push_u32(&mut payload, fields.body_len);
    push_u32(&mut payload, fields.session_id);
    push_u32(&mut payload, fields.frame_id);
    push_u16(&mut payload, fields.view_id);
    push_u16(&mut payload, fields.route_id);
    push_u64(&mut payload, fields.trace_id);
    payload
}

fn build_packet(mut fields: HeaderFields, metadata: Vec<u8>, body: Vec<u8>) -> Vec<u8> {
    fields.meta_len = metadata.len() as u32;
    fields.body_len = body.len() as u32;
    let mut packet = pack_header(fields);
    packet.extend_from_slice(&metadata);
    packet.extend_from_slice(&body);
    packet
}

fn pack_typed_payload_descriptor_region(frames: &[TypedPayloadFrameRecipe]) -> Vec<u8> {
    let mut descriptors = Vec::with_capacity(frames.len() * 16);
    let mut payload_offset = 0u32;

    for frame in frames {
        let payload = frame.payload_utf8.as_bytes();
        push_u8(&mut descriptors, frame.payload_kind.as_u8());
        push_u8(&mut descriptors, 0);
        push_u16(&mut descriptors, frame.profile_id);
        push_u32(&mut descriptors, payload_offset);
        push_u32(&mut descriptors, payload.len() as u32);
        push_u32(&mut descriptors, 0);
        payload_offset += payload.len() as u32;
    }

    descriptors
}

fn pack_typed_payload_frame_region(frames: &[TypedPayloadFrameRecipe]) -> Vec<u8> {
    let mut payload = Vec::new();
    for frame in frames {
        payload.extend_from_slice(frame.payload_utf8.as_bytes());
    }
    payload
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn push_u8(buffer: &mut Vec<u8>, value: u8) {
    buffer.push(value);
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(buffer: &mut Vec<u8>, value: u64) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn header_flags(flags: &[HeaderFlagName]) -> u32 {
    let mut bits = 0u32;
    for flag in flags {
        bits |= flag.as_u32();
    }
    bits
}

fn payload_kinds(payload_kinds: &[PayloadKindName]) -> u32 {
    let mut bits = 0u32;
    for payload_kind in payload_kinds {
        bits |= payload_kind.as_u32();
    }
    bits
}

fn session_patch_fields(fields: &[SessionPatchFieldName]) -> u32 {
    let mut bits = 0u32;
    for field in fields {
        bits |= field.as_u32();
    }
    bits
}

fn flow_update_flags_bits(flags: &[FlowUpdateFlagName]) -> u32 {
    let mut bits = 0u32;
    for flag in flags {
        bits |= flag.as_u32();
    }
    bits
}

fn budget_policy_bits(flags: &[BudgetPolicyName]) -> u8 {
    let mut bits = 0u8;
    for flag in flags {
        bits |= flag.as_u8();
    }
    bits
}

fn result_flags_bits(flags: &[ResultFlagName]) -> u16 {
    let mut bits = 0u16;
    for flag in flags {
        bits |= flag.as_u16();
    }
    bits
}

impl MessageTypeName {
    fn as_u8(self) -> u8 {
        match self {
            Self::ClientHello => 0x01,
            Self::SessionPatchAck => 0x04,
            Self::FlowUpdate => 0x17,
            Self::ResultHint => 0x18,
            Self::FrameSubmit => 0x10,
            Self::ResultPush => 0x12,
        }
    }
}

impl HeaderFlagName {
    fn as_u32(self) -> u32 {
        match self {
            Self::AckRequired => 0x0000_0001,
            Self::CanDrop => 0x0000_0002,
            Self::Stale => 0x0000_0004,
            Self::Eos => 0x0000_0008,
            Self::Retransmit => 0x0000_0010,
            Self::Keyframe => 0x0000_0020,
        }
    }
}

impl PayloadKindName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Tensor => 0x01,
            Self::TokenChunk => 0x02,
            Self::AudioChunk => 0x04,
            Self::VideoChunk => 0x08,
            Self::StructuredEvent => 0x10,
            Self::ToolDelta => 0x20,
            Self::OpaqueBytes => 0x40,
        }
    }

    fn as_u32(self) -> u32 {
        u32::from(self.as_u8())
    }
}

impl SessionPatchFieldName {
    fn as_u32(self) -> u32 {
        match self {
            Self::TargetCadence => 0x0000_0001,
            Self::QualityTier => 0x0000_0002,
            Self::DegradePolicy => 0x0000_0004,
            Self::ActiveLaneMask => 0x0000_0008,
            Self::PreferredCodec => 0x0000_0010,
            Self::PreferredCompression => 0x0000_0020,
            Self::ProfilePatch => 0x0000_0040,
        }
    }
}

impl SessionPatchAckStatusName {
    fn as_u16(self) -> u16 {
        match self {
            Self::Accepted => 0,
            Self::PartiallyApplied => 1,
            Self::Rejected => 2,
        }
    }
}

impl SessionPatchRejectReasonName {
    fn as_u16(self) -> u16 {
        match self {
            Self::None => 0,
            Self::UnsupportedField => 1,
            Self::InvalidRange => 2,
            Self::UnsupportedStrategy => 3,
            Self::InvalidLaneMask => 4,
            Self::RateLimited => 5,
        }
    }
}

impl FlowUpdateScopeKindName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Connection => 0,
            Self::Session => 1,
            Self::Operation => 2,
        }
    }
}

impl FlowUpdateReasonName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Grant => 0,
            Self::Reduce => 1,
            Self::Pause => 2,
            Self::Resume => 3,
            Self::Congestion => 4,
        }
    }
}

impl FlowUpdateBackpressureLevelName {
    fn as_u8(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Soft => 1,
            Self::Hard => 2,
        }
    }
}

impl FlowUpdateFlagName {
    fn as_u32(self) -> u32 {
        match self {
            Self::CreditValid => 0x0000_0001,
            Self::RetryAfterValid => 0x0000_0002,
            Self::BackgroundOnly => 0x0000_0004,
            Self::DrainInFlightOnly => 0x0000_0008,
        }
    }
}

impl SessionPriorityClassName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Interactive => 0,
            Self::Balanced => 1,
            Self::Background => 2,
        }
    }
}

impl SessionStatusName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Opened => 0,
            Self::Rejected => 1,
            Self::RetryLater => 2,
            Self::Resumed => 3,
        }
    }
}

impl SessionCloseReasonName {
    fn as_u16(self) -> u16 {
        match self {
            Self::Normal => 0,
            Self::ClientShutdown => 1,
            Self::ServerShutdown => 2,
            Self::IdleTimeout => 3,
            Self::ProtocolError => 4,
            Self::AuthRevoked => 5,
        }
    }
}

impl InFlightPolicyName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Drain => 0,
            Self::Abort => 1,
        }
    }
}

impl SessionCloseStatusName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Acknowledged => 0,
            Self::Draining => 1,
            Self::Closed => 2,
            Self::Rejected => 3,
        }
    }
}

impl ResultHintBudgetPolicyName {
    fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Full => 1,
            Self::Partial => 2,
            Self::StaleReuse => 3,
            Self::Drop => 4,
        }
    }
}

impl ResultHintCongestionStateName {
    fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Steady => 1,
            Self::Elevated => 2,
            Self::Saturated => 3,
        }
    }
}

impl ResultHintReasonName {
    fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::QueueFull => 1,
            Self::ServerBusy => 2,
            Self::BudgetExceeded => 3,
            Self::Superseded => 4,
        }
    }
}

impl CacheObjectKindName {
    fn as_u16(self) -> u16 {
        match self {
            Self::CameraBlock => 0x0001,
            Self::TileIndexBlock => 0x0002,
            Self::TensorSectionTable => 0x0003,
            Self::CodecTable => 0x0004,
            Self::ReusableResultObject => 0x0005,
            Self::PayloadLayoutTemplate => 0x0006,
            Self::PromptSegment => 0x0007,
            Self::ToolSchema => 0x0008,
            Self::StructuredEventSchema => 0x0009,
        }
    }
}

impl InputProfileName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::ChangedTilesLuma => 1,
            Self::DenseLumaFrame => 2,
        }
    }
}

impl TileIndexModeName {
    fn as_u8(self) -> u8 {
        match self {
            Self::DenseRange => 0,
            Self::RawU16 => 1,
            Self::DeltaU16 => 2,
            Self::Bitset => 3,
        }
    }
}

impl SubmitModeName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Inline => 0,
            Self::Reference => 1,
            Self::Mixed => 2,
        }
    }
}

impl BudgetPolicyName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Partial => 0x01,
            Self::StaleReuse => 0x02,
            Self::Degraded => 0x04,
            Self::Drop => 0x08,
        }
    }
}

impl LossTolerancePolicyName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Strict => 0,
            Self::BestEffort => 1,
            Self::LowLatency => 2,
            Self::FireAndForget => 3,
            Self::InheritSession => 0xff,
        }
    }
}

impl ResultFlagName {
    fn as_u16(self) -> u16 {
        match self {
            Self::Stale => 0x0001,
            Self::Fallback => 0x0002,
            Self::Partial => 0x0004,
        }
    }
}

impl ResultClassName {
    fn as_u8(self) -> u8 {
        match self {
            Self::Complete => 0,
            Self::Partial => 1,
            Self::StaleReuse => 2,
            Self::Degraded => 3,
        }
    }
}
