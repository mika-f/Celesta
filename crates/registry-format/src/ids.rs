fn is_part(part: &str) -> bool {
    let mut chars = part.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Whether `id` is a dotless first-party name or `<publisher>.<name>`.
pub fn is_registry_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    parts.next().is_none() && is_part(first) && second.is_none_or(is_part)
}

/// The publisher of `id`; `"celesta"` for a dotless id.
pub fn publisher_of(id: &str) -> &str {
    id.split_once('.').map_or("celesta", |(p, _)| p)
}
