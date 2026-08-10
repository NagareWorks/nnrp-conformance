use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
#[path = "../atomic_file.rs"]
mod atomic_file;
use atomic_file::write_file_atomically;
use nnrp_conformance::wire_endpoint::{
    ReferenceTransport, WireEndpointSecurity, WireReferenceEndpoint,
};
use nnrp_conformance::wire_external::{
    cancel_drop_reason, cancel_trace, canonical_cache_miss, canonical_response_body,
    canonical_result, canonical_trace_body,
};
use nnrp_conformance_fixtures::{
    WireConformanceLimits, WireConformanceMode, WireConformanceTarget,
    WireConformanceTargetManifest, WireConformanceTransport, WireConformanceTransportEndpoint,
    WireConformanceTransportSecurity, WireHostPlatform, WireHostRouteProviderCapability,
    WireHostRouteSecurityMode,
};
use nnrp_core::MessageType;
use nnrp_runtime::{
    NnrpRuntimeEventMetadata, NnrpRuntimeEventTail, NnrpServer, NnrpServerEvent, NnrpServerSession,
    NnrpSubmitHeaderContext, NnrpSubmitIdentity, NnrpSubmitPolicy, NnrpSubmitRequest,
    NnrpTokenChunk, NnrpTokenSubmitInput,
};
use nnrp_transport_quic::QuicServerEndpointConfig;

#[derive(Debug, Parser)]
#[command(name = "nnrp-wire-reference-target")]
#[command(about = "Run a separate-process Preview4 wire-conformance reference target")]
struct Args {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long, value_enum, default_value_t = TargetProfile::Native)]
    profile: TargetProfile,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TargetProfile {
    Native,
    HostRouteOnly,
    UninstalledQuic,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    run_reference_target(&args.manifest, args.profile).await
}

async fn run_reference_target(manifest_path: &Path, profile: TargetProfile) -> Result<()> {
    match profile {
        TargetProfile::HostRouteOnly => return write_host_route_only_manifest(manifest_path),
        TargetProfile::UninstalledQuic => return write_uninstalled_quic_manifest(manifest_path),
        TargetProfile::Native => {}
    }
    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let cert_dir = manifest_dir.join("certs");
    std::fs::create_dir_all(&cert_dir)?;
    let manifest_dir = std::fs::canonicalize(manifest_dir)?;

    let (_, certificate) = QuicServerEndpointConfig::self_signed_localhost(
        "127.0.0.1:0"
            .parse()
            .expect("fixed bind address must parse"),
    )?;
    let security = WireEndpointSecurity {
        server_name: "localhost".to_string(),
        trusted_certificate_der: certificate.certificate_der.clone(),
        certificate_der: certificate.certificate_der,
        private_key_pkcs8_der: certificate.private_key_pkcs8_der,
    };
    std::fs::write(cert_dir.join("server.der"), &security.certificate_der)?;
    std::fs::write(
        cert_dir.join("server-key.der"),
        &security.private_key_pkcs8_der,
    )?;

    let tcp_server =
        WireReferenceEndpoint::secure(ReferenceTransport::Tcp, "127.0.0.1:0", security.clone())
            .bind()
            .await?;
    let tcp_addr = tcp_server.local_addr()?;

    let quic_server =
        WireReferenceEndpoint::secure(ReferenceTransport::Quic, "127.0.0.1:0", security.clone())
            .bind()
            .await?;
    let quic_addr = quic_server.local_addr()?;

    let ipc_endpoint = reference_ipc_endpoint(&manifest_dir);
    let ipc_server = WireReferenceEndpoint::plain(ReferenceTransport::Ipc, ipc_endpoint.clone())
        .bind()
        .await?;

    let websocket_addr = reserve_loopback_addr()?;
    let websocket_endpoint = format!("wss://localhost:{}/nnrp", websocket_addr.port());
    write_target_manifest(
        manifest_path,
        tcp_addr,
        quic_addr,
        &ipc_endpoint,
        &websocket_endpoint,
    )?;

    cancel_target(&tcp_server).await?;
    drop(tcp_server);
    priority_target(&quic_server).await?;
    progress_target_with_retry(WireReferenceEndpoint::secure(
        ReferenceTransport::Tcp,
        tcp_addr.to_string(),
        security.clone(),
    ))
    .await?;
    cache_target(&quic_server).await?;
    drop(quic_server);
    cancel_target(&ipc_server).await?;
    drop(ipc_server);
    progress_target_with_retry(WireReferenceEndpoint::secure(
        ReferenceTransport::WebSocket,
        websocket_endpoint,
        security,
    ))
    .await?;

    cleanup_ipc_endpoint(&ipc_endpoint);
    Ok(())
}

fn write_target_manifest(
    manifest_path: &Path,
    tcp_addr: SocketAddr,
    quic_addr: SocketAddr,
    ipc_endpoint: &str,
    websocket_endpoint: &str,
) -> Result<()> {
    let tls = WireConformanceTransportSecurity {
        server_name: "localhost".to_string(),
        trusted_certificate_der_path: "certs/server.der".to_string(),
        certificate_der_path: "certs/server.der".to_string(),
        private_key_pkcs8_der_path: "certs/server-key.der".to_string(),
    };
    let manifest = WireConformanceTargetManifest {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-target.schema.json"
                .to_string(),
        ),
        target_name: "reference-preview4-target".to_string(),
        protocol_version: "nnrp-1-preview4".to_string(),
        suite_version: env!("CARGO_PKG_VERSION").to_string(),
        wire_conformance: WireConformanceTarget {
            modes: vec![
                WireConformanceMode::SuiteAsClient,
                WireConformanceMode::SuiteAsServer,
                WireConformanceMode::SuiteAsProxy,
            ],
            transports: vec![
                WireConformanceTransportEndpoint {
                    name: WireConformanceTransport::Tcp,
                    endpoint: tcp_addr.to_string(),
                    tls: true,
                    security: Some(tls.clone()),
                },
                WireConformanceTransportEndpoint {
                    name: WireConformanceTransport::Quic,
                    endpoint: quic_addr.to_string(),
                    tls: true,
                    security: Some(tls.clone()),
                },
                WireConformanceTransportEndpoint {
                    name: WireConformanceTransport::Ipc,
                    endpoint: ipc_endpoint.to_string(),
                    tls: false,
                    security: None,
                },
                WireConformanceTransportEndpoint {
                    name: WireConformanceTransport::Websocket,
                    endpoint: websocket_endpoint.to_string(),
                    tls: true,
                    security: Some(tls),
                },
            ],
            host_route_providers: native_host_route_providers(),
            capabilities: vec![
                "control.cancel_abort".to_string(),
                "control.priority_update".to_string(),
                "control.deadline_expire".to_string(),
                "control.progress_partial".to_string(),
                "control.credit_backpressure".to_string(),
                "control.capability_costs".to_string(),
                "control.route_execution_hint".to_string(),
                "cache.reference".to_string(),
                "control.trace_context".to_string(),
                "control.result_drop_reason".to_string(),
                "control.degrade_profile".to_string(),
                "control.budget_update".to_string(),
                "object.lifecycle".to_string(),
                "host.routes".to_string(),
            ],
            limits: WireConformanceLimits {
                max_frame_bytes: 16_777_216,
                max_in_flight: 256,
            },
        },
    };
    write_manifest_atomically(manifest_path, &manifest)
}

fn write_host_route_only_manifest(manifest_path: &Path) -> Result<()> {
    let manifest = WireConformanceTargetManifest {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-target.schema.json"
                .to_string(),
        ),
        target_name: "reference-preview4-host-route-only-target".to_string(),
        protocol_version: "nnrp-1-preview4".to_string(),
        suite_version: env!("CARGO_PKG_VERSION").to_string(),
        wire_conformance: WireConformanceTarget {
            modes: vec![
                WireConformanceMode::SuiteAsClient,
                WireConformanceMode::SuiteAsServer,
            ],
            transports: Vec::new(),
            host_route_providers: native_host_route_providers(),
            capabilities: vec!["host.routes".to_string()],
            limits: WireConformanceLimits {
                max_frame_bytes: 16_777_216,
                max_in_flight: 256,
            },
        },
    };
    write_manifest_atomically(manifest_path, &manifest)
}

fn native_host_route_providers() -> Vec<WireHostRouteProviderCapability> {
    vec![
        WireHostRouteProviderCapability {
            transport: WireConformanceTransport::Tcp,
            provider_id: "nnrp.transport.tcp.native".to_string(),
            installed: true,
            platforms: vec![WireHostPlatform::Native],
            security_modes: vec![
                WireHostRouteSecurityMode::Plain,
                WireHostRouteSecurityMode::TlsServerAuth,
                WireHostRouteSecurityMode::MutualTls,
            ],
        },
        WireHostRouteProviderCapability {
            transport: WireConformanceTransport::Quic,
            provider_id: "nnrp.transport.quic.native".to_string(),
            installed: true,
            platforms: vec![WireHostPlatform::Native],
            security_modes: vec![
                WireHostRouteSecurityMode::TlsServerAuth,
                WireHostRouteSecurityMode::MutualTls,
            ],
        },
        WireHostRouteProviderCapability {
            transport: WireConformanceTransport::Ipc,
            provider_id: "nnrp.transport.ipc.native".to_string(),
            installed: true,
            platforms: vec![WireHostPlatform::Native],
            security_modes: vec![WireHostRouteSecurityMode::Plain],
        },
        WireHostRouteProviderCapability {
            transport: WireConformanceTransport::Websocket,
            provider_id: "nnrp.transport.websocket.native".to_string(),
            installed: true,
            platforms: vec![WireHostPlatform::Native],
            security_modes: vec![
                WireHostRouteSecurityMode::Plain,
                WireHostRouteSecurityMode::Wss,
            ],
        },
    ]
}

fn write_uninstalled_quic_manifest(manifest_path: &Path) -> Result<()> {
    let manifest = WireConformanceTargetManifest {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-target.schema.json"
                .to_string(),
        ),
        target_name: "reference-preview4-uninstalled-quic-target".to_string(),
        protocol_version: "nnrp-1-preview4".to_string(),
        suite_version: env!("CARGO_PKG_VERSION").to_string(),
        wire_conformance: WireConformanceTarget {
            modes: vec![WireConformanceMode::SuiteAsServer],
            transports: Vec::new(),
            host_route_providers: vec![WireHostRouteProviderCapability {
                transport: WireConformanceTransport::Quic,
                provider_id: "example.transport.quic.uninstalled".to_string(),
                installed: false,
                platforms: vec![WireHostPlatform::Native],
                security_modes: vec![WireHostRouteSecurityMode::TlsServerAuth],
            }],
            capabilities: vec!["host.routes".to_string()],
            limits: WireConformanceLimits {
                max_frame_bytes: 16_777_216,
                max_in_flight: 256,
            },
        },
    };
    write_manifest_atomically(manifest_path, &manifest)
}

fn write_manifest_atomically(
    manifest_path: &Path,
    manifest: &WireConformanceTargetManifest,
) -> Result<()> {
    let contents = format!("{}\n", serde_json::to_string_pretty(manifest)?);
    write_file_atomically(manifest_path, contents.as_bytes()).map_err(Into::into)
}

async fn cancel_target(server: &NnrpServer) -> Result<()> {
    let mut session = server.accept().await?;
    let submit = session.receive_submit().await?;
    match session.await_event().await? {
        NnrpServerEvent::Runtime(event)
            if event.header.message_type == MessageType::Cancel
                && matches!(event.metadata, NnrpRuntimeEventMetadata::ControlRequest(_)) => {}
        event => bail!("cancel target expected CANCEL, got {event:?}"),
    }
    session
        .send_trace_context(
            submit.frame_id,
            cancel_trace(),
            canonical_trace_body().to_vec(),
        )
        .await?;
    session
        .send_result_drop_reason(cancel_drop_reason(submit.operation_id))
        .await?;
    close_server_session(&mut session).await
}

async fn priority_target(server: &NnrpServer) -> Result<()> {
    let mut session = server.accept().await?;
    let submit = session.receive_submit().await?;
    for expected in [MessageType::PriorityUpdate, MessageType::ExpireAt] {
        match session.await_event().await? {
            NnrpServerEvent::Runtime(event)
                if event.header.message_type == expected
                    && matches!(event.metadata, NnrpRuntimeEventMetadata::Scheduling(_)) => {}
            event => bail!("priority target expected {expected:?}, got {event:?}"),
        }
    }
    session
        .send_result_drop_reason(cancel_drop_reason(submit.operation_id))
        .await?;
    close_server_session(&mut session).await
}

async fn cache_target(server: &NnrpServer) -> Result<()> {
    let mut session = server.accept().await?;
    let submit = session.receive_submit().await?;
    match session.await_event().await? {
        NnrpServerEvent::Runtime(event)
            if event.header.message_type == MessageType::CapabilityNegotiation
                && matches!(event.metadata, NnrpRuntimeEventMetadata::Capability(_)) => {}
        event => bail!("cache target expected CAPABILITY_NEGOTIATION, got {event:?}"),
    }
    match session.await_event().await? {
        NnrpServerEvent::Runtime(event)
            if event.header.message_type == MessageType::RouteHint
                && matches!(event.metadata, NnrpRuntimeEventMetadata::RouteHint(_)) => {}
        event => bail!("cache target expected ROUTE_HINT, got {event:?}"),
    }
    match session.await_event().await? {
        NnrpServerEvent::Runtime(event)
            if event.header.message_type == MessageType::CacheReference
                && matches!(event.metadata, NnrpRuntimeEventMetadata::CacheReference(_)) => {}
        event => bail!("cache target expected CACHE_REFERENCE, got {event:?}"),
    }
    session
        .send_cache_miss(canonical_cache_miss(), Vec::new())
        .await?;
    session
        .send_result(
            submit.frame_id,
            canonical_result(),
            canonical_response_body().to_vec(),
        )
        .await?;
    close_server_session(&mut session).await
}

async fn progress_target_with_retry(endpoint: WireReferenceEndpoint) -> Result<()> {
    let mut last_error = None;
    for _ in 0..100 {
        match endpoint.connect().await {
            Ok(client) => {
                let mut session = client.open_session().await?;
                session.submit_nowait(token_submit(301)?).await?;
                match session.await_event().await? {
                    event
                        if event.header.message_type == MessageType::Progress
                            && matches!(event.metadata, NnrpRuntimeEventMetadata::Progress(_)) => {}
                    event => bail!("progress target expected PROGRESS, got {event:?}"),
                }
                match session.await_event().await? {
                    event
                        if event.header.message_type == MessageType::CreditUpdate
                            && matches!(
                                event.metadata,
                                NnrpRuntimeEventMetadata::Pressure(metadata)
                                    if metadata.credit_window == 1
                            ) => {}
                    event => bail!("progress target expected CREDIT_UPDATE, got {event:?}"),
                }
                match session.await_event().await? {
                    event
                        if event.header.message_type == MessageType::PartialResult
                            && matches!(
                                event.metadata,
                                NnrpRuntimeEventMetadata::PartialResult(_)
                            ) => {}
                    event => bail!("progress target expected PARTIAL_RESULT, got {event:?}"),
                }
                match session.await_event().await? {
                    event
                        if event.header.message_type == MessageType::ResultPush
                            && matches!(
                                event.metadata,
                                NnrpRuntimeEventMetadata::ResultPush(metadata)
                                    if metadata == canonical_result()
                            )
                            && matches!(
                                &event.tail,
                                NnrpRuntimeEventTail::Body(body)
                                    if body.as_slice() == canonical_response_body()
                            ) => {}
                    event => bail!("progress target expected canonical RESULT_PUSH, got {event:?}"),
                }
                session.close().await?;
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    let error = last_error
        .context("progress target did not observe the suite listener before retry timeout")?;
    Err(error.into())
}

async fn close_server_session(session: &mut NnrpServerSession) -> Result<()> {
    let close = session.receive_close().await?;
    session.ack_close(&close).await?;
    session.close_in_place().await?;
    Ok(())
}

fn token_submit(operation_id: u64) -> Result<NnrpSubmitRequest> {
    Ok(NnrpSubmitRequest::token(NnrpTokenSubmitInput {
        identity: NnrpSubmitIdentity {
            operation_id,
            frame_id: 1,
            header: NnrpSubmitHeaderContext::default(),
        },
        policy: NnrpSubmitPolicy {
            latency_budget_ms: 25,
            ..NnrpSubmitPolicy::default()
        },
        chunks: vec![NnrpTokenChunk {
            payload: b"reference-target-request".to_vec(),
            descriptor_flags: 0,
        }],
    })?)
}

fn reserve_loopback_addr() -> Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    Ok(address)
}

#[cfg(windows)]
fn reference_ipc_endpoint(_manifest_dir: &Path) -> String {
    format!("npipe://nnrp-wire-reference-{}", std::process::id())
}

#[cfg(not(windows))]
fn reference_ipc_endpoint(manifest_dir: &Path) -> String {
    format!(
        "unix://{}",
        manifest_dir
            .join(format!("nnrp-wire-reference-{}.sock", std::process::id()))
            .display()
    )
}

#[cfg(windows)]
fn cleanup_ipc_endpoint(_endpoint: &str) {}

#[cfg(not(windows))]
fn cleanup_ipc_endpoint(endpoint: &str) {
    if let Some(path) = endpoint.strip_prefix("unix://") {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::write_file_atomically;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn atomic_replace_overwrites_an_existing_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nnrp-wire-manifest-replace-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let destination = directory.join("manifest.json");
        std::fs::write(&destination, "old").unwrap();

        write_file_atomically(&destination, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "new");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
