use reqwest;
use serde_json::Value;
use std::error::Error;
use std::path::Path;
use git2;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::RepoBuilder;

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

pub fn get_repo(repo_slug: &str, api_key: &str, path: &Option<String>) -> Result<(), Box<dyn Error>> {
    println!("Cloning repository: {}", repo_slug);
    println!("Using API key: {}", api_key);
    println!("Using path: {:?}", path);
    let repo_url = format!("https://github.com/{}.git", repo_slug);
    println!("Cloning repository: {}", &repo_url);
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("dummy_username", api_key)
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fo);

    // Check if path is Some and convert to Path
    if let Some(ref path_str) = path {
        let path = Path::new(path_str);

        match builder.clone(&*repo_url, path) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    } else {
        Err("No path provided for cloning the repository".into())
    }
}