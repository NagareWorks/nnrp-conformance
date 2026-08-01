use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::Parser;
#[path = "atomic_file.rs"]
mod atomic_file;
use atomic_file::write_file_atomically;
use nnrp_conformance_fixtures::{
    ApiProfileCaseOutcome, WireConformanceCaseResult, WireConformanceCaseResultReport,
    WireConformanceScenario, WireConformanceTerminal, WireConformanceTransport,
    WireHostAcceptedSessionEvidence, WireHostListenerEvidence, WireHostListenerState,
    WireHostProviderRoute, WireHostReadyListener, WireHostRole, WireHostRouteCandidateEvidence,
    WireHostRouteEvidence, WireHostRouteInjectedFailure, WireHostRouteReadyReport,
    WireHostRouteRejectionReason, WireHostRouteSecurityMode,
};
use nnrp_core::{TransportId, TransportPolicy};
use nnrp_runtime::{
    BoundServerProvider, BoxedFramedTransport, ClientProviderRoute, ClientProviderRoutes,
    ClientTransportSecurity, FramedListener, NnrpClient, NnrpClientConfig, NnrpClientOptions,
    NnrpClientProvider, NnrpServer, NnrpServerConfig, NnrpServerOptions, NnrpServerProvider,
    ProviderEndpoint, RuntimeError, RuntimeFrameLimits, RuntimeTransportKind, ServerProviderRoute,
    ServerProviderRoutes, ServerTransportSecurity,
};
use nnrp_transport_ipc::IpcProvider;
use nnrp_transport_provider::{
    TransportCandidateDiagnostic, TransportProviderDescriptor, TransportProviderKind,
    TransportRejectionReason, TransportSelectionError,
};
use nnrp_transport_quic::QuicProvider;
use nnrp_transport_tcp::TcpProvider;
use nnrp_transport_websocket::WebSocketProvider;

#[derive(Debug, Parser)]
#[command(name = "nnrp-wire-host-route-reference-target")]
#[command(about = "Drive the Rust Preview4 host APIs from an independent process")]
struct Args {
    #[arg(long)]
    scenario: PathBuf,
    #[arg(long)]
    resolved_scenario: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    ready_output: PathBuf,
    #[arg(long)]
    artifacts: PathBuf,
    #[arg(long)]
    suite_version: String,
    #[arg(long)]
    target_name: String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SupportedHostRoles {
    pub(crate) client: bool,
    pub(crate) server: bool,
    pub(crate) label: &'static str,
}

impl SupportedHostRoles {
    fn supports(self, role: WireHostRole) -> bool {
        match self {
            Self { client: true, .. } if role == WireHostRole::Client => true,
            Self { server: true, .. } if role == WireHostRole::Server => true,
            _ => false,
        }
    }
}

pub(crate) async fn run(supported_roles: SupportedHostRoles) -> Result<()> {
    let args = Args::parse();
    let scenario: WireConformanceScenario = load_json(&args.scenario)?;
    let resolved: WireConformanceScenario = load_json(&args.resolved_scenario)?;
    let result = match scenario.host_route.as_ref() {
        Some(fixture) if !supported_roles.supports(fixture.role) => Err(anyhow::anyhow!(
            "reference target implements only the {} host role and rejects the {:?} host role",
            supported_roles.label,
            fixture.role
        )),
        _ => run_case(&scenario, &resolved, &args.artifacts, &args.ready_output).await,
    };
    let report = WireConformanceCaseResultReport {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-case-results.schema.json"
                .to_string(),
        ),
        protocol_version: "nnrp-1-preview4".to_string(),
        suite_version: args.suite_version,
        target_name: args.target_name,
        results: vec![match result {
            Ok(result) => result,
            Err(error) => failed_result(&scenario, error.to_string()),
        }],
    };
    write_json(&args.output, &report)
}

async fn run_case(
    scenario: &WireConformanceScenario,
    resolved: &WireConformanceScenario,
    artifacts: &std::path::Path,
    ready_output: &std::path::Path,
) -> Result<WireConformanceCaseResult> {
    let fixture = scenario
        .host_route
        .as_ref()
        .context("host-route driver requires a host-route scenario")?;
    let resolved_fixture = resolved
        .host_route
        .as_ref()
        .context("resolved host-route scenario is missing its fixture")?;
    anyhow::ensure!(
        scenario.id == resolved.id && fixture.routes.len() == resolved_fixture.routes.len(),
        "resolved host-route scenario does not match the suite scenario"
    );
    validate_provider_routes(&fixture.routes)?;
    validate_provider_routes(&resolved_fixture.routes)?;
    anyhow::ensure!(
        fixture
            .routes
            .iter()
            .zip(&resolved_fixture.routes)
            .all(
                |(route, resolved_route)| route.transport == resolved_route.transport
                    && route.provider_id == resolved_route.provider_id
            ),
        "resolved host-route scenario changes provider identities"
    );
    match fixture.role {
        WireHostRole::Client => run_client_case(scenario, resolved, artifacts).await,
        WireHostRole::Server => run_server_case(scenario, resolved, artifacts, ready_output).await,
    }
}

async fn run_client_case(
    scenario: &WireConformanceScenario,
    resolved: &WireConformanceScenario,
    artifacts: &std::path::Path,
) -> Result<WireConformanceCaseResult> {
    let fixture = scenario
        .host_route
        .as_ref()
        .context("client scenario is missing its host-route fixture")?;
    let resolved_fixture = resolved
        .host_route
        .as_ref()
        .context("resolved client scenario is missing its host-route fixture")?;
    let mut routes = ClientProviderRoutes::new();
    let mut providers = Vec::<Arc<dyn NnrpClientProvider>>::new();
    for (route, resolved_route) in fixture.routes.iter().zip(&resolved_fixture.routes) {
        routes.insert(
            transport_id(route.transport),
            client_route(route, resolved_route, artifacts)?,
        );
        providers.push(client_provider(route)?);
    }
    let options = NnrpClientOptions::new(
        fixture.application_endpoint.parse()?,
        routes,
        TransportPolicy::Auto,
        NnrpClientConfig::default(),
    );

    match NnrpClient::connect(options, providers).await {
        Ok(client) => {
            let selection = client
                .transport_selection()
                .context("provider-routed client did not retain transport selection")?
                .clone();
            let session = client.open_session().await?;
            let selected_transport = wire_transport(selection.selected_provider().transport_id)?;
            let selected_provider = selection.selected_provider().metadata.id.clone();
            match session.close().await {
                Ok(()) | Err(RuntimeError::Io(_)) | Err(RuntimeError::TransportClosed { .. }) => {}
                Err(error) => return Err(error.into()),
            }
            Ok(passed_result(
                scenario,
                WireConformanceTerminal::Success,
                WireHostRouteEvidence {
                    application_endpoint: fixture.application_endpoint.clone(),
                    candidates: candidate_evidence(
                        &fixture.routes,
                        &selection.candidates,
                        Some(&selected_provider),
                    )?,
                    listeners: Vec::new(),
                    accepted_sessions: vec![WireHostAcceptedSessionEvidence {
                        transport: selected_transport,
                        provider_id: selected_provider,
                        active_transport: selected_transport,
                    }],
                    atomic_rollback: false,
                    logical_set_closed: false,
                    terminal_failure: None,
                },
            ))
        }
        Err(RuntimeError::TransportSelection(error)) => {
            let diagnostics = selection_error_candidates(&error);
            Ok(passed_result(
                scenario,
                WireConformanceTerminal::Error,
                WireHostRouteEvidence {
                    application_endpoint: fixture.application_endpoint.clone(),
                    candidates: candidate_evidence(&fixture.routes, diagnostics, None)?,
                    listeners: Vec::new(),
                    accepted_sessions: Vec::new(),
                    atomic_rollback: false,
                    logical_set_closed: false,
                    terminal_failure: None,
                },
            ))
        }
        Err(error) => Err(error.into()),
    }
}

async fn run_server_case(
    scenario: &WireConformanceScenario,
    resolved: &WireConformanceScenario,
    artifacts: &std::path::Path,
    ready_output: &std::path::Path,
) -> Result<WireConformanceCaseResult> {
    let fixture = scenario
        .host_route
        .as_ref()
        .context("server scenario is missing its host-route fixture")?;
    let resolved_fixture = resolved
        .host_route
        .as_ref()
        .context("resolved server scenario is missing its host-route fixture")?;
    let terminal_failure = fixture.routes.iter().find(|route| {
        route
            .injected_failures
            .contains(&WireHostRouteInjectedFailure::TerminalListenerFailure)
    });
    let bind_failure = fixture.routes.iter().any(|route| {
        route
            .injected_failures
            .contains(&WireHostRouteInjectedFailure::BindFailure)
    });
    let mut routes = ServerProviderRoutes::new();
    let mut providers = Vec::<Arc<dyn NnrpServerProvider>>::new();
    for (route, resolved_route) in fixture.routes.iter().zip(&resolved_fixture.routes) {
        routes.insert(
            transport_id(route.transport),
            server_route(route, resolved_route, artifacts)?,
        );
        providers.push(server_provider(route)?);
    }
    let policy = if bind_failure {
        rollback_probe_policy(&fixture.routes)?
    } else {
        TransportPolicy::Auto
    };
    let options = NnrpServerOptions::new(
        fixture.application_endpoint.parse()?,
        routes,
        policy,
        NnrpServerConfig::default(),
    );

    match NnrpServer::listen(options, providers).await {
        Ok(server) if terminal_failure.is_some() => {
            write_ready_report(
                scenario,
                fixture,
                server.bound_provider_endpoints(),
                ready_output,
            )?;
            let failure = terminal_failure.context(
                "terminal-listener branch requires an injected terminal listener failure",
            )?;
            let accept_error = match server.accept().await {
                Err(error) => error,
                Ok(session) => {
                    session.close().await?;
                    anyhow::bail!("terminal listener injection accepted a session")
                }
            };
            anyhow::ensure!(
                server.is_listener_set_closed(),
                "terminal listener error did not close the logical listener set"
            );
            Ok(passed_result(
                scenario,
                WireConformanceTerminal::Error,
                server_evidence(
                    fixture,
                    server.bound_provider_endpoints(),
                    &[],
                    ServerEvidenceState {
                        atomic_rollback: false,
                        logical_set_closed: true,
                        terminal_failure: Some(failure.provider_id.clone()),
                        listener_state: Some(WireHostListenerState::Closed),
                    },
                )?,
            )
            .with_message(format!(
                "listener set closed after terminal failure: {accept_error}"
            )))
        }
        Ok(server) => {
            write_ready_report(
                scenario,
                fixture,
                server.bound_provider_endpoints(),
                ready_output,
            )?;
            let mut accepted = Vec::new();
            for _ in 0..fixture.routes.len() {
                let session = server.accept().await?;
                accepted.push(session.active_transport_id());
                session.close().await?;
            }
            Ok(passed_result(
                scenario,
                WireConformanceTerminal::Success,
                server_evidence(
                    fixture,
                    server.bound_provider_endpoints(),
                    &accepted,
                    ServerEvidenceState {
                        atomic_rollback: false,
                        logical_set_closed: false,
                        terminal_failure: None,
                        listener_state: Some(WireHostListenerState::Accepted),
                    },
                )?,
            ))
        }
        Err(error) if bind_failure => Ok(passed_result(
            scenario,
            WireConformanceTerminal::Error,
            rollback_evidence(fixture)?,
        )
        .with_message(format!(
            "listener bind failed and prior listeners rolled back: {error}"
        ))),
        Err(error) => Err(error.into()),
    }
}

trait ResultMessage {
    fn with_message(self, message: String) -> Self;
}

impl ResultMessage for WireConformanceCaseResult {
    fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}

fn client_route(
    route: &WireHostProviderRoute,
    resolved: &WireHostProviderRoute,
    artifacts: &std::path::Path,
) -> Result<ClientProviderRoute> {
    let endpoint = if route
        .injected_failures
        .contains(&WireHostRouteInjectedFailure::RouteUnresolved)
    {
        mismatched_endpoint(route.transport)?
    } else {
        resolved.locator.parse()?
    };
    let security = if route
        .injected_failures
        .contains(&WireHostRouteInjectedFailure::SecurityIncompatible)
    {
        None
    } else {
        client_security(route, artifacts)?
    };
    Ok(ClientProviderRoute {
        provider_endpoint: Some(endpoint),
        security,
    })
}

fn server_route(
    route: &WireHostProviderRoute,
    resolved: &WireHostProviderRoute,
    artifacts: &std::path::Path,
) -> Result<ServerProviderRoute> {
    Ok(ServerProviderRoute {
        provider_endpoint: Some(resolved.locator.parse()?),
        security: server_security(route, artifacts)?,
    })
}

fn client_security(
    route: &WireHostProviderRoute,
    artifacts: &std::path::Path,
) -> Result<Option<ClientTransportSecurity>> {
    match route.security.mode {
        WireHostRouteSecurityMode::TlsServerAuth
        | WireHostRouteSecurityMode::MutualTls
        | WireHostRouteSecurityMode::Wss => Ok(Some(ClientTransportSecurity::new(
            "localhost",
            std::fs::read(artifacts.join("server.der"))?,
        ))),
        WireHostRouteSecurityMode::Plain | WireHostRouteSecurityMode::BrowserHost => Ok(None),
    }
}

fn server_security(
    route: &WireHostProviderRoute,
    artifacts: &std::path::Path,
) -> Result<Option<ServerTransportSecurity>> {
    match route.security.mode {
        WireHostRouteSecurityMode::TlsServerAuth
        | WireHostRouteSecurityMode::MutualTls
        | WireHostRouteSecurityMode::Wss => Ok(Some(ServerTransportSecurity::new(
            std::fs::read(artifacts.join("server.der"))?,
            std::fs::read(artifacts.join("server-key.der"))?,
        ))),
        WireHostRouteSecurityMode::Plain | WireHostRouteSecurityMode::BrowserHost => Ok(None),
    }
}

fn client_provider(route: &WireHostProviderRoute) -> Result<Arc<dyn NnrpClientProvider>> {
    let provider: Arc<dyn NnrpClientProvider> = match route.provider_id.as_str() {
        "nnrp.transport.tcp.native" => Arc::new(TcpProvider),
        "nnrp.transport.quic.native" => Arc::new(QuicProvider),
        "nnrp.transport.ipc.native" => Arc::new(IpcProvider),
        "nnrp.transport.websocket.native" => Arc::new(WebSocketProvider),
        "example.transport.quic.uninstalled" => Arc::new(UnavailableClientProvider::new(
            route.provider_id.clone(),
            TransportId::Quic,
        )),
        provider_id => bail!("unsupported reference client provider {provider_id}"),
    };
    anyhow::ensure!(
        provider.descriptor().metadata.id == route.provider_id,
        "reference client provider identity drifted from the scenario"
    );
    Ok(provider)
}

fn server_provider(route: &WireHostProviderRoute) -> Result<Arc<dyn NnrpServerProvider>> {
    if route
        .injected_failures
        .contains(&WireHostRouteInjectedFailure::BindFailure)
    {
        return Ok(Arc::new(BindFailureServerProvider::new(
            route.provider_id.clone(),
            transport_id(route.transport),
        )));
    }
    if route
        .injected_failures
        .contains(&WireHostRouteInjectedFailure::TerminalListenerFailure)
    {
        return Ok(Arc::new(TerminalFailureServerProvider::new(
            route.provider_id.clone(),
            transport_id(route.transport),
        )));
    }
    let provider: Arc<dyn NnrpServerProvider> = match route.provider_id.as_str() {
        "nnrp.transport.tcp.native" => Arc::new(TcpProvider),
        "nnrp.transport.quic.native" => Arc::new(QuicProvider),
        "nnrp.transport.ipc.native" => Arc::new(IpcProvider),
        "nnrp.transport.websocket.native" => Arc::new(WebSocketProvider),
        provider_id => bail!("unsupported reference server provider {provider_id}"),
    };
    anyhow::ensure!(
        provider.descriptor().metadata.id == route.provider_id,
        "reference server provider identity drifted from the scenario"
    );
    Ok(provider)
}

fn candidate_evidence(
    routes: &[WireHostProviderRoute],
    diagnostics: &[TransportCandidateDiagnostic],
    selected_provider: Option<&str>,
) -> Result<Vec<WireHostRouteCandidateEvidence>> {
    routes
        .iter()
        .map(|route| {
            let diagnostic = diagnostics
                .iter()
                .find(|candidate| {
                    candidate.transport_id == transport_id(route.transport)
                        && candidate.provider.id == route.provider_id
                })
                .with_context(|| {
                    format!("missing candidate diagnostics for {}", route.provider_id)
                })?;
            let rejection_reason = diagnostic
                .rejection_reason
                .map(wire_rejection_reason)
                .transpose()?;
            Ok(WireHostRouteCandidateEvidence {
                transport: route.transport,
                provider_id: route.provider_id.clone(),
                requested_locator: route.locator.clone(),
                locator_resolved: rejection_reason
                    != Some(WireHostRouteRejectionReason::RouteUnresolved),
                security_satisfied: rejection_reason
                    != Some(WireHostRouteRejectionReason::SecurityUnsatisfied),
                selected: selected_provider == Some(route.provider_id.as_str()),
                rejection_reason,
            })
        })
        .collect()
}

fn selection_error_candidates(error: &TransportSelectionError) -> &[TransportCandidateDiagnostic] {
    error.candidates()
}

fn validate_provider_routes(routes: &[WireHostProviderRoute]) -> Result<()> {
    let mut transports = BTreeSet::new();
    let mut provider_ids = BTreeSet::new();
    for route in routes {
        anyhow::ensure!(
            transports.insert(route.transport),
            "host-route scenario declares more than one route for {:?}",
            route.transport
        );
        anyhow::ensure!(
            provider_ids.insert(route.provider_id.as_str()),
            "host-route scenario repeats provider id {}",
            route.provider_id
        );
    }
    Ok(())
}

fn rollback_probe_policy(routes: &[WireHostProviderRoute]) -> Result<TransportPolicy> {
    let opened_route = routes
        .iter()
        .find(|route| {
            !route
                .injected_failures
                .contains(&WireHostRouteInjectedFailure::BindFailure)
        })
        .context("bind-failure scenario requires a provider that opens before rollback")?;
    Ok(match opened_route.transport {
        WireConformanceTransport::Tcp => TransportPolicy::PreferTcp,
        WireConformanceTransport::Quic => TransportPolicy::PreferQuic,
        WireConformanceTransport::Ipc => TransportPolicy::PreferIpc,
        WireConformanceTransport::Websocket => TransportPolicy::PreferWebSocket,
    })
}

struct ServerEvidenceState {
    atomic_rollback: bool,
    logical_set_closed: bool,
    terminal_failure: Option<String>,
    listener_state: Option<WireHostListenerState>,
}

fn server_evidence(
    fixture: &nnrp_conformance_fixtures::WireHostRouteFixture,
    bound: &BTreeMap<TransportId, ProviderEndpoint>,
    accepted: &[TransportId],
    state: ServerEvidenceState,
) -> Result<WireHostRouteEvidence> {
    let listeners = fixture
        .routes
        .iter()
        .map(|route| WireHostListenerEvidence {
            transport: route.transport,
            provider_id: route.provider_id.clone(),
            requested_locator: route.locator.clone(),
            bound_endpoint: bound
                .get(&transport_id(route.transport))
                .map(ToString::to_string),
            state: state
                .listener_state
                .unwrap_or(WireHostListenerState::Opened),
        })
        .collect();
    let accepted_sessions = accepted
        .iter()
        .map(|transport| {
            let transport = wire_transport(*transport)?;
            let route = fixture
                .routes
                .iter()
                .find(|route| route.transport == transport)
                .context("accepted server transport is outside the route fixture")?;
            Ok(WireHostAcceptedSessionEvidence {
                transport,
                provider_id: route.provider_id.clone(),
                active_transport: transport,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(WireHostRouteEvidence {
        application_endpoint: fixture.application_endpoint.clone(),
        candidates: fixture
            .routes
            .iter()
            .map(|route| WireHostRouteCandidateEvidence {
                transport: route.transport,
                provider_id: route.provider_id.clone(),
                requested_locator: route.locator.clone(),
                locator_resolved: true,
                security_satisfied: true,
                selected: false,
                rejection_reason: None,
            })
            .collect(),
        listeners,
        accepted_sessions,
        atomic_rollback: state.atomic_rollback,
        logical_set_closed: state.logical_set_closed,
        terminal_failure: state.terminal_failure,
    })
}

fn write_ready_report(
    scenario: &WireConformanceScenario,
    fixture: &nnrp_conformance_fixtures::WireHostRouteFixture,
    bound: &BTreeMap<TransportId, ProviderEndpoint>,
    path: &std::path::Path,
) -> Result<()> {
    let listeners = bound
        .iter()
        .map(|(transport, endpoint)| {
            let transport = wire_transport(*transport)?;
            let route = fixture
                .routes
                .iter()
                .find(|route| route.transport == transport)
                .context("bound server transport is outside the route fixture")?;
            Ok(WireHostReadyListener {
                transport,
                provider_id: route.provider_id.clone(),
                bound_endpoint: endpoint.to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let report = WireHostRouteReadyReport {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-host-route-ready.schema.json"
                .to_string(),
        ),
        protocol_version: "nnrp-1-preview4".to_string(),
        scenario_id: scenario.id.clone(),
        listeners,
    };
    write_json_atomic(path, &report)
}

fn rollback_evidence(
    fixture: &nnrp_conformance_fixtures::WireHostRouteFixture,
) -> Result<WireHostRouteEvidence> {
    let mut evidence = server_evidence(
        fixture,
        &BTreeMap::new(),
        &[],
        ServerEvidenceState {
            atomic_rollback: true,
            logical_set_closed: true,
            terminal_failure: None,
            listener_state: None,
        },
    )?;
    for listener in &mut evidence.listeners {
        let route = fixture
            .routes
            .iter()
            .find(|route| route.provider_id == listener.provider_id)
            .context("rollback listener is outside the route fixture")?;
        if route
            .injected_failures
            .contains(&WireHostRouteInjectedFailure::BindFailure)
        {
            listener.bound_endpoint = None;
            listener.state = WireHostListenerState::Failed;
        } else {
            listener.state = WireHostListenerState::RolledBack;
        }
    }
    Ok(evidence)
}

fn passed_result(
    scenario: &WireConformanceScenario,
    terminal: WireConformanceTerminal,
    evidence: WireHostRouteEvidence,
) -> WireConformanceCaseResult {
    WireConformanceCaseResult {
        id: scenario.id.clone(),
        outcome: ApiProfileCaseOutcome::Passed,
        terminal,
        observed_frames: Vec::new(),
        route_evidence: Some(evidence),
        message: Some("independent host-route target executed the public SDK host API".to_string()),
        evidence_paths: Vec::new(),
    }
}

fn failed_result(scenario: &WireConformanceScenario, message: String) -> WireConformanceCaseResult {
    WireConformanceCaseResult {
        id: scenario.id.clone(),
        outcome: ApiProfileCaseOutcome::Failed,
        terminal: WireConformanceTerminal::Error,
        observed_frames: Vec::new(),
        route_evidence: None,
        message: Some(message),
        evidence_paths: Vec::new(),
    }
}

fn transport_id(transport: WireConformanceTransport) -> TransportId {
    match transport {
        WireConformanceTransport::Tcp => TransportId::Tcp,
        WireConformanceTransport::Quic => TransportId::Quic,
        WireConformanceTransport::Ipc => TransportId::Ipc,
        WireConformanceTransport::Websocket => TransportId::WebSocket,
    }
}

fn wire_transport(transport: TransportId) -> Result<WireConformanceTransport> {
    match transport {
        TransportId::Tcp => Ok(WireConformanceTransport::Tcp),
        TransportId::Quic => Ok(WireConformanceTransport::Quic),
        TransportId::Ipc => Ok(WireConformanceTransport::Ipc),
        TransportId::WebSocket => Ok(WireConformanceTransport::Websocket),
        other => bail!("unsupported host-route transport {other:?}"),
    }
}

fn wire_rejection_reason(reason: TransportRejectionReason) -> Result<WireHostRouteRejectionReason> {
    Ok(match reason {
        TransportRejectionReason::PolicyDisallowed => {
            WireHostRouteRejectionReason::PolicyDisallowed
        }
        TransportRejectionReason::LocalUnavailable => {
            WireHostRouteRejectionReason::LocalUnavailable
        }
        TransportRejectionReason::PeerUnsupported => WireHostRouteRejectionReason::PeerUnsupported,
        TransportRejectionReason::LimitExceeded => WireHostRouteRejectionReason::LimitExceeded,
        TransportRejectionReason::RouteUnresolved => WireHostRouteRejectionReason::RouteUnresolved,
        TransportRejectionReason::SecurityUnsatisfied => {
            WireHostRouteRejectionReason::SecurityUnsatisfied
        }
        TransportRejectionReason::ProbeMissing => WireHostRouteRejectionReason::ProbeMissing,
        TransportRejectionReason::ProbeFailed => WireHostRouteRejectionReason::ProbeFailed,
    })
}

fn mismatched_endpoint(transport: WireConformanceTransport) -> Result<ProviderEndpoint> {
    match transport {
        WireConformanceTransport::Tcp
        | WireConformanceTransport::Quic
        | WireConformanceTransport::Websocket => Ok("unix:///nnrp-route-unresolved".parse()?),
        WireConformanceTransport::Ipc => Ok("tcp://127.0.0.1:9".parse()?),
    }
}

fn load_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn write_json_atomic<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
    let contents = format!("{}\n", serde_json::to_string_pretty(value)?);
    write_file_atomically(path, contents.as_bytes()).map_err(Into::into)
}

struct UnavailableClientProvider {
    descriptor: TransportProviderDescriptor,
}

impl UnavailableClientProvider {
    fn new(provider_id: String, transport_id: TransportId) -> Self {
        let mut descriptor = TransportProviderDescriptor::missing(
            "reference-uninstalled-provider",
            env!("CARGO_PKG_VERSION"),
            transport_id,
            TransportProviderKind::PureRust,
            "provider package is not installed",
        );
        descriptor.metadata.id = provider_id;
        Self { descriptor }
    }
}

#[async_trait]
impl NnrpClientProvider for UnavailableClientProvider {
    fn descriptor(&self) -> TransportProviderDescriptor {
        self.descriptor.clone()
    }

    async fn connect(
        &self,
        _endpoint: &ProviderEndpoint,
        _security: Option<&ClientTransportSecurity>,
        _limits: RuntimeFrameLimits,
    ) -> Result<BoxedFramedTransport, RuntimeError> {
        Err(RuntimeError::SelectedProviderUnavailable(
            self.descriptor.metadata.id.clone(),
        ))
    }
}

struct TerminalFailureServerProvider {
    descriptor: TransportProviderDescriptor,
}

struct BindFailureServerProvider {
    descriptor: TransportProviderDescriptor,
}

impl BindFailureServerProvider {
    fn new(provider_id: String, transport_id: TransportId) -> Self {
        let mut descriptor = TransportProviderDescriptor::available(
            "reference-bind-failure-provider",
            env!("CARGO_PKG_VERSION"),
            transport_id,
            TransportProviderKind::PureRust,
        );
        descriptor.metadata.id = provider_id;
        Self { descriptor }
    }
}

#[async_trait]
impl NnrpServerProvider for BindFailureServerProvider {
    fn descriptor(&self) -> TransportProviderDescriptor {
        self.descriptor.clone()
    }

    async fn bind(
        &self,
        _endpoint: &ProviderEndpoint,
        _security: Option<&ServerTransportSecurity>,
        _limits: RuntimeFrameLimits,
    ) -> Result<BoundServerProvider, RuntimeError> {
        Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "injected provider bind failure",
        )
        .into())
    }
}

impl TerminalFailureServerProvider {
    fn new(provider_id: String, transport_id: TransportId) -> Self {
        let mut descriptor = TransportProviderDescriptor::available(
            "reference-terminal-failure-provider",
            env!("CARGO_PKG_VERSION"),
            transport_id,
            TransportProviderKind::PureRust,
        );
        descriptor.metadata.id = provider_id;
        Self { descriptor }
    }
}

#[async_trait]
impl NnrpServerProvider for TerminalFailureServerProvider {
    fn descriptor(&self) -> TransportProviderDescriptor {
        self.descriptor.clone()
    }

    async fn bind(
        &self,
        endpoint: &ProviderEndpoint,
        _security: Option<&ServerTransportSecurity>,
        _limits: RuntimeFrameLimits,
    ) -> Result<BoundServerProvider, RuntimeError> {
        BoundServerProvider::new(
            endpoint.clone(),
            Box::new(TerminalFailureListener {
                kind: RuntimeTransportKind::from_transport_id(self.descriptor.transport_id).ok_or(
                    RuntimeError::UnsupportedTransport(
                        "terminal failure provider requires a runtime transport",
                    ),
                )?,
            }),
        )
    }
}

struct TerminalFailureListener {
    kind: RuntimeTransportKind,
}

#[async_trait]
impl FramedListener for TerminalFailureListener {
    fn transport_kind(&self) -> RuntimeTransportKind {
        self.kind
    }

    fn local_addr(&self) -> Result<std::net::SocketAddr, RuntimeError> {
        Err(RuntimeError::UnsupportedTransport(
            "injected terminal listener has no IP address",
        ))
    }

    async fn accept(&self) -> Result<BoxedFramedTransport, RuntimeError> {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "injected terminal listener failure",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::{SupportedHostRoles, rollback_probe_policy};
    use nnrp_conformance_fixtures::{
        WireConformanceTransport, WireHostCredentialOwner, WireHostProviderRoute, WireHostRole,
        WireHostRouteInjectedFailure, WireHostRouteSecurity, WireHostRouteSecurityMode,
    };
    use nnrp_core::TransportPolicy;

    #[test]
    fn rollback_probe_prefers_a_provider_that_opens_before_failure() {
        let routes = [
            route(WireConformanceTransport::Ipc, false),
            route(WireConformanceTransport::Tcp, true),
        ];

        assert_eq!(
            rollback_probe_policy(&routes).unwrap(),
            TransportPolicy::PreferIpc
        );
    }

    #[test]
    fn rollback_probe_rejects_a_vacuous_failure_fixture() {
        let routes = [route(WireConformanceTransport::Tcp, true)];

        assert!(rollback_probe_policy(&routes).is_err());
    }

    #[test]
    fn singular_reference_targets_reject_the_opposite_host_role() {
        let both = roles(true, true);
        let client = roles(true, false);
        let server = roles(false, true);

        assert!(both.supports(WireHostRole::Client));
        assert!(both.supports(WireHostRole::Server));
        assert!(client.supports(WireHostRole::Client));
        assert!(!client.supports(WireHostRole::Server));
        assert!(server.supports(WireHostRole::Server));
        assert!(!server.supports(WireHostRole::Client));
    }

    fn roles(client: bool, server: bool) -> SupportedHostRoles {
        SupportedHostRoles {
            client,
            server,
            label: "test",
        }
    }

    fn route(transport: WireConformanceTransport, bind_failure: bool) -> WireHostProviderRoute {
        WireHostProviderRoute {
            transport,
            provider_id: format!("test.{transport:?}"),
            locator: "suite://allocate/test".to_string(),
            security: WireHostRouteSecurity {
                mode: WireHostRouteSecurityMode::Plain,
                credential_owner: WireHostCredentialOwner::None,
            },
            injected_failures: if bind_failure {
                vec![WireHostRouteInjectedFailure::BindFailure]
            } else {
                Vec::new()
            },
        }
    }
}
