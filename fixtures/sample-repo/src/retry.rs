pub fn retry_request(attempts: usize) -> bool {
    attempts < 3
}
