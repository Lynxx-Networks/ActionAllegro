use std::collections::HashMap;
use crate::helpers::get_actions;
use egui::{CollapsingHeader, Color32, RichText, TextStyle, Sense, CursorIcon, Order, LayerId, Rect, Shape, Vec2, Id, InnerResponse, Ui, epaint, vec2, Label, SidePanel, CentralPanel};
use serde::{Serialize, Deserialize};
use std::fs;
use serde_json;
use rfd::FileDialog;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,
    label2: String,
    actions: Vec<String>, // Store the names of GitHub Actions here
    display_actions: bool, // Flag to indicate whether to display actions
    error_message: Option<String>,
    columns: Vec<Vec<String>>,
    folders: HashMap<String, Vec<String>>,
    selected_folder: Option<String>,
    new_folder_name: String,
    dragged_action: Option<String>,
    drop_target_folder: Option<String>,
    actions_fetched: bool, // Add this new field

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AppConfig {
    // Include all the fields that make up your application's state
    folders: HashMap<String, Vec<String>>,
    repo_name: String,
    github_pat: String,
    // ... other fields ...
}

fn pick_file_location() -> Option<String> {
    FileDialog::new()
        .save_file()
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

        // Now we move the visuals of the body to where the mouse is.
        // Normally you need to decide a location for a widget first,
        // because otherwise that widget cannot interact with the mouse.
        // However, a dragged component cannot be interacted with anyway
        // (anything with `Order::Tooltip` always gets an empty [`Response`])
        // So this is fine!

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
            // Example stuff:
            label: "myuser/myrepo".to_owned(),
            label2: "ghp_sdfjkh238hdsklsdjf983nldfejfds".to_owned(),
            value: 2.7,
            actions: Vec::new(), // Store the names of GitHub Actions here
            display_actions: false, // Flag to indicate whether to display actions
            error_message: None,
            folders: HashMap::new(),
            selected_folder: None,
            new_folder_name: String::new(),
            dragged_action: None,
            drop_target_folder: None,
            actions_fetched: false,
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


    fn export_config(&self, path: String) {
        println!("test on label: {}", self.label);
        let config = AppConfig {
            folders: self.folders.clone(),
            repo_name: self.label.clone(),   // Set from `label`
            github_pat: self.label2.clone(), // Set from `label2`
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
        // Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

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
                ui.text_edit_singleline(&mut self.label);
            });
            ui.horizontal(|ui| {
                ui.label("What is your Github API Key?: ");
                ui.text_edit_singleline(&mut self.label2);
            });
            if ui.button("Fetch Actions").clicked() {
                self.error_message = None; // Clear previous error messages
                // Call the function to fetch GitHub Actions
                // You need to implement this part
                match get_actions(&self.label, &self.label2) {
                    Ok(actions) => {
                        println!("Fetched {} actions", actions.len());
                        self.actions = actions.clone(); // Clone the actions list
                        self.folders.insert("/".to_owned(), actions); // Add actions to the root folder
                        self.selected_folder = Some("/".to_owned()); // Set selected folder to root
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
                            for action in folder_actions {
                                let action_id = Id::new(action); // Use action name as the unique identifier
                                drag_source(ui, action_id, |ui| {
                                    ui.label(action);
                                    ui.memory(|mem| {
                                        if mem.is_being_dragged(action_id) {
                                            self.dragged_action = Some(action.clone());
                                        }
                                    });
                                });
                            }
                        }
                    } else {
                        ui.label("GitHub Actions:");
                        for action in &self.actions {
                            let action_id = Id::new(action); // Use action name as the unique identifier
                            drag_source(ui, action_id, |ui| {
                                ui.label(action);
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
