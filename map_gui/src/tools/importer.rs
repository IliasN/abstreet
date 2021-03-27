use std::io::Write;

use anyhow::Result;
use clipboard::{ClipboardContext, ClipboardProvider};

use abstio::MapName;
use widgetry::{
    EventCtx, GfxCtx, Line, Outcome, Panel, State, TextExt, Toggle, Transition, Widget,
};

use crate::load::MapLoader;
use crate::tools::{open_browser, PopupMsg};
use crate::AppLike;

pub struct ImportCity<A: AppLike> {
    panel: Panel,
    // Wrapped in an Option just to make calling from event() work.
    on_load: Option<Box<dyn FnOnce(&mut EventCtx, &mut A) -> Transition<A>>>,
}

impl<A: AppLike + 'static> ImportCity<A> {
    pub fn new(
        ctx: &mut EventCtx,
        on_load: Box<dyn FnOnce(&mut EventCtx, &mut A) -> Transition<A>>,
    ) -> Box<dyn State<A>> {
        let panel = Panel::new(Widget::col(vec![
            Widget::row(vec![
                Line("Import a new city").small_heading().into_widget(ctx),
                ctx.style().btn_close_widget(ctx),
            ]),
            Widget::col(vec![
                Widget::row(vec![
                    "Step 1)".text_widget(ctx).centered_vert(),
                    ctx.style()
                        .btn_plain
                        .btn()
                        .label_underlined_text("Go to geojson.io")
                        .build_def(ctx),
                ]),
                Widget::row(vec![
                    "Step 2)".text_widget(ctx).margin_right(16),
                    "Draw a polygon boundary where you want to import"
                        .text_widget(ctx)
                        .margin_below(16),
                ])
                .margin_below(16),
                Widget::row(vec![
                    "Step 3)".text_widget(ctx).margin_right(16),
                    "Copy the JSON text on the right into your clipboard".text_widget(ctx),
                ])
                .margin_below(16),
                Widget::row(vec![
                    "Step 4)".text_widget(ctx).centered_vert(),
                    Toggle::choice(
                        ctx,
                        "left handed driving",
                        "drive on the left",
                        "right",
                        None,
                        false,
                    ),
                ]),
                Widget::row(vec![
                    "Step 5)".text_widget(ctx).centered_vert(),
                    ctx.style()
                        .btn_solid_primary
                        .text("Import the area from your clipboard")
                        .build_def(ctx),
                ])
                .margin_below(32),
                ctx.style()
                    .btn_plain
                    .btn()
                    .label_underlined_text("Alternate instructions")
                    .build_def(ctx),
            ])
            .section(ctx),
        ]))
        .build(ctx);
        Box::new(ImportCity {
            panel,
            on_load: Some(on_load),
        })
    }
}

impl<A: AppLike + 'static> State<A> for ImportCity<A> {
    fn event(&mut self, ctx: &mut EventCtx, app: &mut A) -> Transition<A> {
        match self.panel.event(ctx) {
            Outcome::Clicked(x) => match x.as_ref() {
                "close" => Transition::Pop,
                "Alternate instructions" => {
                    open_browser("https://a-b-street.github.io/docs/howto/new_city.html");
                    Transition::Keep
                }
                "Go to geojson.io" => {
                    open_browser("http://geojson.io");
                    Transition::Keep
                }
                "Import the area from your clipboard" => {
                    let bin_dir = vec![
                        "./target/release",
                        "../target/release",
                        "../../target/release",
                        "./tools",
                        "../tools",
                    ]
                    .into_iter()
                    .find(|x| std::path::Path::new(x).exists())
                    .unwrap_or("./target/release");
                    let mut args = vec![
                        format!("{}/one_step_import", bin_dir),
                        "boundary.geojson".to_string(),
                    ];
                    if self.panel.is_checked("left handed driving") {
                        args.push("--drive_on_left".to_string());
                    }
                    match grab_geojson_from_clipboard() {
                        Ok(()) => Transition::Push(crate::tools::command::RunCommand::new(
                            ctx,
                            app,
                            args,
                            Box::new(|_, _, success, mut lines| {
                                if success {
                                    abstio::delete_file("boundary.geojson");

                                    Transition::ReplaceWithData(Box::new(move |state, ctx, app| {
                                        let mut import =
                                            state.downcast::<ImportCity<A>>().ok().unwrap();
                                        let on_load = import.on_load.take().unwrap();
                                        // one_step_import prints the name of the map as the last
                                        // line.
                                        let name =
                                            MapName::new("zz", "oneshot", &lines.pop().unwrap());
                                        vec![MapLoader::new(ctx, app, name, on_load)]
                                    }))
                                } else {
                                    // The popup already explained the failure
                                    Transition::Keep
                                }
                            }),
                        )),
                        Err(err) => Transition::Push(PopupMsg::new(
                            ctx,
                            "Error",
                            vec![
                                "Couldn't get GeoJSON from your clipboard".to_string(),
                                err.to_string(),
                            ],
                        )),
                    }
                }
                _ => unreachable!(),
            },
            _ => Transition::Keep,
        }
    }

    fn draw(&self, g: &mut GfxCtx, _: &A) {
        self.panel.draw(g);
    }
}

fn grab_geojson_from_clipboard() -> Result<()> {
    // TODO The clipboard crate uses old nightly Errors. Converting to anyhow is weird.
    let mut ctx: ClipboardContext = match ClipboardProvider::new() {
        Ok(ctx) => ctx,
        Err(err) => bail!("{}", err),
    };
    let contents = match ctx.get_contents() {
        Ok(contents) => contents,
        Err(err) => bail!("{}", err),
    };
    if contents.parse::<geojson::GeoJson>().is_err() {
        bail!(
            "Your clipboard doesn't seem to have GeoJSON. Got: {}",
            contents
        );
    }
    let mut f = std::fs::File::create("boundary.geojson")?;
    write!(f, "{}", contents)?;
    Ok(())
}