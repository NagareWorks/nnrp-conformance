use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use nnrp_conformance::wire_endpoint::{
    ReferenceTransport, WireEndpointSecurity, WireReferenceEndpoint,
};
use nnrp_conformance_fixtures::{
    ApiProfileCaseOutcome, FixtureError, WireConformanceCaseResult,
    WireConformanceCaseResultReport, WireConformanceScenario, WireConformanceTransport,
    WireHostProviderRoute, WireHostRole, WireHostRouteInjectedFailure, WireHostRouteReadyReport,
    WireHostRouteSecurityMode, load_json_file,
};
use nnrp_runtime::{NnrpServer, RuntimeError};
use nnrp_transport_quic::QuicServerEndpointConfig;
use tokio::{sync::mpsc, task::JoinHandle};

const HOST_ROUTE_DRIVER_STARTUP_GRACE_MS: u64 = 15_000;

pub(crate) async fn run_host_route_scenario(
    scenario: &WireConformanceScenario,
    target_executable: &Path,
    artifact_root: &Path,
    suite_version: &str,
    target_name: &str,
) -> Result<WireConformanceCaseResult, FixtureError> {
    let fixture = scenario
        .host_route
        .as_ref()
        .ok_or_else(|| validation("host-route executor received a frame-only scenario"))?;
    let case_root = artifact_root.join(sanitize_id(&scenario.id));
    recreate_directory(&case_root)?;
    let security = prepare_security(&case_root, scenario)?;
    let execution = match fixture.role {
        WireHostRole::Client => {
            run_client_scenario(
                scenario,
                target_executable,
                &case_root,
                security.as_ref(),
                suite_version,
                target_name,
            )
            .await
        }
        WireHostRole::Server => {
            run_server_scenario(
                scenario,
                target_executable,
                &case_root,
                security.as_ref(),
                suite_version,
                target_name,
            )
            .await
        }
    }?;
    Ok(execution)
}

struct PreparedClientRoute {
    route: WireHostProviderRoute,
    server: NnrpServer,
}

struct AbortTasks(Vec<JoinHandle<()>>);

impl Drop for AbortTasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

struct ChildGuard(Child);

impl ChildGuard {
    fn child_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

async fn run_client_scenario(
    scenario: &WireConformanceScenario,
    target_executable: &Path,
    case_root: &Path,
    security: Option<&WireEndpointSecurity>,
    suite_version: &str,
    target_name: &str,
) -> Result<WireConformanceCaseResult, FixtureError> {
    let fixture = scenario
        .host_route
        .as_ref()
        .ok_or_else(|| validation("client scenario is missing its host-route fixture"))?;
    let mut resolved = scenario.clone();
    let resolved_fixture = resolved
        .host_route
        .as_mut()
        .ok_or_else(|| validation("resolved client scenario is missing its host-route fixture"))?;
    let mut prepared = Vec::new();
    for (index, route) in fixture.routes.iter().enumerate() {
        if route.injected_failures.is_empty()
            && route.provider_id != "example.transport.quic.uninstalled"
        {
            let peer = bind_client_peer(route, security).await?;
            resolved_fixture.routes[index].locator = peer.route.locator.clone();
            prepared.push(peer);
        } else {
            resolved_fixture.routes[index].locator = unused_locator(route)?;
        }
    }
    let (accepted_tx, mut accepted_rx) = mpsc::channel(prepared.len().max(1));
    let mut tasks = Vec::new();
    for peer in prepared {
        tasks.push(spawn_accept_loop(peer, accepted_tx.clone()));
    }
    let _tasks = AbortTasks(tasks);
    drop(accepted_tx);

    let paths = write_driver_inputs(scenario, &resolved, case_root)?;
    let mut child = ChildGuard(spawn_driver(
        target_executable,
        &paths,
        case_root,
        suite_version,
        target_name,
    )?);
    wait_for_child(child.child_mut(), &paths.stderr, wire_timeout(scenario)).await?;
    let mut result = load_driver_result(&paths.output, scenario, suite_version, target_name)?;
    result.evidence_paths = vec![runner_evidence_path(case_root)];
    if result.outcome != ApiProfileCaseOutcome::Passed {
        return Ok(result);
    }
    let selected = result
        .route_evidence
        .as_ref()
        .and_then(|evidence| {
            evidence
                .candidates
                .iter()
                .find(|candidate| candidate.selected)
        })
        .map(|candidate| (candidate.transport, candidate.provider_id.clone()));
    match selected {
        Some(expected) => {
            let observed = tokio::time::timeout(Duration::from_secs(2), accepted_rx.recv())
                .await
                .map_err(|_| validation("suite did not observe the selected client carrier"))?
                .ok_or_else(|| validation("all suite client peers closed without a session"))?;
            if observed != expected {
                return Err(validation(format!(
                    "target selected {:?}/{} but suite accepted {:?}/{}",
                    expected.0, expected.1, observed.0, observed.1
                )));
            }
        }
        None => {
            if matches!(
                tokio::time::timeout(Duration::from_millis(100), accepted_rx.recv()).await,
                Ok(Some(_))
            ) {
                return Err(validation(
                    "suite observed a client session although the target reported no selection",
                ));
            }
        }
    }
    Ok(result)
}

fn spawn_accept_loop(
    peer: PreparedClientRoute,
    sender: mpsc::Sender<(WireConformanceTransport, String)>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match peer.server.accept().await {
                Ok(session) => {
                    let _ = sender
                        .send((peer.route.transport, peer.route.provider_id.clone()))
                        .await;
                    let _ = session.close().await;
                    return;
                }
                Err(error) => match accept_retry_delay(&error) {
                    Some(delay) => tokio::time::sleep(delay).await,
                    None => return,
                },
            }
        }
    })
}

fn accept_retry_delay(error: &RuntimeError) -> Option<Duration> {
    match error {
        RuntimeError::Io(_) => Some(Duration::from_millis(10)),
        _ => None,
    }
}

async fn bind_client_peer(
    route: &WireHostProviderRoute,
    security: Option<&WireEndpointSecurity>,
) -> Result<PreparedClientRoute, FixtureError> {
    let endpoint = match route.transport {
        WireConformanceTransport::Tcp => {
            wire_endpoint(route, "127.0.0.1:0".to_string(), security.cloned())?
        }
        WireConformanceTransport::Quic => {
            wire_endpoint(route, "127.0.0.1:0".to_string(), security.cloned())?
        }
        WireConformanceTransport::Ipc => wire_endpoint(route, unique_ipc_endpoint(), None)?,
        WireConformanceTransport::Websocket => wire_endpoint(
            route,
            websocket_locator(route.security.mode, 0),
            security.cloned(),
        )?,
    };
    let server = endpoint.bind().await.map_err(runtime_validation)?;
    let locator = match route.transport {
        WireConformanceTransport::Tcp => {
            format!("tcp://{}", server.local_addr().map_err(runtime_validation)?)
        }
        WireConformanceTransport::Quic => {
            format!(
                "quic://{}",
                server.local_addr().map_err(runtime_validation)?
            )
        }
        WireConformanceTransport::Ipc => endpoint.endpoint.clone(),
        WireConformanceTransport::Websocket => websocket_locator(
            route.security.mode,
            server.local_addr().map_err(runtime_validation)?.port(),
        ),
    };
    let mut route = route.clone();
    route.locator = locator;
    Ok(PreparedClientRoute { route, server })
}

async fn run_server_scenario(
    scenario: &WireConformanceScenario,
    target_executable: &Path,
    case_root: &Path,
    security: Option<&WireEndpointSecurity>,
    suite_version: &str,
    target_name: &str,
) -> Result<WireConformanceCaseResult, FixtureError> {
    let mut resolved = scenario.clone();
    let resolved_fixture = resolved
        .host_route
        .as_mut()
        .ok_or_else(|| validation("resolved server scenario is missing its host-route fixture"))?;
    for route in &mut resolved_fixture.routes {
        route.locator = allocate_server_locator(route)?;
    }
    let resolved_routes = resolved_fixture.routes.clone();

    let paths = write_driver_inputs(scenario, &resolved, case_root)?;
    let mut child = ChildGuard(spawn_driver(
        target_executable,
        &paths,
        case_root,
        suite_version,
        target_name,
    )?);
    let mut observed = BTreeSet::new();
    if scenario.expect.terminal == nnrp_conformance_fixtures::WireConformanceTerminal::Success {
        let bound_routes =
            match wait_for_ready(child.child_mut(), &paths, scenario, wire_timeout(scenario)).await
            {
                Ok(routes) => routes,
                Err(error) if paths.output.is_file() => {
                    let mut result =
                        load_driver_result(&paths.output, scenario, suite_version, target_name)?;
                    if result.outcome == ApiProfileCaseOutcome::Passed {
                        return Err(error);
                    }
                    result.evidence_paths = vec![runner_evidence_path(case_root)];
                    return Ok(result);
                }
                Err(error) => return Err(error),
            };
        let mut sessions = Vec::with_capacity(bound_routes.len());
        for route in &bound_routes {
            let endpoint = wire_endpoint(
                route,
                wire_endpoint_value(&route.locator)?,
                security.cloned(),
            )?;
            let client = connect_with_retry(&endpoint, wire_timeout(scenario)).await?;
            let session = client.open_session().await.map_err(runtime_validation)?;
            observed.insert((route.transport, route.provider_id.clone()));
            sessions.push(session);
        }
        for session in sessions {
            match session.close().await {
                Ok(()) | Err(RuntimeError::Io(_)) | Err(RuntimeError::TransportClosed { .. }) => {}
                Err(error) => return Err(runtime_validation(error)),
            }
        }
    }
    wait_for_child(child.child_mut(), &paths.stderr, wire_timeout(scenario)).await?;
    let mut result = load_driver_result(&paths.output, scenario, suite_version, target_name)?;
    result.evidence_paths = vec![runner_evidence_path(case_root)];
    if result.outcome != ApiProfileCaseOutcome::Passed {
        return Ok(result);
    }

    if scenario.expect.terminal == nnrp_conformance_fixtures::WireConformanceTerminal::Success {
        let reported = result
            .route_evidence
            .as_ref()
            .ok_or_else(|| validation("host-route target omitted server route evidence"))?
            .accepted_sessions
            .iter()
            .map(|session| (session.transport, session.provider_id.clone()))
            .collect::<BTreeSet<_>>();
        if reported != observed {
            return Err(validation(format!(
                "target accepted-session evidence {reported:?} does not match suite connections {observed:?}"
            )));
        }
    } else {
        let closure_routes = if paths.ready.is_file() {
            load_ready_routes(&paths.ready, scenario)?
        } else {
            resolved_routes
        };
        for route in &closure_routes {
            if route.injected_failures.iter().any(|failure| {
                matches!(
                    failure,
                    WireHostRouteInjectedFailure::BindFailure
                        | WireHostRouteInjectedFailure::TerminalListenerFailure
                )
            }) {
                continue;
            }
            let endpoint = wire_endpoint(
                route,
                wire_endpoint_value(&route.locator)?,
                security.cloned(),
            )?;
            if tokio::time::timeout(closure_probe_timeout(scenario), endpoint.connect())
                .await
                .is_ok_and(|result| result.is_ok())
            {
                return Err(validation(format!(
                    "suite reconnected to {:?}/{} after target reported rollback or closure",
                    route.transport, route.provider_id
                )));
            }
        }
    }
    Ok(result)
}

fn runner_evidence_path(case_root: &Path) -> String {
    case_root
        .join("runner-evidence.jsonl")
        .display()
        .to_string()
}

async fn connect_with_retry(
    endpoint: &WireReferenceEndpoint,
    timeout: Duration,
) -> Result<nnrp_runtime::NnrpClient, FixtureError> {
    let started = tokio::time::Instant::now();
    let mut last_error = None;
    while started.elapsed() < timeout {
        match endpoint.connect().await {
            Ok(client) => return Ok(client),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
    Err(runtime_validation(last_error.unwrap_or(
        RuntimeError::UnsupportedTransport("suite did not attempt a server connection"),
    )))
}

struct DriverPaths {
    scenario: PathBuf,
    resolved: PathBuf,
    output: PathBuf,
    ready: PathBuf,
    stderr: PathBuf,
}

fn write_driver_inputs(
    scenario: &WireConformanceScenario,
    resolved: &WireConformanceScenario,
    case_root: &Path,
) -> Result<DriverPaths, FixtureError> {
    let paths = DriverPaths {
        scenario: case_root.join("scenario.json"),
        resolved: case_root.join("resolved-scenario.json"),
        output: case_root.join("target-result.json"),
        ready: case_root.join("target-ready.json"),
        stderr: case_root.join("target.stderr.log"),
    };
    write_json(&paths.scenario, scenario)?;
    write_json(&paths.resolved, resolved)?;
    Ok(paths)
}

fn spawn_driver(
    executable: &Path,
    paths: &DriverPaths,
    case_root: &Path,
    suite_version: &str,
    target_name: &str,
) -> Result<Child, FixtureError> {
    let stderr = fs::File::create(&paths.stderr).map_err(io_validation)?;
    let mut command = driver_command(executable);
    command
        .arg("--scenario")
        .arg(&paths.scenario)
        .arg("--resolved-scenario")
        .arg(&paths.resolved)
        .arg("--output")
        .arg(&paths.output)
        .arg("--ready-output")
        .arg(&paths.ready)
        .arg("--artifacts")
        .arg(case_root)
        .arg("--suite-version")
        .arg(suite_version)
        .arg("--target-name")
        .arg(target_name)
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| validation(format!("failed to start host-route target: {error}")))
}

fn driver_command(executable: &Path) -> Command {
    driver_command_for(executable, cfg!(windows), std::env::var_os("COMSPEC"))
}

fn driver_command_for(
    executable: &Path,
    windows: bool,
    command_processor: Option<std::ffi::OsString>,
) -> Command {
    if windows
        && executable
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            })
    {
        let command_processor = command_processor.unwrap_or_else(|| "cmd.exe".into());
        let mut command = Command::new(command_processor);
        command.arg("/d").arg("/s").arg("/c").arg(executable);
        return command;
    }

    Command::new(executable)
}

async fn wait_for_ready(
    child: &mut Child,
    paths: &DriverPaths,
    scenario: &WireConformanceScenario,
    timeout: Duration,
) -> Result<Vec<WireHostProviderRoute>, FixtureError> {
    let started = tokio::time::Instant::now();
    loop {
        if paths.ready.is_file() {
            return load_ready_routes(&paths.ready, scenario);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| validation(format!("failed to poll host-route target: {error}")))?
        {
            let stderr = fs::read_to_string(&paths.stderr).unwrap_or_default();
            return Err(validation(format!(
                "host-route target exited before readiness with {status}: {stderr}"
            )));
        }
        if started.elapsed() >= timeout {
            return Err(validation(
                "host-route target did not report readiness before timeout",
            ));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn load_ready_routes(
    path: &Path,
    scenario: &WireConformanceScenario,
) -> Result<Vec<WireHostProviderRoute>, FixtureError> {
    let fixture = scenario
        .host_route
        .as_ref()
        .ok_or_else(|| validation("readiness requires a host-route fixture"))?;
    let report: WireHostRouteReadyReport = load_json_file(path)?;
    if report.protocol_version != "nnrp-1-preview4" || report.scenario_id != scenario.id {
        return Err(validation(
            "host-route readiness identity does not match the scenario",
        ));
    }
    let mut resolved = Vec::with_capacity(fixture.routes.len());
    for route in &fixture.routes {
        let listener = report
            .listeners
            .iter()
            .find(|listener| {
                listener.transport == route.transport && listener.provider_id == route.provider_id
            })
            .ok_or_else(|| {
                validation(format!(
                    "host-route readiness omitted {:?}/{}",
                    route.transport, route.provider_id
                ))
            })?;
        let mut route = route.clone();
        route.locator = listener.bound_endpoint.clone();
        resolved.push(route);
    }
    if report.listeners.len() != resolved.len() {
        return Err(validation(
            "host-route readiness contains listeners outside the scenario",
        ));
    }
    Ok(resolved)
}

async fn wait_for_child(
    child: &mut Child,
    stderr_path: &Path,
    timeout: Duration,
) -> Result<(), FixtureError> {
    let started = tokio::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| validation(format!("failed to poll host-route target: {error}")))?
        {
            if status.success() {
                return Ok(());
            }
            let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
            return Err(validation(format!(
                "host-route target exited with {status}: {stderr}"
            )));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
            return Err(validation(format!(
                "host-route target exceeded the scenario timeout: {stderr}"
            )));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn load_driver_result(
    path: &Path,
    scenario: &WireConformanceScenario,
    suite_version: &str,
    target_name: &str,
) -> Result<WireConformanceCaseResult, FixtureError> {
    let report: WireConformanceCaseResultReport = load_json_file(path)?;
    driver_result(report, scenario, suite_version, target_name)
}

fn driver_result(
    report: WireConformanceCaseResultReport,
    scenario: &WireConformanceScenario,
    suite_version: &str,
    target_name: &str,
) -> Result<WireConformanceCaseResult, FixtureError> {
    if report.protocol_version != "nnrp-1-preview4"
        || report.suite_version != suite_version
        || report.target_name != target_name
    {
        return Err(validation(format!(
            "host-route target report identity {}/{}/{} does not match nnrp-1-preview4/{suite_version}/{target_name}",
            report.protocol_version, report.suite_version, report.target_name
        )));
    }
    if report.results.len() != 1 || report.results[0].id != scenario.id {
        return Err(validation(
            "host-route target must emit exactly the requested scenario result",
        ));
    }
    report
        .results
        .into_iter()
        .next()
        .ok_or_else(|| validation("host-route target emitted an empty result report"))
}

fn prepare_security(
    case_root: &Path,
    scenario: &WireConformanceScenario,
) -> Result<Option<WireEndpointSecurity>, FixtureError> {
    let needs_security = scenario.host_route.as_ref().is_some_and(|fixture| {
        fixture.routes.iter().any(|route| {
            matches!(
                route.security.mode,
                WireHostRouteSecurityMode::TlsServerAuth
                    | WireHostRouteSecurityMode::MutualTls
                    | WireHostRouteSecurityMode::Wss
                    | WireHostRouteSecurityMode::BrowserHost
            )
        })
    });
    if !needs_security {
        return Ok(None);
    }
    let security_bind = "127.0.0.1:0"
        .parse()
        .map_err(|error| validation(format!("invalid fixture security bind address: {error}")))?;
    let (_, certificate) = QuicServerEndpointConfig::self_signed_localhost(security_bind)
        .map_err(runtime_validation)?;
    let security = WireEndpointSecurity {
        server_name: "localhost".to_string(),
        trusted_certificate_der: certificate.certificate_der.clone(),
        certificate_der: certificate.certificate_der,
        private_key_pkcs8_der: certificate.private_key_pkcs8_der,
    };
    std::fs::write(case_root.join("server.der"), &security.certificate_der)
        .map_err(io_validation)?;
    std::fs::write(
        case_root.join("server-key.der"),
        &security.private_key_pkcs8_der,
    )
    .map_err(io_validation)?;
    Ok(Some(security))
}

fn wire_endpoint(
    route: &WireHostProviderRoute,
    endpoint: String,
    security: Option<WireEndpointSecurity>,
) -> Result<WireReferenceEndpoint, FixtureError> {
    let transport = match route.transport {
        WireConformanceTransport::Tcp => ReferenceTransport::Tcp,
        WireConformanceTransport::Quic => ReferenceTransport::Quic,
        WireConformanceTransport::Ipc => ReferenceTransport::Ipc,
        WireConformanceTransport::Websocket => ReferenceTransport::WebSocket,
    };
    let secure = matches!(
        route.security.mode,
        WireHostRouteSecurityMode::TlsServerAuth
            | WireHostRouteSecurityMode::MutualTls
            | WireHostRouteSecurityMode::Wss
            | WireHostRouteSecurityMode::BrowserHost
    );
    if secure {
        Ok(WireReferenceEndpoint::secure(
            transport,
            endpoint,
            security.ok_or_else(|| validation("secure host route has no test certificate"))?,
        ))
    } else {
        Ok(WireReferenceEndpoint::plain(transport, endpoint))
    }
}

fn allocate_server_locator(route: &WireHostProviderRoute) -> Result<String, FixtureError> {
    match route.transport {
        WireConformanceTransport::Tcp => Ok("tcp://127.0.0.1:0".to_string()),
        WireConformanceTransport::Quic => Ok("quic://127.0.0.1:0".to_string()),
        WireConformanceTransport::Ipc => Ok(unique_ipc_endpoint()),
        WireConformanceTransport::Websocket => Ok(websocket_locator(route.security.mode, 0)),
    }
}

fn websocket_locator(mode: WireHostRouteSecurityMode, port: u16) -> String {
    let scheme = if matches!(
        mode,
        WireHostRouteSecurityMode::Wss | WireHostRouteSecurityMode::BrowserHost
    ) {
        "wss"
    } else {
        "ws"
    };
    format!("{scheme}://localhost:{port}/nnrp")
}

fn unused_locator(route: &WireHostProviderRoute) -> Result<String, FixtureError> {
    allocate_server_locator(route)
}

fn wire_endpoint_value(locator: &str) -> Result<String, FixtureError> {
    for scheme in ["tcp://", "quic://"] {
        if let Some(value) = locator.strip_prefix(scheme) {
            return Ok(value.to_string());
        }
    }
    Ok(locator.to_string())
}

#[cfg(windows)]
fn unique_ipc_endpoint() -> String {
    format!("npipe://nnrp-host-route-{}", unique_nonce())
}

#[cfg(not(windows))]
fn unique_ipc_endpoint() -> String {
    format!(
        "unix://{}",
        std::env::temp_dir()
            .join(format!("nnrp-host-route-{}.sock", unique_nonce()))
            .display()
    )
}

fn unique_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        ^ u128::from(std::process::id())
}

fn wire_timeout(scenario: &WireConformanceScenario) -> Duration {
    Duration::from_millis(
        declared_timeout_ms(scenario)
            .unwrap_or(0)
            .saturating_add(HOST_ROUTE_DRIVER_STARTUP_GRACE_MS),
    )
}

fn closure_probe_timeout(scenario: &WireConformanceScenario) -> Duration {
    Duration::from_millis(
        declared_timeout_ms(scenario)
            .unwrap_or(1_000)
            .clamp(1_000, 5_000),
    )
}

fn declared_timeout_ms(scenario: &WireConformanceScenario) -> Option<u64> {
    scenario
        .steps
        .iter()
        .filter_map(|step| step.timeout_ms)
        .max()
}

fn recreate_directory(path: &Path) -> Result<(), FixtureError> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(io_validation)?;
    }
    std::fs::create_dir_all(path).map_err(io_validation)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), FixtureError> {
    let contents = serde_json::to_vec_pretty(value)
        .map_err(|error| validation(format!("failed to serialize {}: {error}", path.display())))?;
    std::fs::write(path, contents).map_err(io_validation)
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn validation(message: impl Into<String>) -> FixtureError {
    FixtureError::Validation {
        message: message.into(),
    }
}

fn runtime_validation(error: RuntimeError) -> FixtureError {
    validation(error.to_string())
}

fn io_validation(error: std::io::Error) -> FixtureError {
    validation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        HOST_ROUTE_DRIVER_STARTUP_GRACE_MS, accept_retry_delay, closure_probe_timeout,
        driver_command, driver_command_for, driver_result, runner_evidence_path, websocket_locator,
        wire_timeout,
    };
    use nnrp_conformance_fixtures::{
        ApiProfileCaseOutcome, WireConformanceCaseResult, WireConformanceCaseResultReport,
        WireConformanceScenarioManifest, WireConformanceTerminal, WireHostRouteSecurityMode,
        load_json_file,
    };
    use nnrp_runtime::{RuntimeError, RuntimeTransportKind};
    use std::{path::Path, time::Duration};

    #[test]
    fn windows_command_driver_uses_the_command_processor() {
        let executable = Path::new(r"C:\target path\host.cmd");
        let command = driver_command_for(executable, true, Some("test-command-processor".into()));
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_program(), "test-command-processor");
        assert_eq!(&arguments[..3], ["/d", "/s", "/c"]);
        assert_eq!(arguments[3], executable.to_string_lossy());
    }

    #[test]
    fn native_driver_is_started_directly() {
        let executable = if cfg!(windows) {
            Path::new(r"C:\target\host.exe")
        } else {
            Path::new("/target/host")
        };
        let command = driver_command(executable);

        assert_eq!(command.get_program(), executable.as_os_str());
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn websocket_locator_uses_bound_port_and_security_scheme() {
        assert_eq!(
            websocket_locator(WireHostRouteSecurityMode::Plain, 19091),
            "ws://localhost:19091/nnrp"
        );
        assert_eq!(
            websocket_locator(WireHostRouteSecurityMode::Wss, 19092),
            "wss://localhost:19092/nnrp"
        );
        assert_eq!(
            websocket_locator(WireHostRouteSecurityMode::BrowserHost, 19093),
            "wss://localhost:19093/nnrp"
        );
    }

    #[test]
    fn accept_loop_retries_io_with_backoff_but_stops_after_transport_close() {
        let io = RuntimeError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "retry",
        ));
        let closed = RuntimeError::TransportClosed {
            transport: RuntimeTransportKind::Tcp,
            detail: "closed".to_string(),
        };

        assert_eq!(accept_retry_delay(&io), Some(Duration::from_millis(10)));
        assert_eq!(accept_retry_delay(&closed), None);
    }

    #[test]
    fn driver_failure_remains_a_case_result_with_dedicated_runner_evidence() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../wire-conformance/nnrp-1-preview4/cases/host-route-e2e.json");
        let manifest: WireConformanceScenarioManifest = load_json_file(&manifest_path).unwrap();
        let scenario = manifest.scenarios.first().unwrap();
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "test-suite".to_string(),
            target_name: "test-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: scenario.id.clone(),
                outcome: ApiProfileCaseOutcome::Failed,
                terminal: WireConformanceTerminal::Error,
                observed_frames: Vec::new(),
                route_evidence: None,
                message: Some("target diagnostic".to_string()),
                evidence_paths: Vec::new(),
            }],
        };

        let result = driver_result(report, scenario, "test-suite", "test-target").unwrap();

        assert_eq!(result.outcome, ApiProfileCaseOutcome::Failed);
        assert_eq!(result.message.as_deref(), Some("target diagnostic"));
        assert_eq!(
            runner_evidence_path(Path::new("case")),
            Path::new("case")
                .join("runner-evidence.jsonl")
                .display()
                .to_string()
        );
    }

    #[test]
    fn closure_probe_timeout_follows_the_scenario_deadline() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../wire-conformance/nnrp-1-preview4/cases/host-route-e2e.json");
        let manifest: WireConformanceScenarioManifest = load_json_file(&manifest_path).unwrap();

        for scenario in &manifest.scenarios {
            let declared_ms = scenario
                .steps
                .iter()
                .filter_map(|step| step.timeout_ms)
                .max()
                .unwrap_or(1_000);
            assert_eq!(
                closure_probe_timeout(scenario),
                Duration::from_millis(declared_ms.clamp(1_000, 5_000))
            );
        }
    }

    #[test]
    fn wire_timeout_reserves_external_driver_startup_budget() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../wire-conformance/nnrp-1-preview4/cases/host-route-e2e.json");
        let manifest: WireConformanceScenarioManifest = load_json_file(&manifest_path).unwrap();

        for scenario in &manifest.scenarios {
            let declared_ms = scenario
                .steps
                .iter()
                .filter_map(|step| step.timeout_ms)
                .max()
                .unwrap_or(0);
            assert_eq!(
                wire_timeout(scenario),
                Duration::from_millis(
                    declared_ms.saturating_add(HOST_ROUTE_DRIVER_STARTUP_GRACE_MS)
                )
            );
        }
    }
}
