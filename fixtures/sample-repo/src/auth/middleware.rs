pub fn validate_access_token(token: &str) -> bool {
    !token.is_empty() && token.starts_with("Bearer ")
}
