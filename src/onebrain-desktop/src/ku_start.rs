//! Stock Desktop provisioning: the same node owns Base KU and the shared API.
use crate::{
    config::DesktopConfig,
    supervisor::{HostNode, Supervisor},
};
use onebrain_node::OneBrainNode;
use std::sync::Arc;

pub async fn provision_local(
    config: DesktopConfig,
    supervisor: Arc<Supervisor>,
    token: String,
) -> Result<HostNode, &'static str> {
    let node_config = config.to_node_config();
    std::fs::create_dir_all(&node_config.data_dir).map_err(|_| "desktop_data_dir_unavailable")?;
    let mut node = OneBrainNode::new(node_config.clone())
        .await
        .map_err(|_| "desktop_node_init_failed")?;
    let mut base = onebrain_api::base_runtime_config_for_api_token(&token);
    let ku_issue = if let Some(ku) = &config.ku_host {
        match onebrain_api::ku_host::prepare_ku_runtime(&node_config, ku) {
            Ok((runtime, issue)) => {
                base.ku = Some(runtime);
                issue
            }
            Err(reason) => Some(reason),
        }
    } else {
        None
    };
    node.install_base_runtime(base)
        .map_err(|_| "desktop_base_init_failed")?;
    if config.auto_start && !supervisor.stopped() {
        node.start_network()
            .await
            .map_err(|_| "desktop_network_start_failed")?;
    }
    let mut host = HostNode::local(node);
    host.ku_issue = ku_issue;
    Ok(host)
}
