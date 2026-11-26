//! Interactive numeric graph mapper inspector, sharing the evaluator's curve function.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use morphlet::{math_nodes::mapper, model::Node};

const MODES: [&str; 6] = ["線形", "ベジェ", "正弦", "余弦", "ガウス", "指数"];

fn read(node: &Node, key: &str, fallback: f64) -> f64 {
    node.params.get(key).copied().unwrap_or(fallback)
}
fn position(rect: Rect, x: f64, y: f64) -> Pos2 {
    Pos2::new(rect.left() + rect.width()*x as f32, rect.bottom()-rect.height()*y as f32)
}

pub fn show(ui: &mut egui::Ui, node: &mut Node) {
    ui.separator();
    ui.label("グラフマッパー • 0–1");
    let mut mode = read(node, "mode", 1.0) as usize;
    if !node.inputs.contains_key("mode") {
        egui::ComboBox::from_id_salt(("mapper_mode",node.id))
            .selected_text(MODES.get(mode).copied().unwrap_or("不明"))
            .show_ui(ui, |ui| {
                for (i, label) in MODES.iter().enumerate() {
                    ui.selectable_value(&mut mode, i, *label);
                }
            });
        if mode < MODES.len() {
            node.params.insert("mode".into(), mode as f64);
        }
    }
    let connected = ["mode","x1","y1","x2","y2"].iter().any(|k| node.inputs.contains_key(*k));
    let mut c = [read(node,"x1",0.25),read(node,"y1",0.1),read(node,"x2",0.75),read(node,"y2",0.9)];
    let width = ui.available_width().max(64.0);
    let (outer, _) = ui.allocate_exact_size(Vec2::new(width, 172.0), Sense::hover());
    let rect = outer.shrink(12.0);
    let painter = ui.painter_at(outer);
    let fg = ui.visuals().text_color();
    let grid = ui.visuals().weak_text_color().gamma_multiply(0.35);
    for j in 0..=4 {
        let t = j as f64 / 4.0;
        painter.line_segment([position(rect,t,0.0),position(rect,t,1.0)],Stroke::new(1.0,grid));
        painter.line_segment([position(rect,0.0,t),position(rect,1.0,t)],Stroke::new(1.0,grid));
    }
    if connected {
        ui.small("接続中の曲線設定は上流で変更してください。ここには未接続時の設定を表示します。");
    }
    if mode == 1 && !connected {
        for k in 0..2 {
            let handle = position(rect,c[k*2],c[k*2+1]);
            let hit = Rect::from_center_size(handle,Vec2::splat(22.0));
            let response = ui.interact(hit, ui.id().with(("mapper_handle",node.id,k)), Sense::drag());
            if response.dragged() {
                if let Some(p) = response.interact_pointer_pos() {
                    let x = ((p.x-rect.left())/rect.width()).clamp(0.0,1.0) as f64;
                    let y = ((rect.bottom()-p.y)/rect.height()).clamp(0.0,1.0) as f64;
                    c[k*2] = if k==0 {x.min(c[2])} else {x.max(c[0])};
                    c[k*2+1]=y;
                    node.params.insert(if k==0 {"x1"} else {"x2"}.into(),c[k*2]);
                    node.params.insert(if k==0 {"y1"} else {"y2"}.into(),c[k*2+1]);
                }
            }
        }
    }
    if mode == 1 {
        for k in 0..2 {
            let p=position(rect,c[k*2],c[k*2+1]);
            let endpoint=if k==0 {position(rect,0.0,0.0)} else {position(rect,1.0,1.0)};
            painter.line_segment([endpoint,p],Stroke::new(1.0,fg.gamma_multiply(0.5)));
            painter.circle_filled(p,5.0,Color32::from_rgb(235,176,92));
        }
        if !connected && ui.small_button("制御点をリセット").clicked() {
            for (key,value) in [("x1",0.25),("y1",0.1),("x2",0.75),("y2",0.9)] {
                node.params.insert(key.into(),value);
            }
        }
    }
    let color=Color32::from_rgb(74,170,157);
    let mut points=Vec::with_capacity(129);
    for j in 0..=128 {
        let x=j as f64/128.0;
        if let Ok(y)=mapper(x,mode,c) {points.push(position(rect,x,y));}
    }
    if points.len()>1 {painter.add(egui::Shape::line(points,Stroke::new(2.0,color)));}
    if !node.inputs.contains_key("x") && !connected {
        let x=read(node,"x",0.5);
        if let Ok(y)=mapper(x,mode,c) {
            let p=position(rect,x,y);
            painter.line_segment([position(rect,x,0.0),p],Stroke::new(1.0,color));
            painter.circle_filled(p,4.0,color);
            ui.small(format!("x = {x:.3} → y = {y:.3}"));
        }
    }
}
