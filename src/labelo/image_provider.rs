use std::borrow::BorrowMut;
use std::sync::Arc;
use std::time::Duration;
use std::{path, fs};
use std::collections::VecDeque;
use egui;
use egui::epaint::text;
use image::ImageReader;
use std::default::Default;

use mini_moka::sync::Cache;
use std::thread::{self, JoinHandle};
use std::sync::{
    mpsc::{self, SyncSender},
    RwLock};
use std::marker::{Sync, Send};

pub trait ImageProvider 
    where Self: Sized {
    fn from_path(path: path::PathBuf) -> Result<Self, String>;
    fn frame_count(&self) -> usize;
    fn get_frame(&mut self, frame: usize, texture_handle: &mut Option<egui::TextureHandle>, ctx: &egui::Context);
}

const CACHE_COUNT: u64 = 100;
const PREDICTIVE_LOADING_IMAGE_COUNT: usize = 25;
pub struct ImageDirectory {
    path: path::PathBuf,
    image_count: usize,
    image_filenames: Arc<RwLock<Vec<path::PathBuf>>>,
    texture_options: egui::TextureOptions,

    image_cache: Arc<Cache<usize, Arc<egui::ColorImage>>>,

    predictive_frame_sender: Option<SyncSender<usize>>,
    predictive_loading_thread: Option<JoinHandle<()>>,
}

impl Default for ImageDirectory {
    fn default() -> Self {
        Self {
            path: path::PathBuf::new(),
            image_count: 0,
            image_filenames: Arc::new(RwLock::new(vec![])),
            texture_options: egui::TextureOptions::LINEAR,
            image_cache: Arc::new(Cache::new(CACHE_COUNT)),
            predictive_frame_sender: None,
            predictive_loading_thread: None,
        }
    }
}

impl ImageProvider for ImageDirectory {
    fn from_path(path: path::PathBuf) -> Result<Self, String> {
        let mut result = ImageDirectory {
            path: path::PathBuf::new(),
            image_count: 0,
            image_filenames: Arc::new(RwLock::new(vec![])),
            texture_options: egui::TextureOptions::LINEAR,
            image_cache: Arc::new(Cache::builder()
                .max_capacity(CACHE_COUNT)
                .time_to_idle(Duration::from_secs(60))
                .build()),
            predictive_frame_sender: None,
            predictive_loading_thread: None,
        };

        if !path.is_dir() {
            return Err("Not a directory.".to_string());
        }

        for entry_ in fs::read_dir(&path).expect("Could not read from path.") {
            if let Ok(entry) = entry_ {
                if entry.path().is_file() {
                    if let Some(ext) = entry.path().extension() {
                        let e = ext.to_string_lossy().to_string(); // to_str().unwrap().to_string();
                        if e == "jpg" || e == "png" {                        
                            result.image_filenames.write().unwrap().push(entry.path());
                        }
                    }
                }
            }
        }

        result.image_filenames.write().unwrap().sort();
        result.path = path;
        result.image_count = result.image_filenames.read().unwrap().len();

        result.predictive_loader();

        return Ok(result);
    }

    fn frame_count(&self) -> usize {
        self.image_count
    }

    fn get_frame(&mut self, frame: usize, texture_handle: &mut Option<egui::TextureHandle>, ctx: &egui::Context) {

        if frame >= self.image_count { return; }

        if let Some(sender) = &self.predictive_frame_sender {
            let _ = sender.send(frame);
        }

        let img_cached = self.image_cache.get(&frame);

        // if img_cached.is_some() {
        //     println!("Cache hit frame {}", frame);
        // }

        let img = match img_cached {
            Some(colorimage) => { Some(colorimage.clone()) },
            None => {
                let result = Self::load_image(&self.image_filenames.read().unwrap()[frame]);
                if let Ok(i) = result { 
                    let a = Arc::new(i);
                    self.image_cache.insert(frame, a.clone());
                    Some(a)
                } else {
                    None
                }
            }
        };
        
        if let Some(img) = img {
            if let Some(texture_handle) = texture_handle {
                if texture_handle.size()[0] != img.width() || texture_handle.size()[1] != img.height() {
                    *texture_handle = ctx.load_texture("videoframe", img, self.texture_options);
                } else {
                    texture_handle.set(img, self.texture_options);
                }
            } else {
                *texture_handle = Some(ctx.load_texture("videoframe", img, self.texture_options));
            }
        }
    }
}

impl Drop for ImageDirectory {
    fn drop(&mut self) {
        let ss = &self.predictive_frame_sender;
        if let Some(s) = ss {
            drop(s);
            self.predictive_frame_sender = None;
        }
        
        let t = self.predictive_loading_thread.take();
        if let Some(t) = t {
            t.join();
        }
    }
}

impl ImageDirectory {
    fn load_image(path: &path::PathBuf) -> Result<egui::ColorImage, String> {
        let reader = ImageReader::open(path);
        if let Ok(reader) = reader {
            let image = reader.decode();
            if let Err(e) = image {
                return Err(e.to_string());
            }
            let image = image.unwrap();
            let size = [image.width() as _, image.height() as _];
            let image_buffer = image.to_rgba8();
            let pixels = image_buffer.as_flat_samples();

            Ok(egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))            
        } else {
            Err("Could not read image file.".to_string())
        }
    }

    fn predictive_loader(&mut self) {
        let (sender, receiver) = mpsc::sync_channel::<usize>(1000);
        self.predictive_frame_sender = Some(sender);

        let image_cache = self.image_cache.clone();
        let image_filenames = self.image_filenames.clone();
        let image_count = self.image_count;

        let t = std::thread::spawn(move || -> () {
            let mut recent_frames = VecDeque::<usize>::new();
            const MAX_RECENT_FRAMES: usize = 3;
            let mut new_frame_while_loading_images = false;
            let mut updated_frame_while_loading_images: usize = 0;
            loop {
                let result = if new_frame_while_loading_images {
                    new_frame_while_loading_images = false;
                    Ok(updated_frame_while_loading_images)
                } else {
                    receiver.recv()
                };

                if let Err(_result) = result {
                    println!("Receive error, predictive loader is stopping.");
                    return ();
                }

                let frame = result.unwrap();
                // println!("Predictive loader checking out frame {}", frame);

                let mut add_frame = |frame: usize| -> (usize, usize) {
                    recent_frames.push_back(frame);
                    if recent_frames.len() > MAX_RECENT_FRAMES {
                        recent_frames.pop_front();
                    }

                    let mut up = 0;
                    let mut down = 0;
                    for i in 0..recent_frames.len()-1 {
                        if recent_frames[i+1] as i64 - recent_frames[i] as i64 > 0 {
                            up += 1;
                        } else {
                            down += 1;
                        }
                    }
                    (up, down)
                };

                let (up, down) = add_frame(frame);
                // println!("Up: {}, down: {}", up, down);

                #[derive(PartialEq)]
                enum Direction {
                    Up, Down, None
                }

                let dir = if up == recent_frames.len() - 1 {
                    Direction::Up
                } else if down == recent_frames.len() - 1 {
                    Direction::Down
                } else {
                    Direction::None
                };


                if dir != Direction::None {
                    // predictively load a few frames coming after this frame.
                    for n in 0..PREDICTIVE_LOADING_IMAGE_COUNT {
                        let result = receiver.try_recv();
                        if let Ok(new_frame) = result {
                            updated_frame_while_loading_images = new_frame;
                            new_frame_while_loading_images = true;
                            // println!("New frame found after starting image loading, starting at that frame.");
                            break;
                        }

                        let next_frame = if dir == Direction::Up { 
                            (frame + n).min(image_count - 1) 
                        } else { 
                            (frame as i64 - n as i64).max(0) as usize
                        };

                        if !image_cache.contains_key(&(next_frame)) {
                            let fname = image_filenames.read().unwrap()[next_frame].clone();
                            let result = Self::load_image(&fname);
                            if let Ok(image) = result {
                                // println!("Loaded frame {} into cache.", next_frame);
                                image_cache.insert(next_frame, Arc::new(image));
                            }
                        }
                    }
                }
            }
        });

        self.predictive_loading_thread = Some(t);
    }
}