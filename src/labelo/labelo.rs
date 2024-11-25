

use eframe::egui::{self, TextBuffer, Vec2};
use parry2d::{
    bounding_volume::{aabb::Aabb, BoundingVolume},
    math::Point, na::{OPoint, Point2}, query::PointQuery
};
use serde::{Serialize, Deserialize};
use serde_json::{self, from_reader};

use std::{io::Error, path::{Path, PathBuf}, str::FromStr};
use std::default::Default;
use std::fs::File;
use std::io::{Read, Write};

use crate::config::*;

#[derive(Clone)]
pub struct MetaImage<'a> {
    pub image: egui::Image<'a>,
    pub name: String,
    pub path: String
}


impl<'a> MetaImage<'a> {

    pub fn from_file(filename: &str) -> Self {
        let ps = "file://".to_string() + filename;
        Self { image: egui::Image::new(ps.clone()).show_loading_spinner(false),
               name: std::path::Path::new(filename.as_str()).file_name().unwrap().to_str().unwrap().to_string(),
               path: ps
        }
    }
}


#[derive(Clone)]
pub struct LabelTask {
    pub sequences: Vec<AnnotationSequence>,
    pub current_sequence: usize,

    pub configs: LabelConfigs,
}


impl LabelTask {
    pub fn new() -> Self {
        LabelTask { 
            sequences: Vec::new(),
            current_sequence: 0,
            configs: LabelConfigs::default()
        }
    }

    /// Saves annotations for every frame up to and not including `frame_count`.
    pub fn save_annotations(&self, path: &PathBuf, frame_count: usize, save_only_visible: bool) -> Result<(), String> {
        let f = std::fs::File::create(path);
        if let Ok(f) = f {

            let mut full: Vec<AnnotationSequence> = vec![];
            // create full annotation sequences
            for seq in &self.sequences {
                let mut s = AnnotationSequence::new();
                for frame in 0..frame_count {
                    let a = seq.get_interpolated_annotation_for_frame(frame);
                    if let Some(a) = a {
                        if (save_only_visible) {
                            if !a.invisible || !a.interpolated {
                                s.annotations.push(a);
                            }
                        } else {
                            s.annotations.push(a);
                        }
                    }
                }
                full.push(s);
            }

            if let Err(e) = serde_json::to_writer_pretty(f, &full) {
                return Err(e.to_string());
            }
            return Ok(());
        }
        return Err("Could not open output file.".to_string());
    }

    pub fn load_annotations(&mut self, path: &PathBuf, load_only_keyframes: bool) -> Result<(), String> {
        let f = std::fs::File::open(path);
        if let Ok(f) = f {
            let full_ = serde_json::from_reader(f);
            if let Err(e) = full_ {
                return Err(e.to_string());
            } 

            let full: Vec<AnnotationSequence> = full_.unwrap();
            
            if !load_only_keyframes {
                self.sequences = full;
                return Ok(());
            }

            // Create sequences with only the keyframes.
            self.sequences.clear();
            for seq in &full {
                let mut s = AnnotationSequence::new();
                for l in &seq.annotations {
                    if !l.interpolated {
                        s.annotations.push(l.clone());
                    }
                }
                self.sequences.push(s);
            }
            return Ok(());
        }
        return Err("Could not open input file.".to_string());
    }  

    pub fn load_label_configs(&mut self, filename: &PathBuf) -> Result<(), String> {
        let f = File::open(filename);
        if let Ok(mut f) = f {
            let mut s = String::new();
            let f_result = f.read_to_string(&mut s);
            if let Err(e) = f_result {
                return Err(e.to_string());
            }
            let result = toml::from_str::<LabelConfigs>(s.as_str());
            if result.is_err() {
                return Err(result.err().unwrap().message().to_string());
            }
            self.configs = result.unwrap();
            return Ok(());
        }
        return Err(f.err().unwrap().to_string());
    }

    /// Get the annotation for the current annotation sequence for a particular frame.
    /// This can return None.
    pub fn get_current_interpolated_annotation_for_frame(&self, frame: usize) -> Option<Annotation> {
        if self.has_sequences() {
            self.sequences[self.current_sequence].get_interpolated_annotation_for_frame(frame)
        } else {
            None
        }
    }

    /// True ony if there is at least one sequence.
    pub fn has_sequences(&self) -> bool {
        self.sequences.len() > 0
    }

    /// Get the index to the annotation sequence that is closest to the given point in normalized coordinates. Also returns the distance.
    /// Returns: (index, distance, contains_point)
    pub fn get_closest_annotation_sequence(&self, frame: usize, x: f32, y: f32, must_contain: bool) -> Option<(usize, f32, bool)> {
        let anns = self.get_all_annotations_for_frame(frame);
        let p = Point::new(x, y);

        let mut closest_distance: f32 = f32::MAX;
        let mut result = None;

        for s in anns {
            let a_ = self.sequences[s.0].get_interpolated_annotation_for_frame(frame);
            if let Some(a) = a_ {
                let bbox: Aabb = (&a.bbox).into();
                let d = bbox.distance_to_local_point(&p, false).abs();
                println!("{} is {} away", s.0, d);
                let contained = bbox.contains_local_point(&p);
                if (d < closest_distance) && ((must_contain && contained) || !must_contain) {
                    closest_distance = d;
                    result = Some((s.0, d, contained));
                }
            }
        }

        result        
    }


    /// Get all annotations that are containing the given `frame`.
    /// Each result is a pair of (index of the AnnotationSequence, (start index in the AnnotationSequence, optional end index)).
    pub fn get_all_annotations_for_frame(&self, frame: usize) -> Vec<(usize, (usize, Option<usize>))> {
        let mut result = Vec::new();
        let ann = &self.sequences;
        for (i, a) in ann.iter().enumerate() {
            if let Some(b) = a.get_annotations_for_frame(frame) {
                result.push((i, b));
            }
        }
        result
    }


    /// Given a result from `get_all_interpolated_annotations_for_frame`, get the closest one to point (x,y).
    /// Returns None if annotations is empty.
    pub fn get_closest_annotation(x: f32, y: f32, annotations: &Vec<(usize, Annotation)>) -> Option<(usize, Annotation)> {
        let mut d = f32::MAX;
        let mut result = None;
        let p = Point::new(x, y);
        for (i, a) in annotations {
            let bbox: Aabb = (&a.bbox).into();
            let dd = bbox.distance_to_local_point(&p, false);
            if dd < d {
                d = dd;
                result = Some((*i, a.clone()));
            }
        }
        result
    }


    /// Get all interpolated annotations, i.e. all annotations that should e.g. be shown to the user.
    /// Returns a tuple (annotation sequence index, annotation).
    pub fn get_all_interpolated_annotations_for_frame(&self, frame: usize) -> Vec<(usize, Annotation)> {
        let mut result = Vec::new();
        let ann = &self.sequences;
        for (i, a) in ann.iter().enumerate() {
            if let Some(b) = a.get_interpolated_annotation_for_frame(frame) {
                result.push((i, b));
            }
        }
        result        
    }

    /// Add an entirely new annotation object, as opposed to editing an existing one.
    pub fn add_new_annotation_sequence(&mut self, ann: Annotation) {
        let mut anns = AnnotationSequence::new();
        anns.annotations.push(ann);
        self.sequences.push(anns);
    }

}


#[derive(Serialize, Deserialize, Clone)]
pub struct Annotation {
    /// The number of elements in `labels` is determined by the LabelConfigs used for this Annotation.
    pub labels: Vec<Label>,
    pub bbox: SerializableAabb,
    pub frame: usize,
    /// Indicates if the annotation has left the frame (i.e. the annotation stops here in a sequence).
    pub invisible: bool,
    pub interpolated: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializablePoint<T> {
    pub x: T,
    pub y: T
}

impl<T: Copy> SerializablePoint<T> {
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

/// This is introduced so that we can use automatic Serialize/Deserialize derivation. parry2d's Aabb does not implement that.
#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableAabb {
    pub mins: SerializablePoint<f32>,
    pub maxs: SerializablePoint<f32>
}

impl Into<Aabb> for &SerializableAabb {
    fn into(self) -> Aabb {
        Aabb { mins: OPoint::from([self.mins.x, self.mins.y]),
               maxs: OPoint::from([self.maxs.x, self.maxs.y]) }
    }
}

impl Annotation {
    pub fn new(config: &LabelConfigs, start_x: f32, start_y: f32, frame: usize) -> Self {
        let mut result = Self {
            labels: vec![],
            bbox: SerializableAabb{ mins: SerializablePoint{x: start_x, y: start_y }, maxs: SerializablePoint { x: start_x, y: start_y } },
            frame,
            invisible: false,
            interpolated: false
        };

        for c in &config.label_configs {
            match c {
                LabelConfig::S(lcs) => {
                    result.labels.push(Label::S(LabelInstance { name: lcs.name.clone(), state: lcs.states[0].clone() }));
                },
                LabelConfig::I(lci) => {
                    result.labels.push(Label::I(LabelInstance { name: lci.name.clone(), state: lci.first }));
                }
            }
        }

        result
    }

    /// Returns the distance and whether the point is inside the box.
    pub fn distance(&self, x: f32, y: f32) -> (f32, bool) {
        let p = Point::new(x, y);
        let bbox: Aabb = (&self.bbox).into();
        let d = bbox.distance_to_local_point(&p, false);
        (d, bbox.contains_local_point(&p))
    }

    /// Get the corner point identifier, the distance of (x,y) to the point, and the position of the point.
    pub fn closest_corner_point(&self, x: f32, y: f32) -> (BoxCorner, f32, Point2<f32>) {
        let corners = [BoxCorner::LU, BoxCorner::RU, BoxCorner::RD, BoxCorner::LD];
        let points = [BoxCorner::LU.from_serializable_aabb(&self.bbox), 
                                                  BoxCorner::RU.from_serializable_aabb(&self.bbox), 
                                                  BoxCorner::RD.from_serializable_aabb(&self.bbox), 
                                                  BoxCorner::LD.from_serializable_aabb(&self.bbox)];
        let mut min_dist = f32::MAX;
        let mut min_corner = BoxCorner::LU;
        let p = Point::new(x, y);
        for (i, point) in points.iter().enumerate() {
            let dist = (p - point).norm();
            if dist < min_dist {
                min_dist = dist;
                min_corner = corners[i];
            }
        }

        (min_corner, min_dist, min_corner.from_serializable_aabb(&self.bbox))        
    }
}


impl Default for Annotation {
    fn default() -> Self {
        Annotation::new(&LabelConfigs::default(), 0.0, 0.0,0)
    }
}

/// Sequence of annotations, i.e. a sequence of boxes that are interpolated between.
#[derive(Clone, Serialize, Deserialize)]
pub struct AnnotationSequence {
    pub annotations: Vec<Annotation>
}

impl AnnotationSequence {
    pub fn new() -> Self {
        Self { annotations: Vec::<Annotation>::new() }
    }

    /// Propagate the labels (not the rectangles) from the given frame to all following key frames.
    pub fn propagate(&mut self, frame: usize) {
        let ann = self.get_interpolated_annotation_for_frame(frame);
        if let Some(ann) = ann {
            for a in &mut self.annotations {
                if a.frame >= frame {
                    a.invisible = ann.invisible;
                    a.labels = ann.labels.clone();
                }
            }
        }
    }

    pub fn get_interpolated_annotation_for_frame(&self, frame: usize) -> Option<Annotation> {
        let anns = self.get_annotations_for_frame(frame);
        if let Some((index0, index1_)) = anns {
            let frame0 = self.annotations[index0].frame;
            if frame0 == frame {
                return Some(self.annotations[index0].clone());
            } else {
                // if self.annotations[index0].out_of_frame {
                //     return None;
                // }

                if let Some(index1) = index1_ {
                    let frame1 = self.annotations[index1].frame;
                    assert!(frame0 < frame && frame1 > frame);

                    //
                    // Create interpolated annotation.
                    let mut a = Annotation::default();
                    a.invisible = self.annotations[index0].invisible;
                    a.frame = frame;
                    a.interpolated = true;
                    a.labels = self.annotations[index0].labels.clone();
                    let t = (frame - frame0) as f32 / (frame1 - frame0) as f32;
                    let mins0 = &self.annotations[index0].bbox.mins;
                    let maxs0 = &self.annotations[index0].bbox.maxs;
                    let mins1 = &self.annotations[index1].bbox.mins;
                    let maxs1 = &self.annotations[index1].bbox.maxs;
                    
                    // OPoint does not seem to have addition defined on it.
                    let mins = (1.0 - t) * Vec2::new(mins0.x, mins0.y) + t * Vec2::new(mins1.x, mins1.y);
                    let maxs = (1.0 - t) * Vec2::new(maxs0.x, maxs0.y) + t * Vec2::new(maxs1.x, maxs1.y);

                    a.bbox.mins.x = mins.x;
                    a.bbox.mins.y = mins.y;
                    a.bbox.maxs.x = maxs.x;
                    a.bbox.maxs.y = maxs.y;
                    return Some(a);
                } else {
                    // This means we can just return the first annotation, since we extrapolate the same one, if it's not out of frame.
                    let mut a = self.annotations[index0].clone();
                    a.interpolated = true;
                    a.frame = frame;
                    return Some(a);
                }
            }
        }
        return None;
    }

    /// Get indices for annotations below and above the given frame.
    /// If the frame is matching an annotation exactly, then both indices will be the same.
    pub fn get_annotations_for_frame(&self, frame: usize) -> Option<(usize, Option<usize>)> {
        // TODO: replace with binary search.
        for i in 0..self.annotations.len() {

            // Found a perfect match, the frame has an annotation in the sequence:
            if self.annotations[i].frame == frame {
                return Some((i, None));
            }

            if self.annotations[i].frame > frame {
                // There is no annotation below or at the requested frame:
                if i == 0 {
                    return None;
                }

                // Found a pair of annotations that the frame falls in between: 
                return Some((i-1, Some(i)))    
            }
        }

        if !self.annotations.is_empty() {
            if self.annotations[self.annotations.len() - 1].frame < frame {
                return Some((self.annotations.len() - 1, None));
            }
        }

        None
    }

        /// Either edits the annotation if there already is one in the current AnnotationSequence in this frame,
    /// or adds a new keyframe annotation to this annotationsequence otherwise.
    pub fn edit_annotation(&mut self, frame: usize, annotation: &Annotation) {
        let indices_ = self.get_annotations_for_frame(frame);
        if let Some(indices) = indices_ {

            let frame0 = self.annotations[indices.0].frame;

            if frame0 == frame {
                let mut a = annotation.clone();
                a.frame = frame;
                a.interpolated = false;
                self.annotations[indices.0] = a;
            } else {
                if self.annotations.len() - 1 == indices.0 {
                    let mut a = annotation.clone();
                    a.frame = frame;
                    a.interpolated = false;
                    self.annotations.push(a);
                } else {
                    let mut a = annotation.clone();
                    a.frame = frame;
                    a.interpolated = false;
                    self.annotations.insert(indices.0 + 1, a);
                }
            }
        }
    }
}


// Types for annotation editing:
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum BoxCorner {LU, RU, RD, LD}

impl BoxCorner {
    pub fn from_aabb(&self, b: &Aabb) -> Point2<f32> {
        match self {
            BoxCorner::LU => { return Point::new(b.mins.x, b.mins.y) },
            BoxCorner::RU => { return Point::new(b.maxs.x, b.mins.y) },
            BoxCorner::RD => { return Point::new(b.maxs.x, b.maxs.y) },
            BoxCorner::LD => { return Point::new(b.mins.x, b.maxs.y) }
        }
    }

    pub fn from_serializable_aabb(&self, b: &SerializableAabb) -> Point2<f32> {
        match self {
            BoxCorner::LU => { return Point::new(b.mins.x, b.mins.y) },
            BoxCorner::RU => { return Point::new(b.maxs.x, b.mins.y) },
            BoxCorner::RD => { return Point::new(b.maxs.x, b.maxs.y) },
            BoxCorner::LD => { return Point::new(b.mins.x, b.maxs.y) }
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum ActionType {
    None,
    New,
    ModifyCorner(BoxCorner),
    Move(Vec2),
}

pub struct AnnotationAction {
    // annotation: Annotation,
    pub action_type: ActionType
}

impl AnnotationAction {
    pub fn new() -> Self {
        Self {
            action_type: ActionType::New
        }
    }
}

impl Default for AnnotationAction {
    fn default() -> Self {
        AnnotationAction::new()
    }
}