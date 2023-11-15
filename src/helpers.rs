use reqwest;
use serde_json::Value;
use std::error::Error;

pub fn get_actions(repo: &str, token: &str) -> Result<Vec<String>, Box<dyn Error>> {
    println!("Fetching actions for repository: {}", repo);
    let url = format!("https://api.github.com/repos/{}/actions/workflows", repo);

    // Create a client instance
    let client = reqwest::blocking::Client::new();

    // Perform the request with the Authorization header
    let response = client.get(&url)
        .header("User-Agent", "reqwest") // GitHub API requires a user-agent
        .header("Authorization", format!("Bearer {}", token))
        .send()?;

    let body = response.text()?;
    let json: Value = serde_json::from_str(&body)?;

    let mut actions = Vec::new();
    if let Some(workflows) = json["workflows"].as_array() {
        for workflow in workflows {
            if let Some(name) = workflow["name"].as_str() {
                actions.push(name.to_string());
            }
        }
    }

    Ok(actions)
}