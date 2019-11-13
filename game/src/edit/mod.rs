mod impact;
mod stop_signs;
mod traffic_signals;

use crate::common::CommonState;
use crate::debug::DebugMode;
use crate::game::{msg, State, Transition, WizardState};
use crate::helpers::{ColorScheme, ID};
use crate::render::{
    DrawIntersection, DrawLane, DrawMap, DrawOptions, DrawRoad, DrawTurn, Renderable,
    MIN_ZOOM_FOR_DETAIL,
};
use crate::sandbox::{GameplayMode, SandboxMode};
use crate::ui::{PerMapUI, ShowEverything, UI};
use abstutil::Timer;
use ezgui::{
    hotkey, lctrl, Button, Choice, Color, EventCtx, EventLoopMode, GfxCtx, Key, Line,
    MenuUnderButton, ModalMenu, ScreenPt, Text, Wizard,
};
use map_model::{IntersectionID, LaneID, LaneType, Map, MapEdits, RoadID, TurnID, TurnType};
use std::collections::{BTreeSet, HashMap};

pub struct EditMode {
    common: CommonState,
    menu: ModalMenu,
    general_tools: MenuUnderButton,
    mode: GameplayMode,

    lane_editor: LaneEditor,
}

impl EditMode {
    pub fn new(ctx: &EventCtx, mode: GameplayMode) -> EditMode {
        EditMode {
            common: CommonState::new(ctx),
            menu: ModalMenu::new(
                "Map Edit Mode",
                vec![
                    (hotkey(Key::Escape), "back to sandbox mode"),
                    (hotkey(Key::S), "save edits"),
                    (hotkey(Key::L), "load different edits"),
                    (None, "measure impact of edits"),
                ],
                ctx,
            ),
            general_tools: MenuUnderButton::new(
                "assets/ui/hamburger.png",
                "General",
                vec![
                    (lctrl(Key::D), "debug mode"),
                    (hotkey(Key::F1), "take a screenshot"),
                ],
                0.2,
                ctx,
            ),
            mode,
            lane_editor: LaneEditor::setup(ctx),
        }
    }
}

impl State for EditMode {
    fn event(&mut self, ctx: &mut EventCtx, ui: &mut UI) -> Transition {
        // The .clone() is probably not that expensive, and it makes later code a bit
        // easier to read. :)
        let orig_edits = ui.primary.map.get_edits().clone();
        {
            let mut txt = Text::new();
            txt.add(Line(format!("Edits: {}", orig_edits.edits_name)));
            if orig_edits.dirty {
                txt.append(Line("*"));
            }
            txt.add(Line(format!("{} lanes", orig_edits.lane_overrides.len())));
            txt.add(Line(format!(
                "{} stop signs ",
                orig_edits.stop_sign_overrides.len()
            )));
            txt.add(Line(format!(
                "{} traffic signals",
                orig_edits.traffic_signal_overrides.len()
            )));
            self.menu.set_info(ctx, txt);
        }
        self.menu.event(ctx);
        self.general_tools.event(ctx);

        if let Some(t) = self.lane_editor.event(ui, ctx) {
            return t;
        }
        ctx.canvas.handle_event(ctx.input);
        // It only makes sense to mouseover lanes while painting them.
        if ctx.redo_mouseover() {
            ui.recalculate_current_selection(ctx);
            if let Some(ID::Lane(_)) = ui.primary.current_selection {
            } else {
                if self.lane_editor.active_idx.is_some() {
                    ui.primary.current_selection = None;
                }
            }
        }

        if let Some(t) = self.common.event(ctx, ui) {
            return t;
        }

        if self.general_tools.action("debug mode") {
            return Transition::Push(Box::new(DebugMode::new(ctx, ui)));
        }
        if self.general_tools.action("take a screenshot") {
            return Transition::KeepWithMode(EventLoopMode::ScreenCaptureCurrentShot);
        }

        if orig_edits.dirty && self.menu.action("save edits") {
            return Transition::Push(WizardState::new(Box::new(save_edits)));
        } else if self.menu.action("load different edits") {
            return Transition::Push(WizardState::new(Box::new(load_edits)));
        } else if self.menu.action("back to sandbox mode") {
            // TODO Warn about unsaved edits
            // TODO Maybe put a loading screen around these.
            ui.primary
                .map
                .recalculate_pathfinding_after_edits(&mut Timer::new("apply pending map edits"));
            // Parking state might've changed
            ui.primary.clear_sim();
            return Transition::Replace(Box::new(SandboxMode::new(ctx, ui, self.mode.clone())));
        } else if self.menu.action("measure impact of edits") {
            let mut timer = Timer::new("measure impact of edits");
            ui.primary
                .map
                .recalculate_pathfinding_after_edits(&mut timer);
            return Transition::Push(msg(
                "Impact of edits",
                impact::edit_impacts(self.mode.scenario(ui, &mut timer), ui, &mut timer),
            ));
        }

        if let Some(ID::Lane(id)) = ui.primary.current_selection {
            if ctx
                .input
                .contextual_action(Key::U, "bulk edit lanes on this road")
            {
                return Transition::Push(make_bulk_edit_lanes(ui.primary.map.get_l(id).parent));
            } else if orig_edits.lane_overrides.contains_key(&id)
                && ctx.input.contextual_action(Key::R, "revert")
            {
                let mut new_edits = orig_edits.clone();
                new_edits.lane_overrides.remove(&id);
                new_edits.contraflow_lanes.remove(&id);
                apply_map_edits(&mut ui.primary, &ui.cs, ctx, new_edits);
            }
        }
        if let Some(ID::Intersection(id)) = ui.primary.current_selection {
            if ui.primary.map.maybe_get_stop_sign(id).is_some() {
                if ctx
                    .input
                    .contextual_action(Key::E, format!("edit stop signs for {}", id))
                {
                    return Transition::Push(Box::new(stop_signs::StopSignEditor::new(
                        id, ctx, ui,
                    )));
                } else if orig_edits.stop_sign_overrides.contains_key(&id)
                    && ctx.input.contextual_action(Key::R, "revert")
                {
                    let mut new_edits = orig_edits.clone();
                    new_edits.stop_sign_overrides.remove(&id);
                    apply_map_edits(&mut ui.primary, &ui.cs, ctx, new_edits);
                }
            }
            if ui.primary.map.maybe_get_traffic_signal(id).is_some() {
                if ctx
                    .input
                    .contextual_action(Key::E, format!("edit traffic signal for {}", id))
                {
                    return Transition::Push(Box::new(traffic_signals::TrafficSignalEditor::new(
                        id, ctx, ui,
                    )));
                } else if orig_edits.traffic_signal_overrides.contains_key(&id)
                    && ctx.input.contextual_action(Key::R, "revert")
                {
                    let mut new_edits = orig_edits.clone();
                    new_edits.traffic_signal_overrides.remove(&id);
                    apply_map_edits(&mut ui.primary, &ui.cs, ctx, new_edits);
                }
            }
            if !ui.primary.map.get_i(id).is_closed()
                && ctx
                    .input
                    .contextual_action(Key::C, "close for construction")
            {
                let mut new_edits = orig_edits.clone();
                new_edits.stop_sign_overrides.remove(&id);
                new_edits.traffic_signal_overrides.remove(&id);
                new_edits.intersections_under_construction.insert(id);
                apply_map_edits(&mut ui.primary, &ui.cs, ctx, new_edits);
            }
        }

        Transition::Keep
    }

    fn draw_default_ui(&self) -> bool {
        false
    }

    fn draw(&self, g: &mut GfxCtx, ui: &UI) {
        ui.draw(
            g,
            self.common.draw_options(ui),
            &ui.primary.sim,
            &ShowEverything::new(),
        );

        // More generally we might want to show the diff between two edits, but for now,
        // just show diff relative to basemap.
        let edits = ui.primary.map.get_edits();

        let ctx = ui.draw_ctx();
        let mut opts = DrawOptions::new();

        // TODO Similar to drawing areas with traffic or not -- would be convenient to just
        // supply a set of things to highlight and have something else take care of drawing
        // with detail or not.
        if g.canvas.cam_zoom >= MIN_ZOOM_FOR_DETAIL {
            for l in edits
                .lane_overrides
                .keys()
                .chain(edits.contraflow_lanes.keys())
            {
                opts.override_colors
                    .insert(ID::Lane(*l), Color::HatchingStyle1);
                ctx.draw_map.get_l(*l).draw(g, &opts, &ctx);
            }
            for i in edits
                .stop_sign_overrides
                .keys()
                .chain(edits.traffic_signal_overrides.keys())
            {
                opts.override_colors
                    .insert(ID::Intersection(*i), Color::HatchingStyle1);
                ctx.draw_map.get_i(*i).draw(g, &opts, &ctx);
            }

            // The hatching covers up the selection outline, so redraw it.
            match ui.primary.current_selection {
                Some(ID::Lane(l)) => {
                    g.draw_polygon(
                        ui.cs.get("selected"),
                        &ctx.draw_map.get_l(l).get_outline(&ctx.map),
                    );
                }
                Some(ID::Intersection(i)) => {
                    g.draw_polygon(
                        ui.cs.get("selected"),
                        &ctx.draw_map.get_i(i).get_outline(&ctx.map),
                    );
                }
                _ => {}
            }
        } else {
            let color = ui.cs.get_def("unzoomed map diffs", Color::RED);
            for l in edits.lane_overrides.keys() {
                g.draw_polygon(color, &ctx.map.get_parent(*l).get_thick_polygon().unwrap());
            }

            for i in edits
                .stop_sign_overrides
                .keys()
                .chain(edits.traffic_signal_overrides.keys())
            {
                opts.override_colors.insert(ID::Intersection(*i), color);
                ctx.draw_map.get_i(*i).draw(g, &opts, &ctx);
            }
        }

        self.common.draw(g, ui);
        self.menu.draw(g);
        self.general_tools.draw(g);
        self.lane_editor.draw(g);
    }
}

fn save_edits(wiz: &mut Wizard, ctx: &mut EventCtx, ui: &mut UI) -> Option<Transition> {
    let map = &mut ui.primary.map;
    let mut wizard = wiz.wrap(ctx);

    let rename = if map.get_edits().edits_name == "no_edits" {
        Some(wizard.input_string("Name these map edits")?)
    } else {
        None
    };
    // TODO Don't allow naming them no_edits!

    // TODO Do it this weird way to avoid saving edits on every event. :P
    let save = "save edits";
    let cancel = "cancel";
    if wizard
        .choose_string("Overwrite edits?", || vec![save, cancel])?
        .as_str()
        == save
    {
        if let Some(name) = rename {
            let mut edits = map.get_edits().clone();
            edits.edits_name = name;
            map.apply_edits(edits, &mut Timer::new("name map edits"));
        }
        map.save_edits();
    }
    Some(Transition::Pop)
}

fn load_edits(wiz: &mut Wizard, ctx: &mut EventCtx, ui: &mut UI) -> Option<Transition> {
    let map = &mut ui.primary.map;
    let mut wizard = wiz.wrap(ctx);

    // TODO Exclude current
    let map_name = map.get_name().to_string();
    let (_, new_edits) = wizard.choose("Load which map edits?", || {
        let mut list = Choice::from(abstutil::load_all_objects("edits", &map_name));
        list.push(Choice::new("no_edits", MapEdits::new(map_name.clone())));
        list
    })?;
    apply_map_edits(&mut ui.primary, &ui.cs, ctx, new_edits);
    ui.primary.map.mark_edits_fresh();
    Some(Transition::Pop)
}

fn can_change_lane_type(l: LaneID, new_lt: LaneType, map: &Map) -> Option<String> {
    let r = map.get_parent(l);
    let (fwds, idx) = r.dir_and_offset(l);
    let mut proposed_lts = if fwds {
        r.get_lane_types().0
    } else {
        r.get_lane_types().1
    };
    proposed_lts[idx] = new_lt;

    // No-op change
    if map.get_l(l).lane_type == new_lt {
        return None;
    }

    // Only one parking lane per side.
    if proposed_lts
        .iter()
        .filter(|lt| **lt == LaneType::Parking)
        .count()
        > 1
    {
        // TODO Actually, we just don't want two adjacent parking lanes
        // (What about dppd though?)
        return Some(format!(
            "You can only have one parking lane on the same side of the road"
        ));
    }

    let types: BTreeSet<LaneType> = r
        .all_lanes()
        .iter()
        .map(|l| map.get_l(*l).lane_type)
        .collect();

    // Don't let players orphan a bus stop.
    if !r.all_bus_stops(map).is_empty()
        && !types.contains(&LaneType::Driving)
        && !types.contains(&LaneType::Bus)
    {
        return Some(format!("You need a driving or bus lane for the bus stop!"));
    }

    // A parking lane must have a driving lane somewhere on the road.
    if types.contains(&LaneType::Parking) && !types.contains(&LaneType::Driving) {
        return Some(format!(
            "A parking lane needs a driving lane somewhere on the same road"
        ));
    }

    None
}

pub fn apply_map_edits(
    bundle: &mut PerMapUI,
    cs: &ColorScheme,
    ctx: &mut EventCtx,
    mut edits: MapEdits,
) {
    edits.dirty = true;
    let mut timer = Timer::new("apply map edits");

    let (lanes_changed, roads_changed, turns_deleted, turns_added) =
        bundle.map.apply_edits(edits, &mut timer);

    for l in lanes_changed {
        bundle.draw_map.lanes[l.0] = DrawLane::new(
            bundle.map.get_l(l),
            &bundle.map,
            bundle.current_flags.draw_lane_markings,
            cs,
            &mut timer,
        )
        .finish(ctx.prerender);
    }
    for r in roads_changed {
        bundle.draw_map.roads[r.0] =
            DrawRoad::new(bundle.map.get_r(r), &bundle.map, cs, ctx.prerender);
    }

    let mut modified_intersections: BTreeSet<IntersectionID> = BTreeSet::new();
    let mut lanes_of_modified_turns: BTreeSet<LaneID> = BTreeSet::new();
    for t in turns_deleted {
        bundle.draw_map.turns.remove(&t);
        lanes_of_modified_turns.insert(t.src);
        modified_intersections.insert(t.parent);
    }
    for t in &turns_added {
        lanes_of_modified_turns.insert(t.src);
        modified_intersections.insert(t.parent);
    }

    let mut turn_to_lane_offset: HashMap<TurnID, usize> = HashMap::new();
    for l in lanes_of_modified_turns {
        DrawMap::compute_turn_to_lane_offset(
            &mut turn_to_lane_offset,
            bundle.map.get_l(l),
            &bundle.map,
        );
    }
    for t in turns_added {
        let turn = bundle.map.get_t(t);
        if turn.turn_type != TurnType::SharedSidewalkCorner {
            bundle
                .draw_map
                .turns
                .insert(t, DrawTurn::new(&bundle.map, turn, turn_to_lane_offset[&t]));
        }
    }

    for i in modified_intersections {
        bundle.draw_map.intersections[i.0] = DrawIntersection::new(
            bundle.map.get_i(i),
            &bundle.map,
            cs,
            ctx.prerender,
            &mut timer,
        );
    }

    // Do this after fixing up all the state above.
    bundle.map.simplify_edits(&mut timer);
}

fn make_bulk_edit_lanes(road: RoadID) -> Box<dyn State> {
    WizardState::new(Box::new(move |wiz, ctx, ui| {
        let mut wizard = wiz.wrap(ctx);
        let (_, from) = wizard.choose("Change all lanes of type...", || {
            vec![
                Choice::new("driving", LaneType::Driving),
                Choice::new("parking", LaneType::Parking),
                Choice::new("biking", LaneType::Biking),
                Choice::new("bus", LaneType::Bus),
                Choice::new("construction", LaneType::Construction),
            ]
        })?;
        let (_, to) = wizard.choose("Change to all lanes of type...", || {
            vec![
                Choice::new("driving", LaneType::Driving),
                Choice::new("parking", LaneType::Parking),
                Choice::new("biking", LaneType::Biking),
                Choice::new("bus", LaneType::Bus),
                Choice::new("construction", LaneType::Construction),
            ]
            .into_iter()
            .filter(|c| c.data != from)
            .collect()
        })?;

        // Do the dirty deed. Match by road name; OSM way ID changes a fair bit.
        let map = &ui.primary.map;
        let road_name = map.get_r(road).get_name();
        let mut edits = map.get_edits().clone();
        let mut cnt = 0;
        for l in map.all_lanes() {
            if l.lane_type != from {
                continue;
            }
            if map.get_parent(l.id).get_name() != road_name {
                continue;
            }
            // TODO This looks at the original state of the map, not with all the edits applied so far!
            if can_change_lane_type(l.id, to, map).is_none() {
                edits.lane_overrides.insert(l.id, to);
                cnt += 1;
            }
        }
        // TODO warn about road names changing and being weird. :)
        wizard.acknowledge("Bulk lane edit", || {
            vec![format!(
                "Changed {} {:?} lanes to {:?} lanes on {}",
                cnt, from, to, road_name
            )]
        })?;
        apply_map_edits(&mut ui.primary, &ui.cs, ctx, edits);
        Some(Transition::Pop)
    }))
}

struct LaneEditor {
    brushes: Vec<Paintbrush>,
    active_idx: Option<usize>,
}

struct Paintbrush {
    btn: Button,
    enabled_btn: Button,
    label: String,
    // If this returns a string error message, the edit didn't work.
    apply: Box<dyn Fn(&Map, &mut MapEdits, LaneID) -> Option<String>>,
}

impl LaneEditor {
    fn setup(ctx: &EventCtx) -> LaneEditor {
        // TODO This won't handle resizing well
        let mut x1 = 0.5 * ctx.canvas.window_width;
        let mut make_brush =
            |icon: &str,
             label: &str,
             key: Key,
             apply: Box<dyn Fn(&Map, &mut MapEdits, LaneID) -> Option<String>>| {
                let btn = Button::icon_btn(
                    &format!("assets/ui/edit_{}.png", icon),
                    32.0,
                    label,
                    hotkey(key),
                    ctx,
                )
                .at(ScreenPt::new(x1, 0.0));
                let enabled_btn = Button::icon_btn_bg(
                    &format!("assets/ui/edit_{}.png", icon),
                    32.0,
                    label,
                    hotkey(key),
                    Color::RED,
                    ctx,
                )
                .at(ScreenPt::new(x1, 0.0));

                x1 += 70.0;
                Paintbrush {
                    btn,
                    enabled_btn,
                    label: label.to_string(),
                    apply,
                }
            };

        let brushes = vec![
            make_brush(
                "driving",
                "driving lane",
                Key::D,
                Box::new(|map, edits, l| {
                    if let Some(err) = can_change_lane_type(l, LaneType::Driving, map) {
                        return Some(err);
                    }
                    edits.lane_overrides.insert(l, LaneType::Driving);
                    None
                }),
            ),
            make_brush(
                "bike",
                "protected bike lane",
                Key::B,
                Box::new(|map, edits, l| {
                    if let Some(err) = can_change_lane_type(l, LaneType::Biking, map) {
                        return Some(err);
                    }
                    edits.lane_overrides.insert(l, LaneType::Biking);
                    None
                }),
            ),
            make_brush(
                "bus",
                "bus-only lane",
                Key::T,
                Box::new(|map, edits, l| {
                    if let Some(err) = can_change_lane_type(l, LaneType::Bus, map) {
                        return Some(err);
                    }
                    edits.lane_overrides.insert(l, LaneType::Bus);
                    None
                }),
            ),
            make_brush(
                "parking",
                "on-street parking lane",
                Key::P,
                Box::new(|map, edits, l| {
                    if let Some(err) = can_change_lane_type(l, LaneType::Parking, map) {
                        return Some(err);
                    }
                    edits.lane_overrides.insert(l, LaneType::Parking);
                    None
                }),
            ),
            make_brush(
                "construction",
                "lane closed for construction",
                Key::C,
                Box::new(|map, edits, l| {
                    if let Some(err) = can_change_lane_type(l, LaneType::Construction, map) {
                        return Some(err);
                    }
                    edits.lane_overrides.insert(l, LaneType::Construction);
                    None
                }),
            ),
            make_brush(
                "contraflow",
                "reverse lane direction",
                Key::F,
                Box::new(|map, edits, l| {
                    let lane = map.get_l(l);
                    if !lane.lane_type.is_for_moving_vehicles() {
                        return Some(format!("You can't reverse a {:?} lane", lane.lane_type));
                    }
                    if map.get_r(lane.parent).dir_and_offset(l).1 != 0 {
                        return Some(format!(
                            "You can only reverse the lanes next to the road's yellow center line"
                        ));
                    }
                    edits.contraflow_lanes.insert(l, lane.src_i);
                    None
                }),
            ),
        ];

        LaneEditor {
            brushes,
            active_idx: None,
        }
    }

    fn event(&mut self, ui: &mut UI, ctx: &mut EventCtx) -> Option<Transition> {
        // TODO This is some awkward way to express mutual exclusion. :(
        let mut undo_old = None;
        for (idx, p) in self.brushes.iter_mut().enumerate() {
            if Some(idx) == undo_old {
                p.btn.just_replaced(ctx);
                undo_old = None;
            }

            if self.active_idx == Some(idx) {
                p.enabled_btn.event(ctx);
                if p.enabled_btn.clicked() {
                    self.active_idx = None;
                    p.btn.just_replaced(ctx);
                }
            } else {
                p.btn.event(ctx);
                if p.btn.clicked() {
                    undo_old = self.active_idx;
                    self.active_idx = Some(idx);
                    p.enabled_btn.just_replaced(ctx);
                }
            }
        }
        // Have to do this outside the loop where brushes are all mutably borrowed
        if let Some(idx) = undo_old {
            self.brushes[idx].btn.just_replaced(ctx);
        }

        if let Some(ID::Lane(l)) = ui.primary.current_selection {
            if let Some(idx) = self.active_idx {
                if ctx
                    .input
                    .contextual_action(Key::Space, &self.brushes[idx].label)
                {
                    // These errors are universal.
                    if ui.primary.map.get_l(l).is_sidewalk() {
                        return Some(Transition::Push(msg(
                            "Error",
                            vec!["Can't modify sidewalks"],
                        )));
                    }
                    if ui.primary.map.get_l(l).lane_type == LaneType::SharedLeftTurn {
                        return Some(Transition::Push(msg(
                            "Error",
                            vec!["Can't modify shared-left turn lanes yet"],
                        )));
                    }

                    let mut edits = ui.primary.map.get_edits().clone();
                    if let Some(err) = (self.brushes[idx].apply)(&ui.primary.map, &mut edits, l) {
                        return Some(Transition::Push(msg("Error", vec![err])));
                    }
                    apply_map_edits(&mut ui.primary, &ui.cs, ctx, edits);
                }
            }
        }

        None
    }

    fn draw(&self, g: &mut GfxCtx) {
        for (idx, p) in self.brushes.iter().enumerate() {
            if self.active_idx == Some(idx) {
                p.enabled_btn.draw(g);
            } else {
                p.btn.draw(g);
            }
        }
    }
}
