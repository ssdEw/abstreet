mod psrc;
mod seattle;
mod utils;

struct Job {
    osm_to_raw: bool,
    raw_to_map: bool,
    scenario: bool,

    use_fixes: bool,
    only_map: Option<String>,
}

fn main() {
    let mut args = abstutil::CmdArgs::new();
    let job = Job {
        // Download all raw input files, then convert OSM to the intermediate RawMap.
        osm_to_raw: args.enabled("--raw"),
        // Convert the RawMap to the final Map format.
        raw_to_map: args.enabled("--map"),
        // Download trip demand data, then produce the typical weekday scenario.
        scenario: args.enabled("--scenario"),

        // By default, use geometry fixes from map_editor.
        use_fixes: !args.enabled("--nofixes"),
        // Only process one map. If not specified, process all maps defined by clipping polygons in
        // data/input/polygons/.
        only_map: args.optional_free(),
    };
    args.done();
    if !job.osm_to_raw && !job.raw_to_map && !job.scenario {
        println!("Nothing to do! Pass some combination of --raw, --map, --scenario");
        std::process::exit(1);
    }

    let names = if let Some(n) = job.only_map {
        println!("- Just working on {}", n);
        vec![n]
    } else {
        println!("- Working on all Seattle maps");
        abstutil::list_all_objects("../data/input/polygons".to_string())
    };

    for name in names {
        if job.osm_to_raw {
            seattle::osm_to_raw(&name);
        }

        if job.raw_to_map {
            utils::raw_to_map(&name, job.use_fixes);
        }

        if job.scenario {
            seattle::ensure_popdat_exists(job.use_fixes);

            let mut timer = abstutil::Timer::new(format!("Scenario for {}", name));
            let map = map_model::Map::new(abstutil::path_map(&name), job.use_fixes, &mut timer);
            popdat::trips_to_scenario(&map, &mut timer).save();
        }
    }
}
