use std::collections::HashMap;
use crate::helpers::{get_actions, get_repo, get_workflow_details, pull_workflow_yaml, run_workflow};
use egui::{CollapsingHeader, Color32, RichText, TextStyle, Sense, CursorIcon, Order, LayerId, Rect, Shape, Vec2, Id, InnerResponse, Ui, epaint, vec2, Label, SidePanel, CentralPanel};
use serde::{Serialize, Deserialize};
use std::fs;
use serde_json;
use rfd::FileDialog;
use git2::{Repository, StatusOptions};
use serde_yaml::Value;
type JsonValue = serde_json::Value;
type YamlValue = serde_yaml::Value;



#[derive(serde::Deserialize, serde::Serialize, PartialEq)]
enum AppTab {
    Organize,
    Pull,
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
    action_detail_window_open: Option<String>,
    opened_action_id: Option<u64>,
    opened_workflow_details: Option<String>,
    parsed_workflow_yaml: Option<Value>,
    active_workflow_type: Option<String>,
    active_workflow_inputs: Option<HashMap<String, WorkflowInput>>,
    input_text: String,
    selected_option: String,
    current_input_values: HashMap<String, String>,


    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AppConfig {
    // Include all the fields that make up your application's state
    folders: HashMap<String, Vec<String>>,
    repo_name: String,
    github_pat: String,
    repo_path: Option<String>,
    // ... other fields ...
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
                // ... initialize other fields ...
            },
            action_detail_window_open: None,
            opened_action_id: None,
            opened_workflow_details: None,
            parsed_workflow_yaml: None,
            selected_option: String::new(),
            input_text: String::new(),
            active_workflow_type: Option::from(String::new()),
            active_workflow_inputs: Option::from(HashMap::new()),
            current_input_values: HashMap::new(),
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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }

        Default::default()
    }

    fn check_repo_status(&mut self) {
        let repo_path = match self.config.repo_path.as_ref() {
            Some(path) => path,
            None => {
                self.repo_status = RepoStatus::NotCloned;
                return;
            }
        };

        let repo = match Repository::open(repo_path) {
            Ok(repo) => repo,
            Err(_) => {
                self.repo_status = RepoStatus::NotCloned;
                return;
            }
        };

        // Check for uncommitted changes
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);
        let statuses = match repo.statuses(Some(&mut opts)) {
            Ok(statuses) => statuses,
            Err(_) => {
                self.repo_status = RepoStatus::NotCloned; // Or handle the error differently
                return;
            }
        };

        if statuses.is_empty() {
            self.repo_status = RepoStatus::UpToDate;
        } else {
            self.repo_status = RepoStatus::ChangesMade;
        }

        // Additional checks can be added here (e.g., comparing with remote)
    }

    fn show_action_details_window(&mut self, ctx: &egui::Context) {
        if let Some(action) = &self.action_detail_window_open {
            // Check if the workflow details are already fetched
            let mut window_title = "Action Details".to_string();
            let mut is_window_open = true;
            if self.opened_workflow_details.is_none() {
                // Check if there is an opened action ID
                if let Some(action_id) = self.opened_action_id {
                    match get_workflow_details(&self.config.repo_name, &self.config.github_pat, &Some(action_id)) {
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
                                            match pull_workflow_yaml(&self.config.repo_name, &self.config.github_pat, &Some(path.to_string())) {
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
                                                                            for (input_name, input_details) in inputs {
                                                                                if let Some(input_name_str) = input_name.as_str() {
                                                                                    let input = WorkflowInput {
                                                                                        input_type: input_details.get("type").and_then(|t| t.as_str()).unwrap_or("string").to_string(),
                                                                                        description: input_details.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                                                                                        required: input_details.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
                                                                                        options: Some(input_details.get("options").and_then(|o| o.as_sequence()).map_or_else(Vec::new, |opts| opts.iter().filter_map(|opt| opt.as_str()).map(|s| s.to_string()).collect())),
                                                                                    };
                                                                                    inputs_map.insert(input_name_str.to_string(), input);
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
                                            if let Some(path) = workflow_details["path"].as_str() {
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
                    if let Some(details) = &self.opened_workflow_details {
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
                        if let Some(inputs) = &self.active_workflow_inputs {
                            ui.label("Workflow Inputs:");
                            let mut workflow_inputs_data = HashMap::new();
                            for (input_name, input_details) in inputs {
                                ui.horizontal(|ui| {
                                    ui.label(input_name);
                                    let mut input_value = String::new();
                                    match input_details.input_type.as_str() {
                                        "choice" => {
                                            if let Some(options) = &input_details.options {
                                                let current_value = self.current_input_values.entry(input_name.clone()).or_insert_with(|| options.get(0).cloned().unwrap_or_default());
                                                // Create a dropdown for choice type
                                                egui::ComboBox::from_label(input_name)
                                                    .selected_text("Select an option")
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
                                            if ui.text_edit_singleline(current_value).changed() {
                                                // Value updated in the map automatically
                                            }
                                        }
                                    }
                                    workflow_inputs_data.insert(input_name.clone(), input_value);
                                });
                            }
                            if ui.button("Run Workflow").clicked() {
                                let workflow_id = self.opened_action_id.unwrap(); // Make sure to handle unwrap properly
                                let result = run_workflow(&self.config.repo_name, &self.config.github_pat, workflow_id, Some(&self.current_input_values));
                                match result {
                                    Ok(_) => self.info_message = Some(("Workflow triggered successfully").parse().unwrap()),
                                    Err(e) => self.error_message = Some(format!("Failed to trigger workflow: {}", e)),
                                }
                            }
                        }
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


    fn export_config(&self, path: String) {
        println!("test on label: {}", self.label);
        let config = AppConfig {
            folders: self.folders.clone(),
            repo_name: self.label.clone(),   // Set from `label`
            github_pat: self.label2.clone(), // Set from `label2`
            repo_path: self.config.repo_path.clone(), // Set from `config.repo_path`
            // ... other fields ...
        };

        match serde_json::to_string(&config) {
            Ok(json_string) => {
                if let Err(e) = std::fs::write(&path, json_string) {
                    println!("Failed to write config to file: {}", e);
                } else {
                    println!("Config exported to {}", path);
                }
            },
            Err(e) => println!("Failed to serialize config: {}", e),
        }
    }

}
use std::cell::RefCell;

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

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
        egui::CentralPanel::default().show(ctx, |ui| {
            // Intro and repository info at the top
            let can_export = !self.label.is_empty(); // 'true' if a repo is loaded

            // Horizontal layout for heading and button
            ui.horizontal(|ui| {
                ui.heading("Actions Organizer");
                // Enable button if a repository is loaded
                if ui.add_enabled(can_export, egui::Button::new("Export Config")).clicked() {
                    if let Some(path) = pick_file_location() {
                        self.export_config(path);
                    }
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
                ui.text_edit_singleline(&mut self.config.github_pat);
            });
            if ui.button("Fetch Actions").clicked() {
                self.error_message = None;
                match get_actions(&self.config.repo_name, &self.config.github_pat) {
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
                        powered_by_egui_and_eframe(ui);
                        egui::warn_if_debug_build(ui);
                    });
                },
                AppTab::Pull => {
                    ui.separator();
                    ui.vertical_centered(|ui| {
                        if ui.button("Pull Repository").clicked() {
                            println!("Pulling repository: {}", self.config.repo_name);
                            if self.config.repo_name.is_empty() || self.config.github_pat.is_empty() {
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
                                            match get_repo(&self.config.repo_name, &self.config.github_pat, &self.config.repo_path) {
                                                Ok(_) => {
                                                    println!("Repository cloned successfully.");
                                                    self.check_repo_status();
                                                    self.config.repo_path = Some(repo_location); // Save the repo path
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
                    ui.vertical_centered(|ui| {
                        if ui.button("Upload Repository").clicked() {
                            // Implement logic to upload to the repository
                            // This typically involves adding, committing, and pushing changes
                        }
                    });
                    ui.vertical_centered(|ui| {
                        // Display the repository status
                        match self.repo_status {
                            RepoStatus::NotCloned => ui.label("No repo cloned"),
                            RepoStatus::UpToDate => ui.label("Repo cloned and up to date"),
                            RepoStatus::ChangesMade => ui.label("Changes made to repo since last upload"),
                            // ... handle other statuses ...
                        }
                    });
                    // ... rest of the Pull tab UI ...
                },

                // Handle other tabs...
            }




        });
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
