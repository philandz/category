#![allow(clippy::result_large_err)]
use tonic::Status;

pub fn category_name(name: &str) -> Result<(), Status> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Status::invalid_argument("Category name must not be empty"));
    }
    if n.len() > 255 {
        return Err(Status::invalid_argument(
            "Category name must be 255 characters or fewer",
        ));
    }
    Ok(())
}

pub fn user_id_from_metadata(meta: &tonic::metadata::MetadataMap) -> Result<String, Status> {
    meta.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Status::unauthenticated("missing x-user-id metadata"))
}

pub fn user_type_from_metadata(meta: &tonic::metadata::MetadataMap) -> Option<String> {
    meta.get("x-user-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}
