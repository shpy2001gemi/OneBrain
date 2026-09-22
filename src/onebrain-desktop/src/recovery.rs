//! One bounded client-side recovery record. Never an operation dispatcher.
use std::{
    io::{Read, Write},
    path::Path,
};
const MAX_BYTES: usize = 1_048_576;

pub fn load(path: &Path) -> Result<Option<String>, &'static str> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("recovery_read_failed"),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "recovery_read_failed")?;
    if bytes.len() > MAX_BYTES {
        return Err("recovery_size_limit");
    }
    let text = String::from_utf8(bytes).map_err(|_| "recovery_invalid")?;
    validate(&text)?;
    Ok(Some(text))
}

#[cfg(feature = "vnext-outbound-first")]
fn validate(text: &str) -> Result<(), &'static str> {
    use onebrain_node::vnext_product_runtime::obp::contract;
    let validate = || {
        let record = contract::parse(text.as_bytes())?;
        contract::fields(&record, &["origin", "session", "operation", "payload"], &[])?;
        let origin = record["origin"].as_str().ok_or_else(contract_error)?;
        let url = reqwest::Url::parse(origin).map_err(|_| contract_error())?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || url.origin().ascii_serialization() != origin
        {
            return Err(contract_error());
        }
        contract::fields(
            &record["session"],
            &["process_generation", "dataset_generation"],
            &[],
        )?;
        contract::id(&record["session"]["process_generation"])?;
        contract::id(&record["session"]["dataset_generation"])?;
        let name = match record["operation"].as_str() {
            Some("configure") => "ObpConfigureV1",
            Some("source_admit") => "ObpSourceAdmitV1",
            Some("source_set_enabled") => "ObpSourceToggleV1",
            Some("refresh") => "ObpRefreshV1",
            Some("route_request") => "ObpRouteRequestV1",
            Some("intent_retry") => "ObpIntentRetryV1",
            Some("network_kill" | "network_reenable") => "ObpNetworkFenceV1",
            _ => return Err(contract_error()),
        };
        contract::validate(name, &record["payload"])
    };
    validate().map_err(|_| "recovery_invalid")
}
#[cfg(feature = "vnext-outbound-first")]
fn contract_error() -> onebrain_node::vnext_product_runtime::obp::Error {
    onebrain_node::vnext_product_runtime::obp::Error::invalid()
}
#[cfg(not(feature = "vnext-outbound-first"))]
fn validate(_: &str) -> Result<(), &'static str> {
    Err("obp_not_compiled")
}

pub fn save(path: &Path, text: &str) -> Result<(), &'static str> {
    if text.len() > MAX_BYTES {
        return Err("recovery_size_limit");
    }
    validate(text)?;
    if let Some(existing) = load(path)? {
        if existing == text {
            return Ok(());
        }
        return Err("recovery_unresolved_record_exists");
    }
    let parent = path.parent().ok_or("recovery_path_invalid")?;
    std::fs::create_dir_all(parent).map_err(|_| "recovery_write_failed")?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "recovery_write_failed")?;
    temporary
        .write_all(text.as_bytes())
        .map_err(|_| "recovery_write_failed")?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| "recovery_write_failed")?;
    temporary
        .persist_noclobber(path)
        .map_err(|_| "recovery_write_failed")?;
    Ok(())
}

pub fn clear(path: &Path) -> Result<(), &'static str> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("recovery_clear_failed"),
    }
}
