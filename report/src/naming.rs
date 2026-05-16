//! NDI source-name filter and display helpers.

pub fn is_precog_source(name: &str) -> bool {
    name.starts_with("PRECOG-")
}

/// Strip the trailing " (Channel N)" suffix that NDI Find returns.
pub fn display_name(ndi_name: &str) -> &str {
    ndi_name.find(" (").map_or(ndi_name, |i| &ndi_name[..i])
}
