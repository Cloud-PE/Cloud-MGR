#[allow(dead_code)]
pub async fn check_network() -> bool {
    let client = reqwest::Client::new();
    match client
        .get("https://api.cloud-pe.cn/Hub/connecttest/")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}