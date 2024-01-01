use egui::{Ui, Color32, Margin, vec2, Rounding, Stroke, Sense};

use crate::DemoEguiApp;

#[derive(Debug, PartialEq, Default)]
pub enum FriendsViewBarStatusFilter {
  #[default] Name,
  Game,
}

#[derive(Debug, PartialEq, Default)]
pub enum FriendsViewBarPage {
  #[default] Online,
  All,
  Pending,
  Blocked
}

pub struct FriendsViewBar {
  /// What page the user is currently on
  pub page : FriendsViewBarPage,
  /// The value of the criteria ComboBox
  pub status_filter : FriendsViewBarStatusFilter,
  //search_category : FriendViewBarSearchCategory,
  /// The buffer for the search box
  pub search_buffer : String,
}

struct Friend {
  name : String,
  online : bool,
  game : Option<String>,
  game_presence : Option<String>
}

const F9B233: Color32 = Color32::from_rgb(249, 178, 51);
const DARK_GREY: Color32 = Color32::from_rgb(53, 53, 53);


pub fn friends_view(app : &mut DemoEguiApp, ui: &mut Ui) {
  let friends_raw : Vec<Friend> = Vec::from(
    [
      Friend {
        name : "AMoistEggroll".to_owned(),
        online : false,
        game: None,
        game_presence: None,
      },
      Friend {
        name : "BattleDash".to_owned(),
        online : true,
        game: Some("Battlefield 2042".to_owned()),
        game_presence: None,
      },
      Friend {
        name : "GEN_Burnout".to_owned(),
        online : true,
        game: None,
        game_presence: None,
      },
      Friend {
        name : "KursedKrabbo".to_owned(),
        online : true,
        game: Some("Titanfall 2".to_owned()),
        game_presence: Some("Pilots vs Pilots on Glitch".to_owned()),
      }
    ]
  );

  let top_bar = egui::Frame::default()
  //.fill(Color32::from_gray(255))
  .outer_margin(Margin::same(-4.0))
  .inner_margin(Margin::same(5.0));
  
  top_bar.show(ui, |ui| {
    ui.style_mut().spacing.item_spacing = vec2(5.0,5.0);
    ui.vertical(|ui| {
      


      ui.vertical(|ui| { //separating this out for styling reasons
        ui.visuals_mut().extreme_bg_color = Color32::TRANSPARENT;

        ui.visuals_mut().widgets.inactive.expansion = 0.0;
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(2.0, Color32::WHITE);
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(2.0, DARK_GREY);
        ui.visuals_mut().widgets.inactive.rounding = Rounding::same(2.0);

        ui.visuals_mut().widgets.active.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.active.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
        ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(2.0, DARK_GREY);
        ui.visuals_mut().widgets.active.rounding = Rounding::same(2.0);

        ui.visuals_mut().widgets.hovered.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.hovered.fg_stroke = Stroke::new(2.0, F9B233);
        ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(2.0, F9B233);
        ui.visuals_mut().widgets.hovered.rounding = Rounding::same(2.0);

        ui.visuals_mut().widgets.open.bg_fill = DARK_GREY;
        ui.visuals_mut().widgets.open.weak_bg_fill = DARK_GREY;
        ui.visuals_mut().widgets.open.fg_stroke = Stroke::new(2.0, Color32::WHITE);
        ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(2.0, DARK_GREY);
        ui.visuals_mut().widgets.open.rounding = Rounding::same(2.0);

        ui.add_sized([ui.available_width(), 20.0], egui::TextEdit::hint_text(egui::text_edit::TextEdit::singleline(&mut app.friends_view_bar.search_buffer), "Search friends list"));
        let combo_width = (ui.available_width() / 2.0) - ui.spacing().item_spacing.x; //a lot of accounting for shit when i'm just gonna make it a fixed width anyway
        ui.horizontal(|ui| {
            egui::ComboBox::new("FriendsListStatusFilterComboBox", "")
            .selected_text( match app.friends_view_bar.page {
              FriendsViewBarPage::Online => &app.locale.localization.friends_view.toolbar.online,
              FriendsViewBarPage::All => &app.locale.localization.friends_view.toolbar.all,
              FriendsViewBarPage::Pending => &app.locale.localization.friends_view.toolbar.pending,
              FriendsViewBarPage::Blocked => &app.locale.localization.friends_view.toolbar.blocked,
            })
            .width(combo_width)
            .show_ui(ui, |combo| {
              combo.visuals_mut().selection.bg_fill = F9B233;
              combo.visuals_mut().selection.stroke = Stroke::new(2.0, Color32::BLACK);
              combo.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(2.0, F9B233);
              let mut rounding = Rounding::same(2.0);
              rounding.se = 0.0; rounding.sw = 0.0;
              combo.visuals_mut().widgets.inactive.rounding = rounding;
              combo.visuals_mut().widgets.active.rounding = rounding;
              combo.visuals_mut().widgets.hovered.rounding = rounding;
              combo.selectable_value(&mut app.friends_view_bar.page, FriendsViewBarPage::Online, &app.locale.localization.friends_view.toolbar.online);
              rounding.ne = 0.0; rounding.nw = 0.0;
              combo.visuals_mut().widgets.inactive.rounding = rounding;
              combo.visuals_mut().widgets.active.rounding = rounding;
              combo.visuals_mut().widgets.hovered.rounding = rounding;
              combo.selectable_value(&mut app.friends_view_bar.page, FriendsViewBarPage::All, &app.locale.localization.friends_view.toolbar.all);
              combo.selectable_value(&mut app.friends_view_bar.page, FriendsViewBarPage::Pending, &app.locale.localization.friends_view.toolbar.pending);
              rounding.se = 2.0; rounding.sw = 2.0;
              combo.visuals_mut().widgets.inactive.rounding = rounding;
              combo.visuals_mut().widgets.active.rounding = rounding;
              combo.visuals_mut().widgets.hovered.rounding = rounding;
              combo.selectable_value(&mut app.friends_view_bar.page, FriendsViewBarPage::Blocked, &app.locale.localization.friends_view.toolbar.blocked);
            });
            egui::ComboBox::new("FriendsListFilterTypeComboBox","")
            .selected_text(match app.friends_view_bar.status_filter {
              FriendsViewBarStatusFilter::Name => &app.locale.localization.friends_view.toolbar.filter_options.name,
              FriendsViewBarStatusFilter::Game => &app.locale.localization.friends_view.toolbar.filter_options.game
            })
            .width(combo_width)
            .show_ui(ui, |combo| {
              combo.visuals_mut().selection.bg_fill = F9B233;
              combo.visuals_mut().selection.stroke = Stroke::new(2.0, Color32::BLACK);
              combo.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(2.0, F9B233);
              let mut rounding = Rounding::same(2.0);
              rounding.se = 0.0; rounding.sw = 0.0;
              combo.visuals_mut().widgets.inactive.rounding = rounding;
              combo.visuals_mut().widgets.active.rounding = rounding;
              combo.visuals_mut().widgets.hovered.rounding = rounding;
              combo.selectable_value(&mut app.friends_view_bar.status_filter, FriendsViewBarStatusFilter::Name, &app.locale.localization.friends_view.toolbar.filter_options.name);
              rounding.se = 2.0; rounding.sw = 2.0;
              combo.visuals_mut().widgets.inactive.rounding = rounding;
              combo.visuals_mut().widgets.active.rounding = rounding;
              combo.visuals_mut().widgets.hovered.rounding = rounding;
              combo.selectable_value(&mut app.friends_view_bar.status_filter, FriendsViewBarStatusFilter::Game, &app.locale.localization.friends_view.toolbar.filter_options.game);
              }
            );
        });
      });
      let friends : Vec<Friend> = friends_raw.into_iter().filter(|obj| 
        match app.friends_view_bar.status_filter {
            FriendsViewBarStatusFilter::Name => obj.name.to_ascii_lowercase().contains(&app.friends_view_bar.search_buffer),
            FriendsViewBarStatusFilter::Game => if let Some(game) = &obj.game {
              game.to_ascii_lowercase().contains(&app.friends_view_bar.search_buffer)
            } else {
              false
            }
        }
        &&
        match app.friends_view_bar.page {
            FriendsViewBarPage::Online => obj.online,
            FriendsViewBarPage::All => true,
            FriendsViewBarPage::Pending => false,
            FriendsViewBarPage::Blocked => false,
        }
      ).collect();
      ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::WHITE;

      egui::ScrollArea::new([false,true])
      .id_source("FriendsListFriendListScrollArea") //hmm yes, the friends list is made of friends list
      .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
      .show(ui, |ui| {
        
        let item_width = ui.available_rect_before_wrap().width() - (ui.spacing().scroll_bar_width);
        for friend in friends {
          ui.horizontal(|container| {
            container.spacing_mut().item_spacing.x = 0.0;
            egui::Frame::default()
            //.stroke(Stroke { width: 2.0, color: Color32::WHITE })
            .inner_margin(Margin::same(1.0))
            .outer_margin(Margin::same(1.0))
            .show(container, |container| {
              container.spacing_mut().item_spacing.y = 2.0;
              let click_sensor = container.allocate_exact_size(vec2(item_width-2.0,40.0), Sense::click());
              if click_sensor.1.hovered() {
                container.painter().rect_filled(click_sensor.0, Rounding::none(), Color32::WHITE);
                container.painter().rect_stroke(click_sensor.0, Rounding::same(2.0), Stroke::new(4.0, Color32::WHITE));
                container.visuals_mut().override_text_color = Some(Color32::BLACK);
              } else {
                container.visuals_mut().override_text_color = Some(Color32::WHITE);
              }
              container.allocate_space(vec2((-item_width) + 2.0,-40.0));
              container.image(app.user_pfp_renderable, [40.0,40.0]);
              container.allocate_space(vec2(5.0,0.0));
              container.vertical(|text| {
                text.label(egui::RichText::new(&friend.name).size(15.0));
                let game_hack: String;
                text.label(egui::RichText::new(
                  if friend.online {
                    if let Some(game) = friend.game  {
                      if app.locale.localization.friends_view.prepend {
                        if let Some(presence) = friend.game_presence {
                          game_hack = format!("{} {}: {}", &game, &app.locale.localization.friends_view.status_playing, &presence);
                        } else {
                          game_hack = format!("{} {}", &game, &app.locale.localization.friends_view.status_playing);
                        }
                      } else {
                        if let Some(presence) = friend.game_presence {
                          game_hack = format!("{} {}: {}", &app.locale.localization.friends_view.status_playing, &game, &presence);
                        } else {
                          game_hack = format!("{} {}", &app.locale.localization.friends_view.status_playing, &game);
                        }
                      }
                      &game_hack
                      
                    } else {
                      &app.locale.localization.friends_view.status_online
                    }
                  } else {
                    &app.locale.localization.friends_view.status_offline
                  }
                ).size(10.0));
              });
              let mut outline_rect_fucking_jank_ass_bitch_dont_ship_it_idiot_lmao = container.min_rect();
              outline_rect_fucking_jank_ass_bitch_dont_ship_it_idiot_lmao.max = outline_rect_fucking_jank_ass_bitch_dont_ship_it_idiot_lmao.min + vec2(41.0, 41.0);
              outline_rect_fucking_jank_ass_bitch_dont_ship_it_idiot_lmao.min -= vec2(1.0, 1.0);
              container.painter().rect_stroke(outline_rect_fucking_jank_ass_bitch_dont_ship_it_idiot_lmao, Rounding::same(2.0), Stroke::new(2.0, if friend.online { Color32::GREEN } else { Color32::GRAY }));
            });
          });
        }
        ui.allocate_space(vec2(0.0,ui.available_size_before_wrap().y));
      })
    });
  });
  ui.allocate_space(vec2(0.0,8.0));

  
}