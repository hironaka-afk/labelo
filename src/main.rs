use eframe::{egui::{self, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2}, 
    epaint::TextureHandle,
    glow::Texture};
use crate::egui::load::SizedTexture;
use egui_extras;
use egui_dialogs::{self, DialogDetails, StandardReply};
use image::{
    ImageDecoder,
    codecs::jpeg::JpegDecoder};
use labelo::*;
use parry2d::{bounding_volume::Aabb, na::OPoint, math::Point, query::PointQuery};

use std::{borrow::BorrowMut, cell::RefCell, env::join_paths, path::{Path, PathBuf}, process::exit, rc::Rc, str::FromStr};
use std::{
    fs::{self, DirEntry},
    io,
    io::{Write}
};
use clap::{self, Parser};

mod labelo;
use labelo::labelo::*;
use labelo::config::*;
use labelo::image_provider::*;

use egui::{ecolor::Color32, ColorImage, TextBuffer, Ui};


/// Labeling video sequences. The sequences should be images in a directory, named so that they can be sorted alphanumerically.
/// Run without the --label_config argument to generate a default label configuration file in ~/.labelo_config.toml.
/// You can use that to build your own.
/// If the file defined by --label_config does not exist, it will also be created and filled with default values,
/// so you can modify it to your liking.
#[derive(clap::Parser, Debug, Clone)]
#[command(version, about, long_about=None)]
struct Cli {
    /// Label configuration file, defining which labels to use.
    #[arg(short, long)]
    label_config: Option<PathBuf>,
    /// Input directory containing the images.
    #[arg(short, long)]
    input_dir: Option<PathBuf>,
    /// Output label file (json format). If the file exists, it will be read at startup.
    #[arg(short, long, default_value = "labels.json")]
    output_file: PathBuf,
} 

fn main() -> Result<(), eframe::Error> {

    let args = Cli::parse();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Labelo",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::<MyApp>::default())
        })
    )
}


struct MyApp<'a> {
    first_update: bool,

    images_dir: Option<PathBuf>,
    image_provider: ImageDirectory,

    current_image: usize,
    previous_image: usize,
    
    label_configs_filename: PathBuf,
    labels_filename: PathBuf,
    label_task: LabelTask,
    annotation_action: AnnotationAction,
    /// The annotation that is currently being seen in the frame, in the currently active annotation sequence.
    /// This is NOT the object that with_current_annotation() is working on.
    /// This object is used by the labelling pane to display the annotation object properties.
    current_annotation_copy: Option<Annotation>,

    texture: Option<egui::TextureHandle>,

    drag_start_position: Vec2,

    dialogs: egui_dialogs::Dialogs<'a>,

    play_mode: bool,
    play_fps: usize,
}


impl<'a> MyApp<'a> {

    const CORNER_CATCH_RADIUS: f32 = 10.0;

    fn new() -> Self {

        let args = Cli::parse();

        let dir = dirs::home_dir().unwrap_or(PathBuf::new());
        let config_filepath = args.label_config.unwrap_or(dir.join(".labelo_config.toml"));
        if !config_filepath.exists() {
            println!("Creating config file {} since it does not exist.", config_filepath.to_string_lossy());
            let _ = std::fs::create_dir_all(dir);
            let config = LabelConfigs::default();
            let s = toml::to_string_pretty(&config);
            let f = std::fs::File::create(&config_filepath);
            if let (Ok(mut f), Ok(s)) = (f, s) {
                println!("Writing default label config to {}.", config_filepath.to_string_lossy());
                let _ = write!(f, "{}", s);
            }

        }
        
        let mut label_task = LabelTask::new();
        let _ = label_task.load_label_configs(&config_filepath);
        let mut result = Self {
            first_update: true,
            images_dir: args.input_dir,
            image_provider: ImageDirectory::default(),
            // image_stack: vec![],
            current_image: 0,
            previous_image: 0,
            label_configs_filename: config_filepath,
            labels_filename: args.output_file,
            label_task: label_task,
            annotation_action: AnnotationAction::new(),
            current_annotation_copy: None,
            texture: None,
            drag_start_position: Vec2::new(0.0, 0.0),
            dialogs: egui_dialogs::Dialogs::new(),
            play_mode: false,
            play_fps: 30,
        };

        result.label_task.load_annotations(&result.labels_filename, true);
        result
    }



    fn open_dir(&mut self, dir: &PathBuf) -> Result<(), String> {
        let image_provider = ImageDirectory::from_path(dir.clone())?;
        self.image_provider = image_provider;
        return Ok(());
    }


    fn set_current_image(&mut self, frame_index: usize, ctx: &egui::Context) -> Result<(), String> {

        if self.previous_image == frame_index {
            return Ok(());
        }

        // println!("{} -> {}", self.previous_image, frame_index);

        self.image_provider.get_frame(frame_index, &mut self.texture, ctx);

        self.previous_image = self.current_image;
        self.current_image = frame_index;
        // ctx.request_repaint();
        Ok(())
    }


    fn with_current_annotation<F: Fn(&mut Annotation) -> ()>(&mut self, f: F) {
        if self.label_task.has_sequences() {
            let s = &mut self.label_task.sequences[self.label_task.current_sequence];
            let mut a = s.get_interpolated_annotation_for_frame(self.current_image);
            if let Some(a) = &mut a {
                f(a);
                s.edit_annotation(self.current_image, a);
            }
        }    
    }    

}


impl<'a> Default for MyApp<'a> {

    fn default() -> Self {
        let s = MyApp::new();
        s
    }
}


fn draw_annotation(response: &egui::Response, ui: &mut egui::Ui, annotation: &Annotation, is_active: bool) {

    if annotation.invisible {
        return;
    }

    let r = response.rect;
    let w = r.width();
    let h = r.height();
    let p0 = r.left_top();
    
    let rr = Rect::from_min_max(Pos2::new(annotation.bbox.mins.x * w + p0.x, annotation.bbox.mins.y * h + p0.y), 
                                      Pos2::new(annotation.bbox.maxs.x * w + p0.x, annotation.bbox.maxs.y * h + p0.y));

    let st = if annotation.interpolated {
        Stroke::new(4.0, Color32::BLUE)
    } else {
        Stroke::new(4.0, Color32::RED)
    };

    if is_active {
        ui.painter().rect(rr, Rounding::ZERO, Color32::TRANSPARENT, st);

        if let Some(hover_pos) = response.hover_pos() {
            let p = normalized_pos(hover_pos, &response);
            let (_corner, corner_dist, corner_point) = annotation.closest_corner_point(p.x, p.y);
                
            if corner_dist * response.rect.width() < MyApp::CORNER_CATCH_RADIUS {
                let x = corner_point.x * w + p0.x;
                let y = corner_point.y * h + p0.y;
                ui.painter().circle_filled(Pos2::new(x, y), MyApp::CORNER_CATCH_RADIUS, Color32::from_rgba_unmultiplied(0, 128, 0, 64));
            } 
        }
    } else {
        ui.painter().rect(rr, Rounding::ZERO, Color32::from_rgba_unmultiplied(200, 0, 0, 32), st);
    }
}


fn normalized_pos(p: egui::Pos2, response: &egui::Response) -> Vec2 {
    let sz = response.rect.size();
    let p0 = response.rect.left_top();
    // Normalized clicked point:
    let p = (p - p0) / sz ;
    p
}


impl<'a> eframe::App for MyApp<'a> {

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.dialogs.show(ctx);

        if self.first_update {
            if let Some(p) = &self.images_dir.clone() {
                self.image_provider = ImageDirectory::from_path(p.clone()).unwrap_or(ImageDirectory::default());
                self.current_image = 0;
                self.previous_image = 1;
            }

            self.first_update = false;
        }

        let (task_dropped,
             left_arrow,
             right_arrow,
             delete,
             quit,
             left_button_pressed,
             left_button_down,
             left_button_released
            ) = ctx.input(|i| {
                 let mut task_dropped = false;
                 if i.raw.dropped_files.len() > 0 {
                    if let Some(path) = &i.raw.dropped_files[0].path {
                        let open_dir = self.open_dir(path);
                        if let Err(e) = open_dir {
                            let is_toml = if let Some(e) = path.extension() {
                                    e.to_ascii_lowercase() == "toml"
                                } else { 
                                    false 
                                };
                            if is_toml {
                                self.label_configs_filename = path.clone();
                                self.label_task = LabelTask::new();
                                self.label_task.load_label_configs(&self.label_configs_filename);
                            } else {
                                println!("Error: {} and dropped file was not a config file.", e.to_string());
                            }
                        } else {
                            task_dropped = true;
                            self.current_image = 0;
                            self.label_task = LabelTask::new();
                            self.label_task.load_label_configs(&self.label_configs_filename);
                        } 
                    }
                 }

                 (task_dropped,
                  i.key_pressed(egui::Key::ArrowLeft),
                  i.key_pressed(egui::Key::ArrowRight),
                  i.key_pressed(egui::Key::Delete),
                  i.modifiers.ctrl && i.key_pressed(egui::Key::Q),
                  i.pointer.button_pressed(egui::PointerButton::Primary),
                  i.pointer.button_down(egui::PointerButton::Primary),
                  i.pointer.button_released(egui::PointerButton::Primary),
                )
             });

        if self.image_provider.frame_count() > 0 {
            if left_arrow {
                self.current_image = (self.current_image as i32 - 1).rem_euclid(self.image_provider.frame_count() as i32) as usize;
                // ctx.request_repaint()
            }

            if right_arrow {
                self.current_image = ((self.current_image + 1) as i32).rem_euclid(self.image_provider.frame_count() as i32) as usize;
                // ctx.request_repaint()
            }

            if delete {

            }

            if quit {
                let result = self.label_task.save_annotations(&self.labels_filename, self.image_provider.frame_count(), true);
                match result {
                    Ok(_) => exit(1),
                    Err(e) => println!("Saving annotations did not work ({}).", e),
                }
            }

            if task_dropped {
                // self.image_stack.sort_by(|a, b| a.name.cmp(&b.name));
                ctx.request_repaint();
            }
        }

        //
        // Side panel with tools.
        egui::SidePanel::left("leftpanel").show(ctx, |ui| {

            ui.label("Quit: CTRL-Q (saves annotations)");

            let mut dummy_annotation = Annotation::new(&self.label_task.configs, 0.0, 0.0, 0);
            let ann = self.current_annotation_copy.as_mut().unwrap_or(&mut dummy_annotation);
            // if let Some(ann) = &mut self.current_annotation_copy {
                let seq_string = format!("Sequence {}", self.label_task.current_sequence);
                ui.label(seq_string);
                let mut changed = false;
                let mut response = ui.checkbox(&mut ann.invisible, "Invisible");
                changed |= response.changed();
                ui.separator();
                ui.label(RichText::new("Labels").size(15.0).strong());
                for (label_index,lc) in self.label_task.configs.label_configs.iter().enumerate() {
                    match lc {
                        LabelConfig::S(lcs) => {
                            ui.label(format!("{}:",&lcs.name));
                            for s in &lcs.states {
                                if let Label::S(label) = &mut ann.labels[label_index] {
                                    response = ui.selectable_value(&mut label.state, s.to_string(), s);
                                    changed |= response.changed();
                                } else {
                                    println!("Error: Did not find a Label::S where I expected one.");
                                }
                            }
                        },
                        LabelConfig::I(lci) => {
                            if let Label::I(label) = &mut ann.labels[label_index] {
                                let title = format!("{} ({}-{})", &lci.name, lci.first, lci.last);
                                response = ui.add(egui::Slider::new(&mut label.state, lci.first..=lci.last).text(title));
                                changed |= response.changed();
                            } else {
                                println!("Error: Did not find a Label::I where I expected one.");
                            }
                        }
                    }
                }
        
                if changed {
                    if self.label_task.has_sequences() {
                        self.label_task.sequences[self.label_task.current_sequence].edit_annotation(
                            self.current_image, ann);
                    }
                }
        
                ui.add_space(10.0);
                ui.separator();

                if ui.button(RichText::new("Copy labels to following keyframes").small()).clicked() {
                    if self.label_task.has_sequences() {
                        self.label_task.sequences[self.label_task.current_sequence].propagate(self.current_image);
                    }
                }
                ui.separator();
                if ui.button("Save annotations").clicked() {
                    self.label_task.save_annotations(&self.labels_filename, self.image_provider.frame_count(), true);
                }
                ui.horizontal(|ui| {
                    if ui.button("Add sequence").clicked() {
                        self.label_task.sequences.push(AnnotationSequence::new());
                        self.label_task.current_sequence = self.label_task.sequences.len() - 1;
                    }
                });
                ui.separator();
        
                for i in 0..self.label_task.sequences.len() {
                    ui.horizontal(|ui| {
                        let response = ui.selectable_label(i == self.label_task.current_sequence, format!("Annotation sequence {}", i));
                        if response.clicked() {
                            self.label_task.current_sequence = i;
                        }
                        if ui.button("Delete").clicked() {
                            if i < self.label_task.sequences.len() {
                                self.label_task.sequences.remove(i);
                            }
                            let l = self.label_task.sequences.len();
                            if self.label_task.current_sequence >= l {
                                self.label_task.current_sequence = if l > 0 { l - 1 } else { 0 };
                            }

                        }
                    });
                }
        });

        //
        // Main panel with image.
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {

                if let Err(e) = self.set_current_image(self.current_image, &ctx) {
                    println!("Error: {}", e);
                }

                if let Some(texture) = &self.texture {

                    let t = egui::ImageSource::Texture(SizedTexture::from_handle(texture));
                    let response = ui.add(egui::Image::new(t).shrink_to_fit()
                        .sense(Sense::click_and_drag()));
                        
                    response.context_menu(|ui| {
                        if ui.button("New annotation sequence").clicked() {
                                self.label_task.sequences.push(AnnotationSequence::new());
                                self.label_task.current_sequence = self.label_task.sequences.len() - 1;
                            ui.close_menu();
                        }
                    });

                    ui.style_mut().spacing.slider_width = response.rect.width();
                    let slider = egui::Slider::new(&mut self.current_image, 0..=self.image_provider.frame_count()-1)
                        .trailing_fill(true).show_value(false);
                    ui.add(slider);
                    ui.horizontal(|ui| {
                        if ui.button("<-").clicked() {
                            if self.image_provider.frame_count() > 0 {
                                self.current_image = (self.current_image as i64 - 1).max(0) as usize;
                            }
                        }
                        if ui.button("->").clicked() {
                            if self.image_provider.frame_count() > 0 {
                                self.current_image = (self.current_image + 1).min(self.image_provider.frame_count()-1);
                            }
                        }
                        if self.play_mode {
                            if ui.button("Stop").clicked() {
                                self.play_mode = false;
                            }
                        } else {
                            if ui.button("Play").clicked() {
                                self.play_mode = true;
                            }
                        }
                        ui.label(format!("Frame: {}", self.current_image));
                    });

                    //
                    // Select the currently active annotation sequence:
                    if response.clicked() {
                        if let Some(pp) = response.interact_pointer_pos() {
                            let p = normalized_pos(pp, &response);
                            let closest_sequence_ = self.label_task.get_closest_annotation_sequence(self.current_image, p.x, p.y, true);

                            if let Some((closest_sequence, distance, _contains_point)) = closest_sequence_ {
                                self.label_task.current_sequence = closest_sequence;
                                // println!("Selected sequence {}", closest_sequence);
                            }
                        }
                    }

                    
                    // Select an action when the left button is pressed:
                    if left_button_pressed {
                        // println!("Left button pressed");
        
                        if let Some(pp) = response.interact_pointer_pos() {
                            let p = normalized_pos(pp, &response);

                            let annotation_ = self.label_task.get_current_interpolated_annotation_for_frame(self.current_image);
                            
                            let mut action: Option<ActionType> = None;

                            if let Some(annotation) = &annotation_ {

                                let (corner, corner_dist, _corner_point) = annotation.closest_corner_point(p.x, p.y);
                                if corner_dist * response.rect.width() < MyApp::CORNER_CATCH_RADIUS {
                                    action = Some(ActionType::ModifyCorner(corner));
                                } else {
                                    let bbox: Aabb = (&annotation.bbox).into();
                                    if bbox.contains_local_point(&Point::new(p.x, p.y)) {
                                        action = Some(ActionType::Move(p));
                                    }
                                }
                            }
            
                            if action.is_none() && annotation_.is_none() {
                                // If there is a sequence, but the current sequence has no annotations yet, create an annotation for it.
                                if self.label_task.has_sequences() && self.label_task.sequences[self.label_task.current_sequence].annotations.is_empty() {
                                    action = Some(ActionType::New);
                                    self.label_task.sequences[self.label_task.current_sequence].annotations.push(Annotation::new(
                                        &self.label_task.configs, 0.0, 0.0, self.current_image));
                    
                                    self.with_current_annotation(|a| {
                                        a.invisible = false;
                                    });
                                }
                            }

                            self.annotation_action.action_type = action.unwrap_or(self.annotation_action.action_type);
                        }
                    }
        
                    if left_button_released {
                        self.annotation_action.action_type = ActionType::None;
                    }
        

                    // if response.clicked() {
                    //     let p = response.interact_pointer_pos().unwrap();
                    //     // println!("{:?}", p);
                    //     let p = normalized_pos(p, &response);
                    //     // println!("{:.3}, {:.3}", p.x, p.y);
                    // }

                    if response.drag_started() {
                        let p = response.interact_pointer_pos().unwrap();
                        // println!("{:?}", p);
                        let p = normalized_pos(p, &response);
                        // println!("Drag starting: {:.3}, {:.3}", p.x, p.y);
                        self.drag_start_position = p;
                    }

                    if response.dragged() {
                        let p = response.interact_pointer_pos().unwrap();
                        // println!("{:?}", p);
                        let p = normalized_pos(p, &response);

                        // println!("Dragged; action type: {:?}", self.annotation_action.action_type);
                        match &self.annotation_action.action_type {
                            ActionType::New => {
                                self.with_current_annotation(|a| {
                                    a.bbox.mins = SerializablePoint::new(p.x, p.y);
                                });
                                self.annotation_action.action_type = ActionType::ModifyCorner(BoxCorner::RD);
                            },
                            ActionType::None => {},
                            ActionType::ModifyCorner(c) => {
                                let cc = c.clone();
                                self.with_current_annotation(|a| {
                                    match cc {
                                        BoxCorner::LU => {
                                            a.bbox.mins = SerializablePoint::new(a.bbox.maxs.x.min(p.x), a.bbox.maxs.y.min(p.y));
                                        },
                                        BoxCorner::RU => {
                                            a.bbox.maxs.x = a.bbox.mins.x.max(p.x);
                                            a.bbox.mins.y = a.bbox.maxs.y.min(p.y);
                                        },
                                        BoxCorner::RD => {
                                            a.bbox.maxs = SerializablePoint::new(a.bbox.mins.x.max(p.x), a.bbox.mins.y.max(p.y));
                                        },
                                        BoxCorner::LD => {
                                            a.bbox.mins.x = a.bbox.maxs.x.min(p.x);
                                            a.bbox.maxs.y = a.bbox.mins.y.max(p.y);
                                        }
                                    }
                                   
                                });
                            },
                            ActionType::Move(old_p) => {
                                let delta = p - old_p.clone(); 
                                self.with_current_annotation(|a| {
                                    a.bbox.mins.x += delta.x;
                                    a.bbox.mins.y += delta.y;
                                    a.bbox.maxs.x += delta.x;
                                    a.bbox.maxs.y += delta.y;
                                });
                                self.annotation_action.action_type = ActionType::Move(p);
                            }
                        }
                    }

                    if response.drag_stopped() {

                        match self.annotation_action.action_type {
                            ActionType::None => {},
                            ActionType::Move(_) => {},
                            ActionType::New => {},
                            ActionType::ModifyCorner(_) => {}
                        }
                    }

                    //
                    // Draw visible boxes

                    let anns = self.label_task.get_all_interpolated_annotations_for_frame(self.current_image);

                    for a_ in &anns {
                        let (i, a) = a_;
                        if *i == self.label_task.current_sequence {
                            self.current_annotation_copy = Some(a.clone());
                            draw_annotation(&response, ui, a, true);
                        } else {
                            draw_annotation(&response, ui, a, false);
                        }
                    }

                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.images_dir.clone().unwrap_or(PathBuf::new()).to_string_lossy().to_string()));

                    if self.play_mode {
                        self.current_image = (self.current_image + 1).min(self.image_provider.frame_count() - 1);
                        if self.current_image >= self.image_provider.frame_count() - 1 {
                            self.play_mode = false;
                        }
                        ctx.request_repaint();
                    }
                }
            });
           
        });
    }
}


