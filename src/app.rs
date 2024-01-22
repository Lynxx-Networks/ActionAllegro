use std::collections::HashMap;
use crate::helpers::{find_last_commit, get_actions, get_repo, get_workflow_details, pull_workflow_yaml, push_repo, run_workflow};
use egui::{TextStyle, Sense, CursorIcon, Order, LayerId, Rect, Shape, Vec2, Id, InnerResponse, Ui, epaint};
use std::fs;
use serde_json;
use rfd::FileDialog;
use git2::{Repository, StatusOptions, StatusShow};
use serde_yaml::Value;
use directories::ProjectDirs;
use std::time::{Duration, Instant};
use rand::{distributions::Alphanumeric, Rng}; // for generating a salt
use sha2::{Sha256, Digest}; // for hashing the password
use std::fmt::Write;
use aes::Aes128;
use cbc::{Encryptor as Aes128CbcEnc, Decryptor as Aes128CbcDec};
use block_padding::Pkcs7;
use aes::cipher::{KeyIvInit, BlockEncryptMut, BlockDecryptMut};
use hmac::Hmac;
use pbkdf2::pbkdf2;
// fn derive_key(password: &[u8], output: &mut [u8]) {
//     let pbkdf2_iterations = 100_000; // Number of iterations, adjust as needed
//     let salt = b"some-fixed-salt"; // Ideally, use a fixed salt
//
//     pbkdf2::<Hmac<Sha256>>(password, salt, pbkdf2_iterations, output);
// }

fn derive_key_iv(password: &[u8]) -> ([u8; 16], [u8; 16]) {
    let pbkdf2_iterations = 100_000; // Number of iterations, adjust as needed
    let salt = b"some-fixed-salt"; // Ideally, use a fixed salt

    let mut key_iv = [0u8; 32]; // 32 bytes for key, 16 for IV
    pbkdf2::<Hmac<Sha256>>(password, salt, pbkdf2_iterations, &mut key_iv);

    let mut key = [0u8; 16];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&key_iv[..16]);
    iv.copy_from_slice(&key_iv[16..]);

    (key, iv)
}

// Encryption function using the derived key

fn encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let cipher = Aes128CbcEnc::<Aes128>::new_from_slices(key, iv)
        .expect("Invalid key/IV length");

    cipher.encrypt_padded_vec_mut::<Pkcs7>(data)
}


fn decrypt(encrypted_data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, &'static str> {
    let cipher = Aes128CbcDec::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| "Invalid key/IV length")?;

    cipher.decrypt_padded_vec_mut::<Pkcs7>(encrypted_data)
        .map_err(|_| "Decryption failed")
}

type JsonValue = serde_json::Value;
type YamlValue = serde_yaml::Value;


#[derive(serde::Deserialize, serde::Serialize, PartialEq)]
enum AppTab {
    Organize,
    Pull,
    Confirm,
    // Add more tabs as needed
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
enum RepoStatus {
    NotCloned,
    UpToDate,
    ChangesMade,
    // ... other statuses as needed ...
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct WorkflowInput {
    pub input_type: String,
    pub description: String,
    pub required: bool,
    pub options: Option<Vec<String>>,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,
    label2: String,
    name: String,
    salt: Option<String>,
    hashed_password: Option<String>,
    temp_password: String,
    first_launch: bool,
    actions: HashMap<String, u64>, // Store the names of GitHub Actions here
    display_actions: bool, // Flag to indicate whether to display actions
    error_message: Option<String>,
    info_message: Option<String>,
    columns: Vec<Vec<String>>,
    folders: HashMap<String, Vec<String>>,
    selected_folder: Option<String>,
    new_folder_name: String,
    dragged_action: Option<String>,
    drop_target_folder: Option<String>,
    actions_fetched: bool, // Add this new field
    current_tab: AppTab,
    config: AppConfig,
    repo_status: RepoStatus,
    #[serde(skip)]
    last_save_time: Instant,
    #[serde(skip)]
    auto_save_interval: Duration,
    #[serde(skip)]
    last_git_time: Instant,
    #[serde(skip)]
    auto_git_interval: Duration,
    action_detail_window_open: Option<String>,
    opened_action_id: Option<u64>,
    opened_workflow_details: Option<String>,
    parsed_workflow_yaml: Option<Value>,
    active_workflow_type: Option<String>,
    active_workflow_inputs: Option<HashMap<String, WorkflowInput>>,
    input_text: String,
    input_descriptions: HashMap<String, String>,
    selected_option: String,
    current_input_values: HashMap<String, String>,
    commit_message: String,
    show_commit_message_input: bool,
    config_dir: Option<String>,
    needs_password_verification: bool,
    password_attempt: String,
    encoded_github_pat: String,
    decrypted_github_pat: String,
    repo_path: Option<String>,


    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct AppConfig {
    // Include all the fields that make up your application's state
    folders: HashMap<String, Vec<String>>,
    repo_name: String,
    github_pat: String,
    repo_path: Option<String>,
    name: String,
    salt: Option<String>,
    hashed_password: Option<String>,
}

fn pick_file_location() -> Option<String> {
    FileDialog::new()
        .save_file()
        .map(|path| path.to_string_lossy().into_owned())
}

fn pick_folder_location() -> Option<String> {
    FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

fn label_ui(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        // Trick so we don't have to add spaces in the text below:
        let width = ui.fonts(|f|f.glyph_width(&TextStyle::Body.resolve(ui.style()), ' '));
        ui.spacing_mut().item_spacing.x = width;
        ui.label("Welcome to Actions Organizer! This little tool will help you organize your GitHub Actions using a traditional folder layout while allowing you to save your configuration to keep things nice and tidy. You can also run actions from this tool and then get direct links to view the status and logs of the workflows.");
    });
}

pub fn drag_source(ui: &mut Ui, id: Id, body: impl FnOnce(&mut Ui)) {
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));

    if !is_being_dragged {
        let response = ui.scope(body).response;

        // Check for drags:
        let response = ui.interact(response.rect, id, Sense::drag());
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    } else {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);

        // Paint the body to a new layer:
        let layer_id = LayerId::new(Order::Tooltip, id);
        let response = ui.with_layer_id(layer_id, body).response;

        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
            let delta = pointer_pos - response.rect.center();
            ui.ctx().translate_layer(layer_id, delta);
        }
    }
}

pub fn drop_target<R>(
    ui: &mut Ui,
    can_accept_what_is_being_dragged: bool,
    body: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let is_being_dragged = ui.memory(|mem| mem.is_anything_being_dragged());

    let margin = Vec2::splat(4.0);

    let outer_rect_bounds = ui.available_rect_before_wrap();
    let inner_rect = outer_rect_bounds.shrink2(margin);
    let where_to_put_background = ui.painter().add(Shape::Noop);
    let mut content_ui = ui.child_ui(inner_rect, *ui.layout());
    let ret = body(&mut content_ui);
    let outer_rect = Rect::from_min_max(outer_rect_bounds.min, content_ui.min_rect().max + margin);
    let (rect, response) = ui.allocate_at_least(outer_rect.size(), Sense::hover());

    let style = if is_being_dragged && can_accept_what_is_being_dragged && response.hovered() {
        ui.visuals().widgets.active
    } else {
        ui.visuals().widgets.inactive
    };

    let mut fill = style.bg_fill;
    let mut stroke = style.bg_stroke;
    if is_being_dragged && !can_accept_what_is_being_dragged {
        fill = ui.visuals().gray_out(fill);
        stroke.color = ui.visuals().gray_out(stroke.color);
    }

    ui.painter().set(
        where_to_put_background,
        epaint::RectShape::new(rect, style.rounding, fill, stroke),
    );

    InnerResponse::new(ret, response)
}

impl Default for TemplateApp {
    fn default() -> Self {
        let mut folders = HashMap::new();
        folders.insert("Test Folder 1".to_owned(), vec!["Action 1".to_owned(), "Action 2".to_owned()]);
        folders.insert("Test Folder 2".to_owned(), vec!["Action 3".to_owned(), "Action 4".to_owned()]);
        let config_dir = ProjectDirs::from("com", "3rtNetworks", "ActionAllegro")
            .map(|proj_dirs| proj_dirs.config_dir().to_path_buf().display().to_string());

        Self {
            label: "myuser/myrepo".to_owned(),
            label2: "ghp_sdfjkh238hdsklsdjf983nldfejfds".to_owned(),
            value: 2.7,
            actions: HashMap::new(), // Store the names of GitHub Actions here
            display_actions: false, // Flag to indicate whether to display actions
            error_message: None,
            info_message: None,
            current_tab: AppTab::Organize,
            folders: HashMap::new(),
            selected_folder: None,
            new_folder_name: String::new(),
            dragged_action: None,
            drop_target_folder: None,
            actions_fetched: false,
            repo_status: RepoStatus::NotCloned,
            config: AppConfig {
                folders: HashMap::new(),
                repo_name: String::new(),
                github_pat: String::new(),
                repo_path: None, // Initialize as None
                name: String::new(),
                salt: None,
                hashed_password: None,
                // ... initialize other fields ...
            },
            action_detail_window_open: None,
            opened_action_id: None,
            last_save_time: Instant::now(),
            auto_save_interval: Duration::from_secs(30),
            last_git_time: Instant::now(),
            auto_git_interval: Duration::from_secs(2),
            opened_workflow_details: None,
            parsed_workflow_yaml: None,
            selected_option: String::new(),
            first_launch: true,
            input_text: String::new(),
            input_descriptions: HashMap::new(),
            active_workflow_type: Option::from(String::new()),
            active_workflow_inputs: Option::from(HashMap::new()),
            current_input_values: HashMap::new(),
            commit_message: String::new(),
            name: String::new(),
            salt: None,
            hashed_password: None,
            needs_password_verification: true,
            password_attempt: String::new(),
            temp_password: String::new(),
            config_dir,
            show_commit_message_input: false,
            encoded_github_pat: String::new(),
            decrypted_github_pat: String::new(),
            repo_path: None,
            columns: vec![
                vec!["Item A", "Item B", "Item C"],
                vec!["Item D", "Item E"],
            ].into_iter()
             .map(|v| v.into_iter().map(ToString::to_string).collect())
             .collect(),
        }
    }
}

impl TemplateApp {

    /// Called once before the first frame.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Initialize with default values
        let mut app = Self::default();

        // Determine the path of the config file
        if let Some(config_dir) = &app.config_dir {
            let config_path = Path::new(config_dir);
            let file_path = config_path.join("config.json");

            // Check if the config file exists
            if file_path.exists() {
                app.import_config(); // Load the configuration
            }
            app.first_launch = !file_path.exists();
        }

        // Return the initialized app
        app
    }

    fn verify_password(&self, attempt: &str) -> bool {
        if let (Some(ref salt), Some(ref hashed_password)) = (&self.config.salt, &self.config.hashed_password) {
            let mut hasher = Sha256::new();
            hasher.update(salt);
            hasher.update(attempt);
            let hash_result = hasher.finalize();

            let mut attempt_hashed = String::new();
            for byte in hash_result.iter() {
                write!(attempt_hashed, "{:02x}", byte).expect("Failed to write to string");
            }

            attempt_hashed == *hashed_password
        } else {
            false
        }
    }

    fn check_repo_status(&mut self) {
        let repo_path = match self.config.repo_path.as_ref() {
            Some(path) => path,
            None => {
                self.repo_status = RepoStatus::NotCloned;
                return;
            }
        };

        // Open the repository
        match Repository::open(repo_path) {
            Ok(repo) => {
                let mut opts = StatusOptions::new();
                opts.show(StatusShow::IndexAndWorkdir);
                opts.include_untracked(true);
                opts.renames_head_to_index(true);
                opts.renames_index_to_workdir(true);

                match repo.statuses(Some(&mut opts)) {
                    Ok(statuses) => {
                        if statuses.is_empty() {
                            self.repo_status = RepoStatus::UpToDate;
                        } else {
                            self.repo_status = RepoStatus::ChangesMade;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to retrieve repository statuses: {}", e);
                        // Handle error appropriately
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to open repository: {}", e);
                // Handle error appropriately
            }
        }
    }

    fn handle_commit_and_push(&mut self) {
        if let Some(ref repo_path) = self.config.repo_path {
            match Repository::open(repo_path) {
                Ok(repo) => {
                    // Step 1: Check if there are changes
                    let statuses = repo.statuses(None).unwrap(); // Handle this unwrap properly
                    if statuses.is_empty() {
                        self.info_message = Some("No changes to upload".to_string());
                    } else {
// Step 2: Stage changes
                        let mut index = repo.index().unwrap(); // Handle this unwrap properly
                        for entry in statuses.iter() {
                            let path_str = entry.path().unwrap(); // Handle this unwrap properly
                            let path = Path::new(path_str);
                            let status = entry.status();

                            if status.is_index_deleted() || status.is_wt_deleted() {
                                // Handle file deletion
                                index.remove_path(path).unwrap(); // Handle this unwrap properly
                            } else if status.is_index_new() || status.is_wt_new() {
                                // Handle new files and directories
                                if path.is_dir() {
                                    // If the path is a directory, add all files in the directory
                                    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                                        let file_path = entry.path();
                                        if file_path.is_file() {
                                            index.add_path(file_path.strip_prefix(path.parent().unwrap_or_else(|| Path::new(""))).unwrap()).unwrap(); // Handle this unwrap properly
                                        }
                                    }
                                } else {
                                    // If the path is a file, just add it
                                    index.add_path(path).unwrap(); // Handle this unwrap properly
                                }
                            } else {
                                // Handle modified files
                                index.add_path(path).unwrap(); // Handle this unwrap properly
                            }
                        }
                        index.write().unwrap(); // Handle this unwrap properly

                        // Step 3: Commit changes
                        let oid = index.write_tree().unwrap(); // Handle this unwrap properly
                        let signature = repo.signature().unwrap(); // Handle this unwrap properly
                        let parent_commit = find_last_commit(&repo).unwrap(); // Function to find the last commit
                        let tree = repo.find_tree(oid).unwrap(); // Handle this unwrap properly
                        let full_commit_message = format!("{}: {}\n\nThis commit originated from ActionAllegro", self.config.name, self.commit_message);
                        repo.commit(Some("HEAD"), &signature, &signature, &full_commit_message, &tree, &[&parent_commit]).unwrap(); // Handle this unwrap properly

                        // Step 4: Push the commit
                        if let Err(e) = push_repo(repo_path, &self.decrypted_github_pat) {
                            self.error_message = Some(format!("Failed to push changes: {}", e));
                            self.check_repo_status();
                        } else {
                            self.info_message = Some("Changes uploaded successfully".to_string());
                            self.check_repo_status();
                        }
                    }
                },
                Err(e) => self.error_message = Some(format!("Failed to open repository: {}", e)),
            }
        } else {
            self.error_message = Some("Repository path is not set".to_string());
        }
    }


    fn show_action_details_window(&mut self, ctx: &egui::Context) {
        if let Some(_action) = &self.action_detail_window_open {
            // Check if the workflow details are already fetched
            let mut window_title = "Action Details".to_string();
            if self.opened_workflow_details.is_none() {
                // Check if there is an opened action ID
                if let Some(action_id) = self.opened_action_id {
                    match get_workflow_details(&self.config.repo_name, &self.decrypted_github_pat, &Some(action_id)) {
                        Ok(workflow_details) => {
                            let workflow_details_str = workflow_details.to_string();
                            println!("Fetched details for workflow: {}", workflow_details_str);
                            self.opened_workflow_details = Some(workflow_details_str.clone());

                            // Parse the JSON to get the workflow file path
                            if let Some(ref details_str) = self.opened_workflow_details {
                                match serde_json::from_str::<JsonValue>(details_str) {
                                    Ok(workflow_details) => {
                                        // Proceed with extracting the workflow file path
                                        if let Some(path) = workflow_details["path"].as_str() {
                                            match pull_workflow_yaml(&self.config.repo_name, &self.decrypted_github_pat, &Some(path.to_string())) {
                                                Ok(yaml_content) => {
                                                    match serde_yaml::from_str::<YamlValue>(&yaml_content) {
                                                        Ok(parsed_yaml) => {
                                                            // Process the parsed YAML content
                                                            if let Some(triggers) = parsed_yaml.get("on").and_then(|on| on.as_mapping()) {
                                                                for (trigger_type, details) in triggers {
                                                                    if trigger_type.as_str() == Some("workflow_dispatch") {
                                                                        self.active_workflow_type = Some("workflow_dispatch".to_string());

                                                                        // Process inputs if available
                                                                        if let Some(inputs) = details.get("inputs").and_then(|i| i.as_mapping()) {
                                                                            let mut inputs_map = HashMap::new();
                                                                            // let description = details.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string();
                                                                            for (input_name, input_details) in inputs {
                                                                                if let Some(input_name_str) = input_name.as_str() {
                                                                                    let input_description = input_details.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string();
                                                                                    let input = WorkflowInput {
                                                                                        input_type: input_details.get("type").and_then(|t| t.as_str()).unwrap_or("string").to_string(),
                                                                                        description: input_description.clone(),
                                                                                        required: input_details.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
                                                                                        options: Some(input_details.get("options").and_then(|o| o.as_sequence()).map_or_else(Vec::new, |opts| opts.iter().filter_map(|opt| opt.as_str()).map(|s| s.to_string()).collect())),
                                                                                    };
                                                                                    inputs_map.insert(input_name_str.to_string(), input);
                                                                                    self.input_descriptions.insert(input_name_str.to_string(), input_description.clone()); // Corrected placement
                                                                                }
                                                                            }
                                                                            self.active_workflow_inputs = Some(inputs_map);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        Err(e) => {
                                                            self.error_message = Some(format!("YAML parsing error: {}", e));
                                                            return;
                                                        }
                                                    }
                                                },
                                                Err(e) => {
                                                    self.error_message = Some(format!("Error pulling YAML: {}", e));
                                                    return;
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        // Handle the error appropriately
                                        self.error_message = Some(format!("JSON parsing error: {}", e));
                                        return;
                                    }
                                }
                                if let Some(ref details_str) = self.opened_workflow_details {
                                    match serde_json::from_str::<JsonValue>(details_str) {
                                        Ok(workflow_details) => {
                                            // Proceed with extracting the workflow file path
                                            if let Some(_path) = workflow_details["path"].as_str() {
                                                // ... rest of your code ...
                                            }
                                        },
                                        Err(e) => {
                                            self.error_message = Some(format!("JSON parsing error: {}", e));
                                            return;
                                        }
                                    }
                                }

                            }
                        },
                        Err(e) => self.error_message = Some(format!("Error: {}", e)),
                    }
                }
            }

            // Parse the workflow details for the window title
            if let Some(ref details_str) = self.opened_workflow_details {
                if let Ok(workflow_details) = serde_json::from_str::<serde_json::Value>(details_str) {
                    if let Some(name) = workflow_details["name"].as_str() {
                        window_title = name.to_string();
                    }
                }
            }

            let mut is_window_open = true;
            egui::Window::new(window_title)
                .open(&mut is_window_open)
                .show(ctx, |ui| {
                    if let Some(_details) = &self.opened_workflow_details {
                        if let Some(ref details_str) = self.opened_workflow_details {
                            match serde_json::from_str::<serde_json::Value>(details_str) {
                                Ok(workflow_details) => {
                                    // Display workflow name as a header
                                    if let Some(name) = workflow_details["name"].as_str() {
                                        ui.heading(name);
                                    }

                                    // Display clickable URL
                                    if let Some(html_url) = workflow_details["html_url"].as_str() {
                                        if ui.hyperlink_to("Link to workflow", html_url).clicked() {
                                            // Handle the click event, if needed
                                        }
                                    }
                                    // Create and display the link to the workflow logs
                                    if let Some(path) = workflow_details["path"].as_str() {
                                        let log_url = format!("https://github.com/{}/actions/workflows/{}", self.config.repo_name, path.clone());
                                        if ui.hyperlink_to("View Workflow Logs", log_url).clicked() {
                                            // Handle the click event, if needed
                                        }
                                    }

                                    // Display other details
                                    if let Some(path) = workflow_details["path"].as_str() {
                                        ui.label(format!("Path: {}", path));
                                    }
                                    if let Some(created_at) = workflow_details["created_at"].as_str() {
                                        ui.label(format!("Created at: {}", created_at));
                                    }
                                    if let Some(state) = workflow_details["state"].as_str() {
                                        ui.label(format!("State: {}", state));
                                    }
                                    ui.separator();

                                    // ... rest of your code for extracting the workflow file path ...
                                },
                                Err(e) => {
                                    self.error_message = Some(format!("JSON parsing error: {}", e));
                                    return;
                                }
                            }
                        }
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(inputs) = &self.active_workflow_inputs {
                                for (input_name, input_details) in inputs {
                                    ui.horizontal(|ui| {
                                        ui.push_id(input_name, |ui| { // Ensure each input has a unique ID
                                            // Vertical layout for the label (description) and the input field
                                            ui.vertical(|ui| {
                                                // Display the description as a label above the input
                                                // Display the variable name
                                                ui.label(input_name);
                                                if let Some(description) = self.input_descriptions.get(input_name) {
                                                    ui.label(description);
                                                }

                                                match input_details.input_type.as_str() {
                                                    "choice" => {
                                                        if let Some(options) = &input_details.options {
                                                            let current_value = self.current_input_values.entry(input_name.clone()).or_insert_with(|| options.get(0).cloned().unwrap_or_default());
                                                            // Create a dropdown for choice type
                                                            egui::ComboBox::from_label("")
                                                                .selected_text(current_value.clone())
                                                                .show_ui(ui, |ui| {
                                                                    for option in options {
                                                                        ui.selectable_value(current_value, option.clone(), option);
                                                                    }
                                                                });
                                                        }
                                                    }
                                                    _ => {
                                                        // Create a text box for string type
                                                        let current_value = self.current_input_values.entry(input_name.clone()).or_insert_with(String::new);
                                                        ui.text_edit_singleline(current_value);
                                                    }
                                                }
                                            });
                                        });
                                        // workflow_inputs_data.insert(input_name.clone(), input_value);
                                    });
                                }
                                if ui.button("Run Workflow").clicked() {
                                    let workflow_id = self.opened_action_id.unwrap(); // Make sure to handle unwrap properly
                                    let result = run_workflow(&self.config.repo_name, &self.decrypted_github_pat, workflow_id, Some(&self.current_input_values));
                                    match result {
                                        Ok(_) => self.info_message = Some(("Workflow triggered successfully").parse().unwrap()),
                                        Err(e) => self.error_message = Some(format!("Failed to trigger workflow: {}", e)),
                                    }
                                }
                            }
                        });
                    } else {
                        ui.label("Fetching workflow details...");
                    }
                });

            // Reset the details when the window is closed
            if !is_window_open {
                self.action_detail_window_open = None;
                self.opened_workflow_details = None;
                self.opened_action_id = None;
                self.active_workflow_type = None;
                self.active_workflow_inputs = None;
            }
        }
    }

    fn import_config(&mut self) {
        if let Some(config_dir) = &self.config_dir {
            let config_path = Path::new(config_dir);
            let file_path = config_path.join("config.json");

            match std::fs::read_to_string(&file_path) {
                Ok(json_string) => {
                    match serde_json::from_str::<AppConfig>(&json_string) {
                        Ok(config) => {
                            println!("Config imported from {:?}", file_path);

                            // Store the configuration and encoded GitHub PAT
                            self.config = config;
                            self.encoded_github_pat = self.config.github_pat.clone();
                            println!("encoded_github_pat: {}", self.encoded_github_pat);

                            // Update other fields of TemplateApp based on imported config
                            self.label = self.config.repo_name.clone();
                            self.name = self.config.name.clone();
                            self.folders = self.config.folders.clone();
                            self.salt = self.config.salt.clone();
                            self.hashed_password = self.config.hashed_password.clone();
                            self.repo_path = self.config.repo_path.clone();
                        },
                        Err(e) => println!("Failed to deserialize config: {}", e),
                    }
                }
                Err(e) => println!("Failed to read config from file: {:?}, error: {}", file_path, e),
            }
        } else {
            println!("Config directory path is not set.");
        }
    }



    fn export_config(&mut self) {
        println!("Running Export.");
        if let Some(ref config_dir) = self.config_dir {
            let config_path = Path::new(config_dir);
            // Ensure the directory exists
            if let Err(e) = fs::create_dir_all(config_path) {
                println!("Failed to create config directory: {}", e);
                return;
            }

            let file_path = config_path.join("config.json");
            self.config.github_pat.clear();

            // Derive key and IV from the temporary password
            let (key, iv) = derive_key_iv(self.temp_password.as_bytes());
            println!("Derived key: {:?}, IV: {:?}", key, iv);
            println!("GitHub PAT: {}", self.decrypted_github_pat);
            let encrypted_github_pat = encrypt(self.decrypted_github_pat.as_bytes(), &key, &iv);
            println!("Encrypted GitHub PAT: {:?}", encrypted_github_pat);
            println!("test on label: {}", self.config.repo_name);
            println!("temp_password: {}", self.temp_password);
            println!("repo path: {:?}", self.config.repo_path.clone());

            let config = AppConfig {
                folders: self.folders.clone(),
                repo_name: self.config.repo_name.clone(),
                github_pat: base64::encode(&encrypted_github_pat),
                repo_path: self.config.repo_path.clone(),
                name: self.name.clone(),
                salt: self.salt.clone(),
                hashed_password: self.hashed_password.clone(),
                // ... other fields ...
            };
            println!("test on name: {:?}", self.config);
            match serde_json::to_string(&config) {
                Ok(json_string) => {
                    if let Err(e) = std::fs::write(file_path.clone(), json_string) {
                        println!("Failed to write config to file: {}", e);
                    } else {
                        println!("Config exported to {:?}", file_path);
                        // self.config.repo_path = Some(file_path.into_os_string().into_string().unwrap()); // Save the repo path
                        println!("repo path 2: {:?}", self.config.repo_path);
                    }
                },
                Err(e) => println!("Failed to serialize config: {}", e),
            }
        }
    }

}
use std::cell::RefCell;
use std::path::Path;

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.needs_password_verification && !self.first_launch {
            egui::Window::new("Password Required")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Please enter your password to continue:");
                    let password_response = ui.add(egui::TextEdit::singleline(&mut self.temp_password).password(true));

                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if password_response.lost_focus() && enter_pressed {
                        // Verify the password
                        if self.verify_password(&self.temp_password) {
                            self.needs_password_verification = false;
                            if let Ok(decoded_github_pat) = base64::decode(&self.config.github_pat) {
                                // Derive key and IV using the correct method
                                let (key, iv) = derive_key_iv(self.temp_password.as_bytes());
                                if let Ok(decrypted_github_pat) = decrypt(&decoded_github_pat, &key, &iv) {
                                    self.decrypted_github_pat = String::from_utf8_lossy(&decrypted_github_pat).to_string();
                                    println!("Decrypted GitHub PAT: {}", self.decrypted_github_pat)
                                } else {
                                    println!("Failed to decrypt GitHub PAT");
                                }
                            } else {
                                println!("Failed to decode GitHub PAT");
                            }
                            // Load configuration and proceed
                        } else {
                            ui.label("Incorrect password. Please try again.");
                        }
                    }
                });
        }
        else if self.first_launch {
            egui::Window::new("Welcome to ActionAllegro!")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("This application strives to do what GitHub can't be bothered to do. It solves a problem of organization, ease of use, and barrier to entry. We'll start with some basic housework. Please enter a name to commit with and a password for this application");
                    ui.label("Please enter your name:");
                    ui.text_edit_singleline(&mut self.name); // Input for the name
                    ui.label("Please enter your password:");
                    let password_response = ui.add(egui::TextEdit::singleline(&mut self.temp_password).password(true));
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if password_response.lost_focus() && enter_pressed && !self.name.is_empty() && !self.temp_password.is_empty() {
                        self.first_launch = false;
                        self.config.name = self.name.clone();

                        // Generate a salt
                        let salt: String = rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(16)
                            .map(char::from)
                            .collect();

                        // Hash the password with the salt
                        let mut hasher = Sha256::new();
                        hasher.update(&salt);
                        hasher.update(&self.temp_password);
                        let hash_result = hasher.finalize();

                        // Convert the hash result to a hexadecimal string
                        let mut hashed_password = String::new();
                        for byte in hash_result.iter() {
                            write!(hashed_password, "{:02x}", byte).expect("Failed to write to string");
                        }
                        println!("PW: {}", &hashed_password);
                        // Store the salt and hashed password in the AppConfig
                        self.salt = Some(salt);
                        self.hashed_password = Some(hashed_password);

                        println!("Config: {:?}", self.config);

                        // Save the config
                        self.export_config();

                        // self.temp_password.clear();
                        self.needs_password_verification = false;
                    }
                });
        } else {
            self.show_action_details_window(ctx);

            egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
                // The top panel is often a good place for a menu bar:

                egui::menu::bar(ui, |ui| {
                    #[cfg(not(target_arch = "wasm32"))] // no File->Quit on web pages!
                    {
                        ui.menu_button("File", |ui| {
                            if ui.button("Quit").clicked() {
                                _frame.close();
                            }
                        });
                        ui.add_space(16.0);
                    }

                    egui::widgets::global_dark_light_mode_buttons(ui);
                });
            });

            let dropped_action = RefCell::new(None); // Use RefCell for interior mutability
            // println!("last save: {:?}", self.last_save_time)
            if self.last_save_time.elapsed() >= self.auto_save_interval {
                self.export_config(); // Call your export function
                self.last_save_time = Instant::now(); // Reset the timer
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                // Intro and repository info at the top
                let can_export = !self.label.is_empty(); // 'true' if a repo is loaded

                // Horizontal layout for heading and button
                ui.horizontal(|ui| {
                    ui.heading("ActionAllegro");
                    // Enable button if a repository is loaded
                    if ui.add_enabled(can_export, egui::Button::new("Export Config")).clicked() {
                        self.export_config();
                    }
                });

                // Show message when no repository is loaded (i.e., label is empty)
                if !can_export {
                    ui.label("Load a repository to enable configuration export.");
                }


                ui.horizontal(|ui| {
                    ui.label("What is your Repository name?: ");
                    ui.text_edit_singleline(&mut self.config.repo_name);
                });
                ui.horizontal(|ui| {
                    ui.label("What is your Github API Key?: ");
                    ui.add(egui::TextEdit::singleline(&mut self.decrypted_github_pat).password(true));
                });
                if ui.button("Fetch Actions").clicked() {
                    self.error_message = None;
                    match get_actions(&self.config.repo_name, &self.decrypted_github_pat) {
                        Ok(actions) => {
                            println!("Fetched {} actions", actions.len());
                            println!("Actions: {:?}", actions.keys());

                            // Update self.actions
                            self.actions = actions.clone();

                            // Update folders with action names only
                            let action_names = actions.keys().cloned().collect::<Vec<String>>();
                            self.folders.insert("/".to_owned(), action_names);

                            self.selected_folder = Some("/".to_owned());
                            println!("Reached OK");
                            self.display_actions = true;
                            self.export_config()
                        }
                        Err(err) => {
                            println!("Error occurred");
                            self.error_message = Some(format!("Error: {}", err));
                        }
                    }
                }


                if let Some(error_msg) = &self.error_message {
                    ui.colored_label(egui::Color32::RED, error_msg);
                }
                if let Some(error_msg) = &self.info_message {
                    ui.colored_label(egui::Color32::GREEN, error_msg);
                }

                ui.horizontal(|ui| {
                    // Tab for "Organize and Run"
                    if ui.selectable_label(self.current_tab == AppTab::Organize, "Organize and Run").clicked() {
                        self.current_tab = AppTab::Organize;
                    }

                    // Tab for "Pull and Upload"
                    if ui.selectable_label(self.current_tab == AppTab::Pull, "Pull and Upload").clicked() {
                        self.check_repo_status();
                        self.current_tab = AppTab::Pull;
                    }

                    // Tab for "Confirm Changes"
                    if ui.selectable_label(self.current_tab == AppTab::Confirm, "Confirm Changes").clicked() {
                        self.check_repo_status();
                        self.current_tab = AppTab::Confirm;
                    }
                    // Add more tabs as needed
                });

                // Step 4: Render content based on selected tab
                match self.current_tab {
                    AppTab::Organize => {
                        ui.separator(); // Separate the top elements from the panels below
                        // UI for adding a new folder
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.new_folder_name);
                            if ui.button("Add New Folder").clicked() {
                                if !self.new_folder_name.is_empty() {
                                    self.folders.insert(self.new_folder_name.clone(), Vec::new());
                                    self.new_folder_name.clear(); // Clear the input field after adding
                                }
                            }
                        });
                        // let mut dropped_action = None;
                        // Layout for the folder structure and main content
                        ui.columns(2, |columns| {
                            columns[0].vertical(|ui| {
                                ui.set_min_width(75.0); // Set a minimum width for the folder column
                                if ui.button("/").clicked() {
                                    self.selected_folder = Some("/".to_owned());
                                }

                                // Make each folder a drop target
                                for folder_name in self.folders.keys() {
                                    if folder_name != "/" {
                                        drop_target(ui, true, |ui| {
                                            let folder_button = ui.button(folder_name);
                                            if folder_button.clicked() {
                                                self.selected_folder = Some(folder_name.clone());
                                            }

                                            // Handling the drop logic
                                            if ui.memory(|mem| mem.is_anything_being_dragged()) {
                                                ui.input(|input| {
                                                    if input.pointer.any_released() && folder_button.hovered() {
                                                        if let Some(dragged_action) = &self.dragged_action {
                                                            *dropped_action.borrow_mut() = Some((dragged_action.clone(), folder_name.clone()));
                                                        }
                                                    }
                                                });
                                            }
                                        });
                                    }
                                }
                            });


                            columns[1].vertical(|ui| {
                                // Actions display logic
                                if let Some(folder_name) = &self.selected_folder {
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        ui.label(format!("Contents of folder: {}", folder_name));
                                        if let Some(folder_actions) = self.folders.get(folder_name) {
                                            for action_name in folder_actions {
                                                ui.horizontal(|ui| {
                                                    let action_id_ui = Id::new(action_name); // Use action name as the unique identifier for UI elements
                                                    drag_source(ui, action_id_ui, |ui| {
                                                        ui.label(action_name);
                                                        ui.memory(|mem| {
                                                            if mem.is_being_dragged(action_id_ui) {
                                                                self.dragged_action = Some(action_name.clone());
                                                            }
                                                        });
                                                    });
                                                    // Add a small button next to the action for selection
                                                    if ui.button("Open").clicked() {
                                                        println!("Opening action: {:?}", action_name);
                                                        if let Some(&action_id) = self.actions.get(action_name) {
                                                            // Use the numerical ID from the actions HashMap
                                                            self.opened_action_id = Some(action_id);
                                                            self.action_detail_window_open = Some(action_name.clone());
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    });
                                } else {
                                    ui.label("GitHub Actions:");
                                    for (action_name, action_id) in &self.actions {
                                        ui.horizontal(|ui| {
                                            let action_id_ui = Id::new(action_name); // Use action name as the unique identifier for UI elements
                                            drag_source(ui, action_id_ui, |ui| {
                                                ui.label(action_name);
                                            });
                                            // Add a small button next to the action for selection
                                            if ui.button("Open").clicked() {
                                                self.opened_action_id = Some(*action_id); // Store the numerical ID
                                                self.action_detail_window_open = Some(action_name.clone()); // Store the action name
                                                // Code to open a new window or perform another action
                                            }
                                        });
                                    }
                                }
                            });
                        });
                        // Handle the action drop after the loop to avoid double mutable borrow
                        if let Some((action, target_folder)) = dropped_action.borrow().clone() { // Clone the value here
                            for (folder_name, actions) in self.folders.iter_mut() {
                                if folder_name != &target_folder && actions.contains(&action) {
                                    // Remove the action from its current folder
                                    actions.retain(|a| a != &action);
                                    break;
                                }
                            }

                            if let Some(folder_actions) = self.folders.get_mut(&target_folder) {
                                folder_actions.push(action);
                            }
                            self.dragged_action = None;
                        }


                        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                            egui::warn_if_debug_build(ui);
                        });
                    },
                    AppTab::Pull => {
                        if self.last_git_time.elapsed() >= self.auto_git_interval {
                            self.check_repo_status(); // Check the repo status
                        }
                        ui.separator();
                        ui.horizontal_top(|ui| {
                            ui.horizontal_top(|ui| {
                                if ui.add_sized([120.0, 40.0], egui::Button::new("Pull Repository")).clicked() {
                                    println!("Pulling repository: {}", self.config.repo_name);
                                    if self.config.repo_name.is_empty() || self.decrypted_github_pat.is_empty() {
                                        self.error_message = Some("Please configure the repository and GitHub PAT before pulling.".to_string());
                                    } else {
                                        self.error_message = None;
                                        if let Some(repo_location) = pick_folder_location() {
                                            // Check if repository already exists
                                            match Repository::open(&repo_location) {
                                                Ok(_) => {
                                                    println!("Repository already exists at the selected location.");
                                                    self.error_message = Some("Repository already exists at the selected location.".to_string());
                                                    // You can implement further logic here, like fetching updates
                                                },
                                                Err(_) => {
                                                    self.config.repo_path = Some(repo_location.clone()); // Save the repo path
                                                    // Repository does not exist, attempt to clone
                                                    match get_repo(&self.config.repo_name, &self.decrypted_github_pat, &self.config.repo_path) {
                                                        Ok(_) => {
                                                            println!("Repository cloned successfully.");
                                                            // self.repo_path = Path::new(repo_location.clone());
                                                            // println!("Repo path: {:?}", self.repo_path);
                                                            self.check_repo_status();
                                                            self.config.repo_path = Some(repo_location); // Save the repo path
                                                            println!("repo path 2: {:?}", self.config.repo_path);
                                                        },
                                                        Err(e) => self.error_message = Some(format!("Error: {}", e)),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Display the error message if it's set
                                if let Some(ref message) = self.error_message {
                                    ui.colored_label(egui::Color32::RED, message);
                                }
                            });
                            ui.horizontal_top(|ui| {
                                let mut show_commit_window = self.show_commit_message_input;
                                let mut commit_and_push_clicked = false;

                                if ui.add_sized([120.0, 40.0], egui::Button::new("Upload Repository")).clicked() {
                                    show_commit_window = true;
                                }

                                if show_commit_window {
                                    egui::Window::new("Commit Changes")
                                        .open(&mut show_commit_window)
                                        .show(ctx, |ui| {
                                            ui.text_edit_singleline(&mut self.commit_message);
                                            if ui.button("Commit & Push").clicked() {
                                                commit_and_push_clicked = true;
                                            }
                                        });

                                    if commit_and_push_clicked {
                                        if !self.commit_message.is_empty() {
                                            self.handle_commit_and_push();
                                            show_commit_window = false; // Close the window
                                        } else {
                                            // Display error message
                                        }
                                    }
                                }

                                self.show_commit_message_input = show_commit_window;
                            });
                        });
                        ui.separator();
                        ui.vertical_centered(|ui| {
                            // Display the repository status
                            match self.repo_status {
                                RepoStatus::NotCloned => ui.heading("No repo cloned"),
                                RepoStatus::UpToDate => ui.heading("Repo cloned and up to date"),
                                RepoStatus::ChangesMade => {
                                    ui.heading("Changes made to repo since last upload:");

                                    // Check if repo_path is Some and convert to Path
                                    if let Some(ref repo_path_str) = self.config.repo_path {
                                        let repo_path = Path::new(repo_path_str);
                                        // Open the repository
                                        if let Ok(repo) = Repository::open(repo_path) {
                                            let mut opts = StatusOptions::new();
                                            opts.show(StatusShow::IndexAndWorkdir);
                                            opts.include_untracked(true);

                                            if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
                                                for entry in statuses.iter() {
                                                    let status = entry.status();
                                                    let path_str = entry.path().unwrap_or(""); // Handle unwrap properly
                                                    let path = Path::new(path_str);

                                                    if status.is_index_new() || status.is_wt_new() {
                                                        if path.is_dir() {
                                                            // If it's a directory, list all files within it
                                                            for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                                                                let file_path = entry.path();
                                                                if file_path.is_file() {
                                                                    ui.label(format!("New file: {}", file_path.display()));
                                                                }
                                                            }
                                                        } else {
                                                            ui.label(format!("New file: {}", path_str));
                                                        }
                                                    } else if status.is_index_modified() || status.is_wt_modified() {
                                                        ui.label(format!("Modified: {}", path_str));
                                                    } else if status.is_index_deleted() || status.is_wt_deleted() {
                                                        ui.label(format!("Deleted: {}", path_str));
                                                    }
                                                }
                                            }
                                        } else {
                                            ui.label("Failed to open repository");
                                        }
                                    } else {
                                        ui.label("Repository path is not set");
                                    } ui.label("")
                                }
                                // ... handle other statuses ...
                            }
                        });
                        // ... rest of the Pull tab UI ...
                    },
                    AppTab::Confirm => {
                        ui.separator();
                        ui.label("Review and Confirm Terraform Changes:");
                        ui.separator();

                    }

                    // Handle other tabs...
                }
            });
        }
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
