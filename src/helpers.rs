use std::collections::HashMap;
use reqwest;
use serde_json::{json, Value};
use std::error::Error;
use std::path::Path;
use egui::Id;
use git2;
use git2::{Commit, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use git2::build::RepoBuilder;

pub fn get_actions(repo: &str, token: &str) -> Result<HashMap<String, u64>, Box<dyn Error>> {
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

    let mut actions = HashMap::new();
    if let Some(workflows) = json["workflows"].as_array() {
        for workflow in workflows {
            if let (Some(name), Some(id)) = (workflow["name"].as_str(), workflow["id"].as_u64()) {
                actions.insert(name.to_string(), id);
            }
        }
    }

    Ok(actions)
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

pub fn find_last_commit(repo: &Repository) -> Result<Commit, Box<dyn Error>> {
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

// fn check_repo_status(&mut self) {
//     let repo_path = match self.config.repo_path.as_ref() {
//         Some(path) => path,
//         None => {
//             self.repo_status = RepoStatus::NotCloned;
//             return;
//         }
//     };
//
//     // Open the repository
//     match Repository::open(repo_path) {
//         Ok(repo) => {
//             let mut opts = StatusOptions::new();
//             opts.show(StatusShow::IndexAndWorkdir);
//             opts.include_untracked(true);
//             opts.renames_head_to_index(true);
//             opts.renames_index_to_workdir(true);
//
//             match repo.statuses(Some(&mut opts)) {
//                 Ok(statuses) => {
//                     if statuses.is_empty() {
//                         self.repo_status = RepoStatus::UpToDate;
//                     } else {
//                         self.repo_status = RepoStatus::ChangesMade;
//                     }
//                 }
//                 Err(e) => {
//                     eprintln!("Failed to retrieve repository statuses: {}", e);
//                     // Handle error appropriately
//                 }
//             }
//         }
//         Err(e) => {
//             eprintln!("Failed to open repository: {}", e);
//             // Handle error appropriately
//         }
//     }
// }