//! Binary item files over the bridge: base64 on the wire, sealed blobs in the vault, the
//! `path → blob` record in the item's source doc (`app_host::binary_files`).
use app_host::{BinaryFile, BoundFile, MAX_BINARY_FILE};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use osvauld_rpc::{BinaryFileInfo, Response};
use vault::Vault;

/// Refuses an oversize payload before allocating its decoded bytes.
pub(crate) fn decode(b64: &str) -> Result<Vec<u8>, String> {
    if b64.len() > MAX_BINARY_FILE.div_ceil(3) * 4 {
        return Err(format!("binary file over {MAX_BINARY_FILE} bytes"));
    }
    STANDARD.decode(b64).map_err(|e| format!("binary file is not base64: {e}"))
}

/// The store step [`app_host::bind_binary_file`] calls for new bytes.
pub(crate) fn store<'a>(
    vault: &'a Vault,
    ws: &'a str,
    item: &'a str,
) -> impl FnOnce(&[u8]) -> Result<String, String> + 'a {
    move |bytes| {
        let blob = Vault::new_blob_id();
        vault
            .put_blob(ws, item, &blob, bytes)
            .map_err(|e| e.to_string())?;
        Ok(blob)
    }
}

/// Persist a bind: the source snapshot, then drop the orphaned blob. A failed delete only
/// leaks sealed bytes; the write itself has succeeded.
pub(crate) fn finish(vault: &Vault, ws: &str, item: &str, bound: BoundFile) -> Response {
    if let Err(e) = vault.put_src(ws, item, &bound.snapshot) {
        return Response::err(e.to_string());
    }
    if let Some(old) = bound.orphan {
        let _ = vault.delete_blob(ws, item, &old);
    }
    Response::ok("written")
}

pub(crate) fn list(files: Vec<BinaryFile>) -> Response {
    Response::ok(
        files
            .into_iter()
            .map(|f| BinaryFileInfo {
                path: f.path,
                size: f.size,
                sha256: f.sha256,
            })
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn read(vault: &Vault, ws: &str, item: &str, file: Option<BinaryFile>) -> Response {
    let Some(file) = file else {
        return Response::err("no such binary file");
    };
    match vault.get_blob(ws, item, &file.blob) {
        Ok(Some(bytes)) => Response::ok(STANDARD.encode(bytes)),
        Ok(None) => Response::err(format!("{}: stored bytes are missing", file.path)),
        Err(e) => Response::err(e.to_string()),
    }
}
