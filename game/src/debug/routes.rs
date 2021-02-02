use map_gui::ID;
use map_model::NORMAL_LANE_THICKNESS;
use sim::{TripEndpoint, TripMode};
use widgetry::{
    Color, Drawable, EventCtx, GeomBatch, GfxCtx, HorizontalAlignment, Line, Outcome, Panel,
    Spinner, State, StyledButtons, TextExt, VerticalAlignment, Widget,
};

use crate::app::{App, Transition};
use crate::common::CommonState;

pub struct RouteExplorer {
    panel: Panel,
    start: TripEndpoint,
    // (endpoint, confirmed, render the paths to it)
    goal: Option<(TripEndpoint, bool, Drawable)>,
}

impl RouteExplorer {
    pub fn new(ctx: &mut EventCtx, start: TripEndpoint) -> Box<dyn State<App>> {
        Box::new(RouteExplorer {
            start,
            goal: None,
            panel: Panel::new(Widget::col(vec![
                Widget::row(vec![
                    Line("Route explorer").small_heading().draw(ctx),
                    ctx.style().btn_close_widget(ctx),
                ]),
                profile_to_controls(ctx, &RoutingProfile::default_biking()).named("profile"),
            ]))
            .aligned(HorizontalAlignment::Right, VerticalAlignment::Top)
            .build(ctx),
        })
    }

    fn controls_to_profile(&self) -> RoutingProfile {
        if !self.panel.is_button_enabled("cars") {
            return RoutingProfile::Driving;
        }
        if !self.panel.is_button_enabled("pedestrians") {
            return RoutingProfile::Walking;
        }
        RoutingProfile::Biking {
            bike_lane_penalty: self.panel.spinner("bike lane penalty") as f64 / 10.0,
            bus_lane_penalty: self.panel.spinner("bus lane penalty") as f64 / 10.0,
            driving_lane_penalty: self.panel.spinner("driving lane penalty") as f64 / 10.0,
        }
    }

    fn recalc_paths(&mut self, ctx: &mut EventCtx, app: &App) {
        let mode = match self.controls_to_profile() {
            RoutingProfile::Driving => TripMode::Drive,
            RoutingProfile::Walking => TripMode::Walk,
            RoutingProfile::Biking { .. } => TripMode::Bike,
        };

        if let Some((ref goal, _, ref mut preview)) = self.goal {
            *preview = Drawable::empty(ctx);
            if let Some(polygon) =
                TripEndpoint::path_req(self.start.clone(), goal.clone(), mode, &app.primary.map)
                    .and_then(|req| app.primary.map.pathfind(req).ok())
                    .and_then(|path| path.trace(&app.primary.map))
                    .map(|pl| pl.make_polygons(NORMAL_LANE_THICKNESS))
            {
                *preview = GeomBatch::from(vec![(Color::PURPLE, polygon)]).upload(ctx);
            }
        }
    }
}

impl State<App> for RouteExplorer {
    fn event(&mut self, ctx: &mut EventCtx, app: &mut App) -> Transition {
        ctx.canvas_movement();

        match self.panel.event(ctx) {
            Outcome::Clicked(x) => match x.as_ref() {
                "close" => {
                    return Transition::Pop;
                }
                "bikes" => {
                    let controls = profile_to_controls(ctx, &RoutingProfile::default_biking());
                    self.panel.replace(ctx, "profile", controls);
                    self.recalc_paths(ctx, app);
                }
                "cars" => {
                    let controls = profile_to_controls(ctx, &RoutingProfile::Driving);
                    self.panel.replace(ctx, "profile", controls);
                    self.recalc_paths(ctx, app);
                }
                "pedestrians" => {
                    let controls = profile_to_controls(ctx, &RoutingProfile::Walking);
                    self.panel.replace(ctx, "profile", controls);
                    self.recalc_paths(ctx, app);
                }
                _ => unreachable!(),
            },
            Outcome::Changed => {
                self.recalc_paths(ctx, app);
            }
            _ => {}
        }

        if self
            .goal
            .as_ref()
            .map(|(_, confirmed, _)| *confirmed)
            .unwrap_or(false)
        {
            return Transition::Keep;
        }

        if ctx.redo_mouseover() {
            app.primary.current_selection = app.mouseover_unzoomed_everything(ctx);
            if match app.primary.current_selection {
                Some(ID::Intersection(i)) => !app.primary.map.get_i(i).is_border(),
                Some(ID::Building(_)) => false,
                _ => true,
            } {
                app.primary.current_selection = None;
            }
        }
        if let Some(hovering) = match app.primary.current_selection {
            Some(ID::Intersection(i)) => Some(TripEndpoint::Border(i)),
            Some(ID::Building(b)) => Some(TripEndpoint::Bldg(b)),
            None => None,
            _ => unreachable!(),
        } {
            if self.start != hovering {
                if self
                    .goal
                    .as_ref()
                    .map(|(to, _, _)| to != &hovering)
                    .unwrap_or(true)
                {
                    self.goal = Some((hovering, false, Drawable::empty(ctx)));
                    self.recalc_paths(ctx, app);
                }
            } else {
                self.goal = None;
            }
        } else {
            self.goal = None;
        }

        if let Some((_, ref mut confirmed, _)) = self.goal {
            if app.per_obj.left_click(ctx, "end here") {
                app.primary.current_selection = None;
                *confirmed = true;
            }
        }

        Transition::Keep
    }

    fn draw(&self, g: &mut GfxCtx, app: &App) {
        self.panel.draw(g);
        CommonState::draw_osd(g, app);

        g.draw_polygon(
            Color::BLUE.alpha(0.8),
            match self.start {
                TripEndpoint::Border(i) => app.primary.map.get_i(i).polygon.clone(),
                TripEndpoint::Bldg(b) => app.primary.map.get_b(b).polygon.clone(),
                TripEndpoint::SuddenlyAppear(_) => unreachable!(),
            },
        );
        if let Some((ref endpt, _, ref draw)) = self.goal {
            g.draw_polygon(
                Color::GREEN.alpha(0.8),
                match endpt {
                    TripEndpoint::Border(i) => app.primary.map.get_i(*i).polygon.clone(),
                    TripEndpoint::Bldg(b) => app.primary.map.get_b(*b).polygon.clone(),
                    TripEndpoint::SuddenlyAppear(_) => unreachable!(),
                },
            );
            g.redraw(draw);
        }
    }
}

// TODO Move to map_model
// TODO Not sure an enum makes sense, based on how we're still going to be toggling based on
// PathConstraints.
enum RoutingProfile {
    Driving,
    Biking {
        bike_lane_penalty: f64,
        bus_lane_penalty: f64,
        driving_lane_penalty: f64,
    },
    Walking,
}

impl RoutingProfile {
    fn default_biking() -> RoutingProfile {
        RoutingProfile::Biking {
            bike_lane_penalty: 1.0,
            bus_lane_penalty: 1.1,
            driving_lane_penalty: 1.5,
        }
    }
}

fn profile_to_controls(ctx: &mut EventCtx, profile: &RoutingProfile) -> Widget {
    let mut rows = vec![Widget::custom_row(vec![
        ctx.style()
            .btn_plain_light_icon("system/assets/meters/bike.svg")
            .disabled(matches!(profile, RoutingProfile::Biking { .. }))
            .build_widget(ctx, "bikes"),
        ctx.style()
            .btn_plain_light_icon("system/assets/meters/car.svg")
            .disabled(matches!(profile, RoutingProfile::Driving))
            .build_widget(ctx, "cars"),
        ctx.style()
            .btn_plain_light_icon("system/assets/meters/pedestrian.svg")
            .disabled(matches!(profile, RoutingProfile::Walking))
            .build_widget(ctx, "pedestrians"),
    ])
    .evenly_spaced()];
    if let RoutingProfile::Biking {
        bike_lane_penalty,
        bus_lane_penalty,
        driving_lane_penalty,
    } = profile
    {
        // TODO Spinners that natively understand a floating point range with a given precision
        rows.push(Widget::row(vec![
            "Bike lane penalty:".draw_text(ctx).margin_right(20),
            Spinner::new(ctx, (0, 20), (*bike_lane_penalty * 10.0) as isize)
                .named("bike lane penalty"),
        ]));
        rows.push(Widget::row(vec![
            "Bus lane penalty:".draw_text(ctx).margin_right(20),
            Spinner::new(ctx, (0, 20), (*bus_lane_penalty * 10.0) as isize)
                .named("bus lane penalty"),
        ]));
        rows.push(Widget::row(vec![
            "Driving lane penalty:".draw_text(ctx).margin_right(20),
            Spinner::new(ctx, (0, 20), (*driving_lane_penalty * 10.0) as isize)
                .named("driving lane penalty"),
        ]));
    }
    Widget::col(rows)
}