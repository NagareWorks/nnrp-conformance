#[path = "../host_route_reference_target.rs"]
mod host_route_reference_target;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    host_route_reference_target::run(host_route_reference_target::SupportedHostRoles {
        client: true,
        server: false,
        label: "client",
    })
    .await
}
