use crate::map::objects::*;
use egui::{widgets::*, *};
use kdtree::distance::squared_euclidean;
use kdtree::KdTree;
use rand::distributions::{Alphanumeric, Distribution};
use rand::thread_rng;
use std::collections::HashMap;

pub mod objects;

// This can by any object or point with its associated metadata
/// Struct that contains coordinates to help calculate nearest point in space

pub struct Map {
    pub zoom: f32,
    previous_zoom: f32,
    points: Option<HashMap<usize, MapPoint>>,
    lines: Vec<MapLine>,
    labels: Vec<MapLabel>,
    tree: Option<KdTree<f64, usize, [f64; 2]>>,
    visible_points: Option<Vec<usize>>,
    map_area: Option<Rect>,
    initialized: bool,
    reference: MapBounds,
    current: MapBounds,
    style: egui::Style,
    current_index: usize,
    pub settings: MapSettings,
}

impl Default for Map {
    fn default() -> Self {
        Map::new()
    }
}

impl Widget for &mut Map {
    fn ui(self, ui_obj: &mut egui::Ui) -> Response {
        if !self.initialized {
            #[cfg(feature = "puffin")]
            puffin::profile_scope!("map_init");

            let mut rng = thread_rng();
            let component_id: String = Alphanumeric
                .sample_iter(&mut rng)
                .take(15)
                .map(char::from)
                .collect();
            // TODO:use this variable
            let _idx = egui::Id::new(component_id);
            self.map_area = Some(ui_obj.available_rect_before_wrap());
        } else {
            self.map_area = Some(ui_obj.ctx().used_rect());
        }

        self.asign_visual_style(ui_obj);

        let canvas = egui::Frame::canvas(ui_obj.style());

        self.capture_mouse_events(ui_obj);

        let inner_response = canvas.show(ui_obj, |ui_obj| {
            #[cfg(feature = "puffin")]
            puffin::profile_scope!("paint_map");

            //if ui_obj.is_rect_visible(self.map_area.unwrap()) {
            let (resp, paint) = ui_obj
                .allocate_painter(self.map_area.unwrap().size(), egui::Sense::click_and_drag());
            let vec = resp.drag_delta();
            if vec.length() != 0.0 {
                #[cfg(feature = "puffin")]
                puffin::profile_scope!("calculating_points_in_visible_area");

                let coords = (vec.to_pos2().x, vec.to_pos2().y);
                self.set_pos(self.current.pos.x - coords.0, self.current.pos.y - coords.1);
                self.calculate_visible_points();
            }
            let map_style = self.settings.styles[self.current_index].clone() * self.zoom;
            if self.zoom < self.settings.line_visible_zoom {
                // filling text settings
                let mut text_settings = TextSettings {
                    size: 12.00 * self.zoom,
                    anchor: Align2::CENTER_CENTER,
                    family: FontFamily::Proportional,
                    text: String::new(),
                    position: Pos2::new(0.00, 0.00),
                    text_color: ui_obj.visuals().text_color(),
                };
                for label in &self.labels {
                    text_settings.text = label.text.clone();
                    paint.text(
                        label.center,
                        Align2::CENTER_CENTER,
                        label.text.as_str(),
                        map_style.font.clone().unwrap(),
                        ui_obj.visuals().text_color(),
                    );
                    self.paint_label(&paint, &text_settings);
                }
            }

            let vec_points = &self.visible_points;
            let hashm = &self.points;
            let factor = (
                self.map_area.unwrap().center().x + self.map_area.unwrap().min.x,
                self.map_area.unwrap().center().y + self.map_area.unwrap().min.y,
            );
            let min_point = Pos2::new(self.current.pos.x - factor.0, self.current.pos.y - factor.1);

            let _a = self.paint_map_lines(vec_points, hashm, &paint, &min_point);
            let _b = self.paint_map_points(vec_points, hashm, &paint, ui_obj, &min_point, &resp);

            self.paint_sub_components(ui_obj, self.map_area.unwrap());

            if self.zoom != self.previous_zoom {
                #[cfg(feature = "puffin")]
                puffin::profile_scope!("calculating viewport with zoom");
                self.adjust_bounds();
                self.calculate_visible_points();
                self.previous_zoom = self.zoom;
            }

            self.hover_management(ui_obj, &paint, &resp);

            if cfg!(debug_assertions) {
                self.print_debug_info(paint, resp);
            }
            //}
        });
        inner_response.response
    }
}

impl Map {
    pub fn new() -> Self {
        let settings = MapSettings::new();
        Map {
            zoom: 1.0,
            previous_zoom: 1.0,
            map_area: None,
            tree: None,
            points: None,
            lines: Vec::new(),
            labels: Vec::new(),
            visible_points: None,
            initialized: false,
            current: MapBounds::default(),
            reference: MapBounds::default(),
            settings,
            current_index: 0,
            style: egui::Style::default(),
        }
    }

    fn calculate_visible_points(&mut self) {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!("calculate_visible_points");
        if self.current.dist > 0.0 {
            if let Some(tree) = &self.tree {
                let center = [
                    (self.current.pos.x / self.zoom) as f64,
                    (self.current.pos.y / self.zoom) as f64,
                ];
                let radius = self.current.dist.powi(2);
                let vis_pos = tree.within(&center, radius, &squared_euclidean).unwrap();
                let mut visible_points = vec![];
                for point in vis_pos {
                    visible_points.push(*point.1);
                }
                self.visible_points = Some(visible_points);
            }
        }
    }

    pub fn add_hashmap_points(&mut self, hash_map: HashMap<usize, MapPoint>) {
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut tree = KdTree::<f64, usize, [f64; 2]>::new(2);
        let mut h_map = hash_map.clone();
        for entry in h_map.iter_mut() {
            entry.1.coords[0] *= -1.0;
            entry.1.coords[1] *= -1.0;
            if entry.1.coords[0] < min.0 {
                min.0 = entry.1.coords[0];
            }
            if entry.1.coords[1] < min.1 {
                min.1 = entry.1.coords[1];
            }
            if entry.1.coords[0] > max.0 {
                max.0 = entry.1.coords[0];
            }
            if entry.1.coords[1] > max.1 {
                max.1 = entry.1.coords[1];
            }
            let _result = tree.add([entry.1.coords[0], entry.1.coords[1]], *entry.0);
            for line in &mut entry.1.lines {
                line[0] *= -1.0;
                line[1] *= -1.0;
                line[2] *= -1.0;
            }
        }
        self.reference.min = Pos2::new(min.0 as f32, min.1 as f32);
        self.reference.max = Pos2::new(max.0 as f32, max.1 as f32);
        self.points = Some(h_map);
        self.tree = Some(tree);
        let rect = Rect::from_min_max(self.reference.min, self.reference.max);
        self.reference.pos = rect.center();
        let dist_x = (self.map_area.unwrap().right_bottom().x as f64
            - self.map_area.unwrap().left_top().x as f64)
            / 2.0;
        let dist_y = (self.map_area.unwrap().right_bottom().y as f64
            - self.map_area.unwrap().left_top().y as f64)
            / 2.0;
        self.reference.dist = (dist_x.powi(2) + dist_y.powi(2) / 2.0).sqrt();
        self.current = self.reference.clone();
        self.calculate_visible_points();
    }

    pub fn set_pos(&mut self, x: f32, y: f32) {
        if x <= self.current.max.x
            && x >= self.current.min.x
            && y <= self.current.max.y
            && y >= self.current.min.y
        {
            self.current.pos = Pos2::new(x, y);
            self.reference.pos = Pos2::new(x / self.zoom, y / self.zoom);
        }
    }

    pub fn add_labels(&mut self, labels: Vec<MapLabel>) {
        self.labels = labels;
    }

    pub fn add_lines(&mut self, lines: Vec<MapLine>) {
        self.lines = lines
    }

    fn adjust_bounds(&mut self) {
        self.current.max.x = self.reference.max.x * self.zoom;
        self.current.max.y = self.reference.max.y * self.zoom;
        self.current.min.x = self.reference.min.x * self.zoom;
        self.current.min.y = self.reference.min.y * self.zoom;
        self.current.dist = self.reference.dist / self.zoom as f64;
        self.set_pos(
            self.reference.pos.x * self.zoom,
            self.reference.pos.y * self.zoom,
        );
    }

    fn capture_mouse_events(&mut self, ui_obj: &Ui) {
        // capture MouseWheel Event for Zoom control change
        ui_obj.input(|x| {
            #[cfg(feature = "puffin")]
            puffin::profile_scope!("capture_mouse_events");

            if !x.events.is_empty() {
                for event in &x.events {
                    match event {
                        Event::MouseWheel {
                            unit: _,
                            delta,
                            modifiers,
                        } => {
                            let zoom_modifier = if modifiers.mac_cmd {
                                delta.y / 80.00
                            } else {
                                delta.y / 400.00
                            };
                            let mut pre_zoom = self.zoom + zoom_modifier;
                            if pre_zoom > self.settings.max_zoom {
                                pre_zoom = self.settings.max_zoom;
                            }
                            if pre_zoom < self.settings.min_zoom {
                                pre_zoom = self.settings.min_zoom;
                            }
                            self.zoom = pre_zoom;
                        }
                        _ => {
                            continue;
                        }
                    };
                }
            }
        });
    }

    fn asign_visual_style(&mut self, ui_obj: &mut Ui) {
        let style_index = ui_obj.visuals().dark_mode as usize;

        if self.current_index != style_index {
            #[cfg(feature = "puffin")]
            puffin::profile_scope!("asign_visual_style");

            self.current_index = style_index;
            self.style = ui_obj.style_mut().clone();
            self.style.visuals.extreme_bg_color =
                self.settings.styles[style_index].background_color;
            self.style.visuals.window_stroke = self.settings.styles[style_index].border.unwrap();
        }
    }

    fn print_debug_info(&mut self, paint: Painter, resp: Response) {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!("printing debug data");

        let mut init_pos = Pos2::new(
            self.map_area.unwrap().left_top().x + 10.00,
            self.map_area.unwrap().left_top().y + 10.00,
        );
        let mut msg = "MIN:".to_string()
            + self.current.min.x.to_string().as_str()
            + ","
            + self.current.min.y.to_string().as_str();
        paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        init_pos.y += 15.0;
        msg = "MAX:".to_string()
            + self.current.max.x.to_string().as_str()
            + ","
            + self.current.max.y.to_string().as_str();
        paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        init_pos.y += 15.0;
        msg = "CUR:(".to_string()
            + self.current.pos.x.to_string().as_str()
            + ","
            + self.current.pos.y.to_string().as_str()
            + ")";
        paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        init_pos.y += 15.0;
        msg = "DST:".to_string() + self.current.dist.to_string().as_str();
        paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        init_pos.y += 15.0;
        msg = "ZOM:".to_string() + self.zoom.to_string().as_str();
        paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::GREEN, msg);
        if let Some(rectz) = self.map_area {
            init_pos.y += 15.0;
            msg = "REC:(".to_string()
                + rectz.left_top().x.to_string().as_str()
                + ","
                + rectz.left_top().y.to_string().as_str()
                + "),("
                + rectz.right_bottom().x.to_string().as_str()
                + ","
                + rectz.right_bottom().y.to_string().as_str()
                + ")";
            paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        }
        if let Some(points) = &self.points {
            init_pos.y += 15.0;
            msg = "NUM:".to_string() + points.len().to_string().as_str();
            paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        }
        if let Some(vec_k) = self.visible_points.as_ref() {
            init_pos.y += 15.0;
            msg = "VIS:".to_string() + vec_k.len().to_string().as_str();
            paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_GREEN, msg);
        }
        if let Some(pointer_pos) = resp.hover_pos() {
            init_pos.y += 15.0;
            msg = "HVR:".to_string()
                + pointer_pos.x.to_string().as_str()
                + ","
                + pointer_pos.y.to_string().as_str();
            paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::LIGHT_BLUE, msg);
        }
        let vec = resp.drag_delta();
        if vec.length() != 0.0 {
            init_pos.y += 15.0;
            msg = "DRG:".to_string()
                + vec.to_pos2().x.to_string().as_str()
                + ","
                + vec.to_pos2().y.to_string().as_str();
            paint.debug_text(init_pos, Align2::LEFT_TOP, Color32::GOLD, msg);
        }
    }

    fn paint_sub_components(&mut self, ui_obj: &mut Ui, rect: Rect) {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!("map_ui_paint_sub_components");
        let zoom_slider = egui::Slider::new(
            &mut self.zoom,
            self.settings.min_zoom..=self.settings.max_zoom,
        )
        .show_value(false)
        //.step_by(0.1)
        .orientation(SliderOrientation::Vertical);
        let mut pos1 = rect.right_top();
        let mut pos2 = rect.right_top();
        pos1.x -= 80.0;
        pos1.y += 120.0;
        pos2.x -= 50.0;
        pos2.y += 240.0;
        let sub_rect = egui::Rect::from_two_pos(pos1, pos2);
        ui_obj.allocate_ui_at_rect(sub_rect, |ui_obj| {
            ui_obj.add(zoom_slider);
        });
    }

    fn hover_management(&mut self, _ui_obj: &Ui, _paint: &Painter, resp: &Response) {
        if resp.secondary_clicked() {
            todo!();
        }
    }

    fn paint_map_points(
        &self,
        vec_points: &Option<Vec<usize>>,
        hashm: &Option<HashMap<usize, MapPoint>>,
        paint: &Painter,
        ui_obj: &Ui,
        min_point: &Pos2,
        resp: &Response,
    ) -> Result<(), ()> {
        let mut nearest_id = None;
        if hashm.is_none() {
            return Err(());
        }
        if vec_points.is_none() {
            return Err(());
        }
        // detecting the nearest hover node
        if self.settings.node_text_visibility == VisibilitySetting::Hover && resp.hovered() {
            if let Some(point) = resp.hover_pos() {
                let hovered_map_point = Pos2::new(
                    point.x - min_point.x,
                    point.y - min_point.y
                );
                if let Ok(nearest_node) = self.tree.as_ref().unwrap().nearest(
                    &[hovered_map_point.x as f64, hovered_map_point.y as f64],
                    1,
                    &squared_euclidean,
                ) {
                    nearest_id = Some(nearest_node.first().unwrap().1);
                }
            }
        }
        // filling text settings
        let mut text_settings = TextSettings {
            size: 12.00 * self.zoom,
            anchor: Align2::LEFT_BOTTOM,
            family: FontFamily::Proportional,
            text: String::new(),
            position: Pos2::new(0.00, 0.00),
            text_color: ui_obj.visuals().text_color(),
        };

        // Drawing Points
        for temp_point in vec_points.as_ref().unwrap() {
            if let Some(system) = hashm.as_ref().unwrap().get(temp_point) {
                #[cfg(feature = "puffin")]
                puffin::profile_scope!("painting_points_m");
                let center = Pos2::new(
                    system.coords[0] as f32 * self.zoom,
                    system.coords[1] as f32 * self.zoom,
                );
                let viewport_point = Pos2::new(center.x - min_point.x, center.y - min_point.y);
                if self.zoom > self.settings.label_visible_zoom
                    && (self.settings.node_text_visibility == VisibilitySetting::Allways
                        || (self.settings.node_text_visibility == VisibilitySetting::Hover
                            && nearest_id.unwrap_or(&0usize) == &system.id))
                {
                    let mut viewport_text = viewport_point;
                    viewport_text.x += 3.0 * self.zoom;
                    viewport_text.y -= 3.0 * self.zoom;
                    text_settings.position = viewport_text;
                    text_settings.text = system.name.to_string();
                    self.paint_label(paint, &text_settings);
                }
                paint.circle(
                    viewport_point,
                    4.00 * self.zoom,
                    self.settings.styles[self.current_index].fill_color,
                    self.settings.styles[self.current_index].border.unwrap(),
                );
            }
        }
        Ok(())
    }

    fn paint_map_lines(
        &self,
        vec_points: &Option<Vec<usize>>,
        hashm: &Option<HashMap<usize, MapPoint>>,
        paint: &Painter,
        min_point: &Pos2,
    ) -> Result<(), ()> {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!("paint_map_lines");

        if hashm.is_none() {
            return Err(());
        }
        if vec_points.is_none() {
            return Err(());
        }

        // Drawing Lines
        if self.zoom > self.settings.line_visible_zoom {
            for temp_point in vec_points.as_ref().unwrap() {
                if let Some(system) = hashm.as_ref().unwrap().get(temp_point) {
                    let center = Pos2::new(
                        system.coords[0] as f32 * self.zoom,
                        system.coords[1] as f32 * self.zoom,
                    );
                    let a_point = Pos2::new(center.x - min_point.x, center.y - min_point.y);
                    for line in &system.lines {
                        let b_point = Pos2::new(
                            (line[0] as f32 * self.zoom) - min_point.x,
                            (line[1] as f32 * self.zoom) - min_point.y,
                        );
                        paint.line_segment(
                            [a_point, b_point],
                            self.settings.styles[self.current_index].line.unwrap(),
                        );
                    }
                }
            }
            for line in &self.lines {
                paint.line_segment(
                    line.points,
                    self.settings.styles[self.current_index].line.unwrap(),
                );
            }
        }
        Ok(())
    }

    fn paint_label(&self, paint: &Painter, text_settings: &TextSettings) {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!("paint_label");
        paint.text(
            text_settings.position,
            text_settings.anchor,
            text_settings.text.clone(),
            FontId::new(text_settings.size, text_settings.family.clone()),
            text_settings.text_color,
        );
    }
}
