use crate::objects::{DrawCtx, ID};
use crate::plugins::sim::new_des_model;
use crate::plugins::{BlockingPlugin, PluginCtx};
use crate::render::MIN_ZOOM_FOR_DETAIL;
use ezgui::{EventLoopMode, GfxCtx, Key};
use geom::{Distance, Duration, Speed};
use map_model::{LaneID, Map, Traversable};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;
use sim::{
    CarID, DrawCarInput, DrawPedestrianInput, GetDrawAgents, PedestrianID, Tick, VehicleType,
};

pub struct EvenSimplerModelController {
    current_tick: Tick,
    sim: new_des_model::DrivingSimState,
    auto_mode: bool,
}

impl EvenSimplerModelController {
    pub fn new(ctx: &mut PluginCtx) -> Option<EvenSimplerModelController> {
        if let Some(ID::Lane(id)) = ctx.primary.current_selection {
            if ctx.primary.map.get_l(id).is_driving()
                && ctx
                    .input
                    .contextual_action(Key::Z, "start even simpler model")
            {
                return Some(EvenSimplerModelController {
                    current_tick: Tick::zero(),
                    sim: populate_sim(id, &ctx.primary.map),
                    auto_mode: false,
                });
            }
        }
        None
    }
}

impl BlockingPlugin for EvenSimplerModelController {
    fn blocking_event(&mut self, ctx: &mut PluginCtx) -> bool {
        ctx.input.set_mode_with_prompt(
            "Even Simpler Model",
            format!("Even Simpler Model at {}", self.current_tick.as_time()),
            &ctx.canvas,
        );
        if self.auto_mode {
            ctx.hints.mode = EventLoopMode::Animation;
            if ctx.input.modal_action("toggle forwards play") {
                self.auto_mode = false;
            } else if ctx.input.is_update_event() {
                self.current_tick = self.current_tick.next();
                self.sim
                    .step_if_needed(self.current_tick.as_time(), &ctx.primary.map);
            }
        } else {
            if ctx.input.modal_action("forwards") {
                self.current_tick = self.current_tick.next();
                self.sim
                    .step_if_needed(self.current_tick.as_time(), &ctx.primary.map);
            } else if ctx.input.modal_action("toggle forwards play") {
                self.auto_mode = true;
                ctx.hints.mode = EventLoopMode::Animation;
            } else if ctx.input.modal_action("spawn tons of cars everywhere") {
                self.current_tick = Tick::zero();
                self.sim = densely_populate_sim(&ctx.primary.map);
            }
        }
        if ctx.input.modal_action("quit") {
            return false;
        }
        true
    }

    fn draw(&self, g: &mut GfxCtx, ctx: &DrawCtx) {
        if g.canvas.cam_zoom < MIN_ZOOM_FOR_DETAIL {
            self.sim
                .draw_unzoomed(self.current_tick.as_time(), g, &ctx.map);
        }
    }
}

impl GetDrawAgents for EvenSimplerModelController {
    fn tick(&self) -> Tick {
        self.current_tick
    }

    fn get_draw_car(&self, id: CarID, map: &Map) -> Option<DrawCarInput> {
        self.sim
            .get_all_draw_cars(self.current_tick.as_time(), map)
            .into_iter()
            .find(|x| x.id == id)
    }

    fn get_draw_ped(&self, _id: PedestrianID, _map: &Map) -> Option<DrawPedestrianInput> {
        None
    }

    fn get_draw_cars(&self, on: Traversable, map: &Map) -> Vec<DrawCarInput> {
        self.sim
            .get_draw_cars_on(self.current_tick.as_time(), on, map)
    }

    fn get_draw_peds(&self, _on: Traversable, _map: &Map) -> Vec<DrawPedestrianInput> {
        Vec::new()
    }

    fn get_all_draw_cars(&self, map: &Map) -> Vec<DrawCarInput> {
        self.sim.get_all_draw_cars(self.current_tick.as_time(), map)
    }

    fn get_all_draw_peds(&self, _map: &Map) -> Vec<DrawPedestrianInput> {
        Vec::new()
    }
}

fn populate_sim(start: LaneID, map: &Map) -> new_des_model::DrivingSimState {
    let mut sim = new_des_model::DrivingSimState::new(map);

    let mut sources = vec![start];
    // Try to find a lane likely to have conflicts
    {
        for t in map.get_turns_from_lane(start) {
            if t.turn_type == map_model::TurnType::Straight {
                if let Some(l) = map
                    .get_parent(t.id.dst)
                    .any_on_other_side(t.id.dst, map_model::LaneType::Driving)
                {
                    sources.push(l);
                }
            }
        }
    }

    let mut counter = 0;
    let mut rng = XorShiftRng::from_seed([42; 16]);
    for source in sources {
        let len = map.get_l(source).length();
        if len < new_des_model::MAX_VEHICLE_LENGTH {
            println!("Can't spawn cars on {}, it's only {} long", source, len);
            continue;
        }

        for _ in 0..10 {
            if spawn_car(&mut sim, &mut rng, map, counter, source) {
                counter += 1;
            }
        }
    }

    sim
}

fn densely_populate_sim(map: &Map) -> new_des_model::DrivingSimState {
    let mut sim = new_des_model::DrivingSimState::new(map);
    let mut rng = XorShiftRng::from_seed([42; 16]);
    let mut counter = 0;

    for l in map.all_lanes() {
        let len = l.length();
        if l.is_driving() && len >= new_des_model::MAX_VEHICLE_LENGTH {
            for _ in 0..rng.gen_range(0, 5) {
                if spawn_car(&mut sim, &mut rng, map, counter, l.id) {
                    counter += 1;
                }
            }
        }
    }

    sim
}

fn spawn_car(
    sim: &mut new_des_model::DrivingSimState,
    rng: &mut XorShiftRng,
    map: &Map,
    id: usize,
    start_lane: LaneID,
) -> bool {
    let path = random_path(start_lane, rng, map);
    let max_speed = if rng.gen_bool(0.1) {
        Some(Speed::miles_per_hour(10.0))
    } else {
        None
    };
    let last_lane = path.last().unwrap().as_lane();
    let vehicle_len = rand_dist(
        rng,
        new_des_model::MIN_VEHICLE_LENGTH,
        new_des_model::MAX_VEHICLE_LENGTH,
    );
    let start_dist = rand_dist(rng, vehicle_len, map.get_l(start_lane).length());
    let end_dist = rand_dist(rng, Distance::ZERO, map.get_l(last_lane).length());
    if path.len() == 1 && start_dist >= end_dist {
        return false;
    }
    let spawn_time = Duration::seconds(0.2) * (id % 5) as f64;

    sim.spawn_car(
        new_des_model::Vehicle {
            id: CarID::tmp_new(id, VehicleType::Car),
            vehicle_type: VehicleType::Car,
            length: vehicle_len,
            max_speed,
        },
        path,
        spawn_time,
        start_dist,
        end_dist,
        map,
    );
    true
}

fn random_path(start: LaneID, rng: &mut XorShiftRng, map: &Map) -> Vec<Traversable> {
    let mut path = vec![Traversable::Lane(start)];
    let mut last_lane = start;
    for _ in 0..5 {
        if let Some(t) = map.get_turns_from_lane(last_lane).choose(rng) {
            path.push(Traversable::Turn(t.id));
            path.push(Traversable::Lane(t.id.dst));
            last_lane = t.id.dst;
        } else {
            break;
        }
    }
    path
}

fn rand_dist(rng: &mut XorShiftRng, low: Distance, high: Distance) -> Distance {
    Distance::meters(rng.gen_range(low.inner_meters(), high.inner_meters()))
}