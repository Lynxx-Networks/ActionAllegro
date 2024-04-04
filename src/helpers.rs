use std::collections::HashMap;
use reqwest;
use serde_json::{json, Value};
use std::error::Error;
use std::path::Path;
use git2;
use git2::{Commit, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use git2::build::RepoBuilder;
use std::sync::{Arc, Mutex};
use std::thread;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, LINK};

pub fn get_actions(repo: &str, token: &str) -> Result<HashMap<String, u64>, Box<dyn Error>> {
    println!("Fetching actions for repository: {}", repo);
    let base_url = format!("https://api.github.com/repos/{}/actions/workflows", repo);
    let client = Client::new();

    let mut actions = HashMap::new();
    let mut next_page_url = Some(base_url);

    while let Some(url) = next_page_url {
        // Reset for next iteration
        next_page_url = None;

        let response = client.get(&url)
            .header("User-Agent", "reqwest")
            .header("Authorization", format!("Bearer {}", token))
            .send()?;

        let headers = response.headers().clone();
        let body = response.text()?;
        let json: Value = serde_json::from_str(&body)?;

        if let Some(workflows) = json["workflows"].as_array() {
            for workflow in workflows {
                if let (Some(name), Some(id)) = (workflow["name"].as_str(), workflow["id"].as_u64()) {
                    actions.insert(name.to_string(), id);
                }
            }
        }

        // Check for a 'next' page link in the headers
        if let Some(link_header) = headers.get(LINK) {
            if let Ok(link_header_str) = link_header.to_str() {
                next_page_url = extract_next_page_url(link_header_str);
            }
        }
    }

    Ok(actions)
}

// Helper function to extract the 'next' page URL from the Link header
fn extract_next_page_url(link_header: &str) -> Option<String> {
    link_header.split(',')
        .find(|part| part.contains("rel=\"next\""))
        .and_then(|next_link_part| {
            let url_part = next_link_part.split(';').next()?;
            let url = url_part.trim().trim_start_matches('<').trim_end_matches('>');
            Some(url.to_string())
        })
}


pub fn get_workflow_details(repo: &str, token: &str, workflow_id: &Option<u64>) -> Result<Value, Box<dyn Error>> {
    match workflow_id {
        Some(id) => {
            println!("Fetching details for workflow: {:?}", id);
            let url = format!("https://api.github.com/repos/{}/actions/workflows/{:?}", repo, id);

            let client = reqwest::blocking::Client::new();
            let response = client.get(&url)
                .header("User-Agent", "reqwest")
                .header("Authorization", format!("Bearer {}", token))
                .header("Accept", "application/vnd.github+json")
                .send()?;

            let body = response.text()?;
            let json: Value = serde_json::from_str(&body)?;

            Ok(json)
        },
        None => {
            Err("Workflow ID is None".into())
        }
    }
}


pub fn get_repo(repo_slug: &str, api_key: &str, path: &Option<String>) -> Result<(), Box<dyn Error>> {
    println!("Processing repository: {}", repo_slug);
    let repo_url = format!("https://github.com/{}.git", repo_slug);
    println!("Repository URL: {}", &repo_url);

    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("dummy_username", api_key)
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);

    if let Some(ref path_str) = path {
        let path = Path::new(path_str);

        match Repository::open(&path) {
            Ok(repo) => {
                println!("Repository already exists, fetching updates...");
                let mut remote = repo.find_remote("origin")?;
                // Adjust branch name here if necessary
                remote.fetch(&["main"], Some(&mut fo), None)?;

                // Additional logic to merge fetched changes could be implemented here

                Ok(())
            },
            Err(_) => {
                println!("Repository does not exist, cloning...");
                let mut builder = RepoBuilder::new();
                builder.fetch_options(fo);
                match builder.clone(&*repo_url, path) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(Box::new(e)),
                }
            }
        }
    } else {
        Err("No path provided for processing the repository".into())
    }
}

pub fn get_repo_scratch(repo_slug: &str, api_key: &str, path: &Option<String>) -> Result<(), Box<dyn Error>> {
    println!("Processing repository: {}", repo_slug);
    let repo_url = format!("https://github.com/{}.git", repo_slug);
    println!("Repository URL: {}", &repo_url);

    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("dummy_username", api_key)
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);

    if let Some(ref path_str) = path {
        let path = Path::new(path_str);

        println!("Cloning repository to ensure fresh pull...");
        let mut builder = RepoBuilder::new();
        builder.fetch_options(fo);
        match builder.clone(&*repo_url, path) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    } else {
        Err("No path provided for processing the repository".into())
    }
}


pub fn push_repo(repo_path: &str, api_key: &str) -> Result<(), Box<dyn Error>> {
    println!("Pushing to repository at path: {}", repo_path);

    // Open the existing repository
    let repo = Repository::open(repo_path)?;
    let mut remote = repo.find_remote("origin")?;

    // Prepare authentication callbacks
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("dummy_username", api_key)
    });

    // Prepare push options with the callbacks
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    // Push changes
    // Assuming 'main' branch, adjust as necessary
    remote.push(&["refs/heads/main:refs/heads/main"], Some(&mut push_opts))?;

    Ok(())
}

pub fn find_last_commit(repo: &Repository) -> Result<Commit<'_>, Box<dyn Error>> {
    // First look up the HEAD of the repository
    let head = repo.head()?;

    // Then lookup the commit that HEAD points to
    let commit = head.peel_to_commit()?;
    Ok(commit)
}

pub fn pull_workflow_yaml(repo_slug: &str, api_key: &str, path: &Option<String>) -> Result<String, Box<dyn Error>> {
    println!("Pulling workflow YAML for repository: {}", repo_slug);
    println!("Using API key: {}", api_key);
    println!("Using path: {:?}", path);

    if let Some(workflow_path) = path {
        let repo_url = format!("https://api.github.com/repos/{}/contents/{}", repo_slug, workflow_path);
        let client = reqwest::blocking::Client::new();
        let response = client.get(&repo_url)
            .header("User-Agent", "reqwest")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()?;

        if response.status().is_success() {
            let content = response.json::<serde_json::Value>()?;
            if let Some(content_str) = content["content"].as_str() {
                // Remove newline and other whitespace characters
                let clean_content_str = content_str.replace("\n", "").replace("\r", "").trim().to_string();

                let decoded_content = base64::decode(&clean_content_str)?;
                let yaml_content = String::from_utf8(decoded_content)?;
                return Ok(yaml_content);
            }
        }

        Err("Failed to fetch or decode workflow YAML".into())
    } else {
        Err("Workflow path not provided".into())
    }
}

pub fn run_workflow(repo_slug: &str, api_key: &str, workflow_id: u64, inputs: Option<&HashMap<String, String>>) -> Result<(), Box<dyn Error>> {
    println!("Triggering workflow for repository: {}", repo_slug);

    let url = format!("https://api.github.com/repos/{}/actions/workflows/{}/dispatches", repo_slug, workflow_id);
    let client = reqwest::blocking::Client::new();

    // Prepare the JSON body
    let mut body = json!({
        "ref": "main" // Assuming 'main' branch, adjust as necessary
    });

    // Correctly structure the inputs within the body
    if let Some(inputs_data) = inputs {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("inputs".to_string(), json!(inputs_data));
        }
    }

    // Print the request body for debugging
    let request_body = serde_json::to_string_pretty(&body).unwrap_or_else(|_| "Failed to serialize request body".to_string());
    println!("Request body:\n{}", request_body);


    let response = client.post(&url)
        .header("User-Agent", "reqwest")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/vnd.github.v3+json")
        .json(&body)
        .send()?;

    if response.status().is_success() {
        Ok(())
    } else {
        let error_msg = response.text()?;
        Err(error_msg.into())
    }
}


pub fn fetch_pending_jobs(shared_result: Arc<Mutex<Option<Result<Vec<String>, String>>>>, api_key: String, action_listener_url: String) {
    // Existing code to perform the fetch operation...
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let response = client.get(&format!("{}/get-pending-jobs", action_listener_url))
        .header("X-API-KEY", api_key)
        .send();

    let result = match response {
        Ok(res) => res.json::<Vec<String>>().map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string())
    };

    let mut shared_data = shared_result.lock().unwrap();
    *shared_data = Some(result);
}

pub fn job_response(
    job_id: &str,
    user_decision: &str,
    api_key: &str,
    action_listener_url: &str,
) -> Result<(), String> {
    // Prepare the client and the request body
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let payload = json!({
        "decision": user_decision
    });

    // Send the POST request
    let response = client.post(&format!("{}/post-user-decision/{}", action_listener_url, job_id))
        .header("X-API-KEY", api_key)
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;

    // Process the response
    match response.status().is_success() {
        true => Ok(()),
        false => Err(format!("Failed to send user decision: {}", response.status())),
    }
}
