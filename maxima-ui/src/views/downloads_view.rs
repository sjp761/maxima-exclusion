use egui::{load::SizedTexture, pos2, vec2, Align2, Color32, FontId, ImageSource, Mesh, Rect, Rounding, Shape, Stroke, Ui};

use crate::{bridge_thread, GameUIImages, GameUIImagesWrapper, MaximaEguiApp, APP_MARGIN};

#[derive(Clone)]
pub struct QueuedDownload {
    pub slug: String,
    pub downloaded_bytes: usize,
    pub total_bytes: usize,
    // maybe add a thing here for updates? idk there's no real api to hook this up to yet
}

fn render_queued(app: &mut MaximaEguiApp, ui: &mut Ui, game: &QueuedDownload) {
    ui.spacing_mut().item_spacing.y = APP_MARGIN.y;
    let size = vec2(ui.available_width(), 160.0);
    ui.allocate_ui(size, |ui| {
        let game = app.games.get_mut(&game.slug).unwrap();
        let game_images: Option<&GameUIImages> = match &game.images { // TODO: just replace this entire system with the one i made in a newer project
        GameUIImagesWrapper::Unloaded => {
            app.backend.backend_commander.send(bridge_thread::MaximaLibRequest::GetGameImagesRequest(game.slug.clone())).unwrap();
            game.images = GameUIImagesWrapper::Loading;
            None
        },
        GameUIImagesWrapper::Loading => {
            None
        },
        GameUIImagesWrapper::Available(images) => {
            Some(images) },
        };
        let container_rect = Rect {
            min: ui.cursor().min,
            max: ui.cursor().min + size
        };

        let corner_radius = 8.0;

        ui.painter().rect_filled(container_rect, Rounding::same(corner_radius), Color32::BLACK);

        if let Some(img) = game_images {
            let img_rounding = Rounding { nw: corner_radius, ne: 0.0, sw: corner_radius, se: 0.0 };
            let img_response = ui.add(egui::Image::new((img.hero.renderable, img.hero.size)).rounding(img_rounding).max_size(size));

            let top_left =      pos2(img_response.rect.max.x - 80.0, img_response.rect.min.y);
            let top_right =     pos2(img_response.rect.max.x - 00.0, img_response.rect.min.y);
            let top_righter =     pos2(img_response.rect.max.x + 10.0, img_response.rect.min.y);
            let bottom_left =   pos2(img_response.rect.max.x - 80.0, img_response.rect.max.y);
            let bottom_right =  pos2(img_response.rect.max.x - 00.0, img_response.rect.max.y);
            let bottom_righter =  pos2(img_response.rect.max.x + 10.0, img_response.rect.max.y);

            let mut mesh = Mesh::default();
            mesh.colored_vertex(top_left, Color32::TRANSPARENT);
            mesh.colored_vertex(bottom_left, Color32::TRANSPARENT);
            mesh.colored_vertex(top_right, Color32::BLACK);
            mesh.colored_vertex(bottom_right, Color32::BLACK);
            mesh.colored_vertex(top_righter, Color32::BLACK);
            mesh.colored_vertex(bottom_righter, Color32::BLACK);

            mesh.add_triangle(1, 3, 2);
            mesh.add_triangle(1, 2, 0);
            mesh.add_triangle(3, 5, 4);
            mesh.add_triangle(3, 4, 2);
            ui.painter().add(Shape::mesh(mesh));

            if let Some(logo) = &img.logo {
                let logo_rect = img_response.rect.clone().shrink(26.0);
                ui.put(logo_rect, egui::Image::new((logo.renderable, logo.size)).maintain_aspect_ratio(true).fit_to_exact_size(logo_rect.size()));
                //ui.painter().image(logo.renderable, logo_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }

            ui.painter().rect(container_rect, Rounding::same(corner_radius), Color32::TRANSPARENT, Stroke::new(2.0, Color32::WHITE));

            ui.painter().text(pos2(img_response.rect.max.x + 10.0, img_response.rect.min.y + 4.0), Align2::LEFT_TOP, &game.name, FontId::proportional(32.0), Color32::WHITE);

            let progress_bar_rect = Rect {
                min: img_response.rect.max + vec2(0.0, -16.0),
                max: container_rect.max - vec2(6.0, 6.0)
            };

            ui.painter().rect_filled(progress_bar_rect, Rounding::same(corner_radius - 6.0), Color32::RED);
        }
        
    });
}

pub fn downloads_view(app : &mut MaximaEguiApp, ui: &mut Ui) {
    for game in app.install_queue.clone() {
        render_queued(app, ui, &game);
    }
}