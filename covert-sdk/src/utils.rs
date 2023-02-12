pub(crate) fn get_mount_path(mount: &str, path: &str) -> String {
    let mut mount = mount.to_string();
    if !mount.starts_with('/') {
        mount = format!("/{mount}");
    }
    if !mount.ends_with('/') {
        mount = format!("{mount}/");
    }
    format!("{mount}{path}")
}
