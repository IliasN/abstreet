use std::collections::HashSet;

use abstutil::{Counter, Timer};
use map_model::{
    DirectedRoadID, IntersectionID, LaneID, Map, Path, PathConstraints, PathRequest, PathStep,
    Pathfinder, Position, RoadID,
};

use crate::{App, Cell, Neighbourhood};

pub struct Shortcuts {
    pub paths: Vec<Path>,
    pub count_per_road: Counter<RoadID>,
    pub count_per_intersection: Counter<IntersectionID>,
}

impl Shortcuts {
    pub fn quiet_and_total_streets(&self, neighbourhood: &Neighbourhood) -> (usize, usize) {
        let quiet_streets = neighbourhood
            .orig_perimeter
            .interior
            .iter()
            .filter(|r| self.count_per_road.get(**r) == 0)
            .count();
        let total_streets = neighbourhood.orig_perimeter.interior.len();
        (quiet_streets, total_streets)
    }
}

pub fn find_shortcuts(app: &App, neighbourhood: &Neighbourhood, timer: &mut Timer) -> Shortcuts {
    let map = &app.map;
    let modal_filters = &app.session.modal_filters;
    // The overall approach: look for all possible paths from an entrance to an exit, only if they
    // connect to different major roads.
    //
    // But an entrance and exit to _what_? If we try to route from the entrance to one cell to the
    // exit of another, then the route will make strange U-turns and probably use the perimeter. By
    // definition, two cells aren't reachable without using the perimeter. So restrict our search
    // to pairs of entrances/exits in the _same_ cell.
    let mut requests = Vec::new();

    for cell in &neighbourhood.cells {
        let entrances = find_entrances(map, neighbourhood, cell);
        let exits = find_exits(map, neighbourhood, cell);

        for entrance in &entrances {
            for exit in &exits {
                if entrance.major_road_name != exit.major_road_name {
                    requests.push(PathRequest::vehicle(
                        Position::start(entrance.lane),
                        Position::end(exit.lane, map),
                        PathConstraints::Car,
                    ));
                }
            }
        }
    }

    let mut params = map.routing_params().clone();
    modal_filters.update_routing_params(&mut params);
    // Don't allow leaving the neighbourhood and using perimeter roads at all. Even if the optimal
    // path is to leave and re-enter, don't do that. The point of this view is to show possible
    // detours people might try to take in response to one filter. Note the original "demand model"
    // input is bogus anyway; it's all possible entrances and exits to the neighbourhood, without
    // regards for the larger path somebody actually wants to take.
    params.avoid_roads.extend(neighbourhood.perimeter.clone());

    let pathfinder = Pathfinder::new_dijkstra(map, params, vec![PathConstraints::Car], timer);
    let paths: Vec<Path> = timer
        .parallelize(
            "calculate paths between entrances and exits",
            requests,
            |req| {
                pathfinder
                    .pathfind_v2(req, map)
                    .and_then(|path| path.into_v1(map).ok())
            },
        )
        .into_iter()
        .flatten()
        .collect();

    // TODO Rank the likeliness of each shortcut by
    // 1) Calculating a path between similar start/endpoints -- travelling along the perimeter,
    //    starting and ending on a specific road that makes sense. (We have to pick the 'direction'
    //    along the perimeter roads that's sensible.)
    // 2) Comparing that time to the time for cutting through

    // How many shortcuts pass through each street?
    let mut count_per_road = Counter::new();
    let mut count_per_intersection = Counter::new();
    for path in &paths {
        for step in path.get_steps() {
            match step {
                PathStep::Lane(l) => {
                    if neighbourhood.orig_perimeter.interior.contains(&l.road) {
                        count_per_road.inc(l.road);
                    }
                }
                PathStep::Turn(t) => {
                    if neighbourhood.interior_intersections.contains(&t.parent) {
                        count_per_intersection.inc(t.parent);
                    }
                }
                // Car paths don't make contraflow movements
                _ => unreachable!(),
            }
        }
    }

    Shortcuts {
        paths,
        count_per_road,
        count_per_intersection,
    }
}

struct EntryExit {
    // TODO Really this is a DirectedRoadID, but since pathfinding later needs to know lanes, just
    // use this
    lane: LaneID,
    major_road_name: String,
}

fn find_entrances(map: &Map, neighbourhood: &Neighbourhood, cell: &Cell) -> Vec<EntryExit> {
    let mut entrances = Vec::new();
    for i in &cell.borders {
        if let Some(major_road_name) = find_major_road_name(map, neighbourhood, *i) {
            let mut seen: HashSet<DirectedRoadID> = HashSet::new();
            for l in map.get_i(*i).get_outgoing_lanes(map, PathConstraints::Car) {
                let dr = map.get_l(l).get_directed_parent();
                if !seen.contains(&dr) && cell.roads.contains_key(&dr.road) {
                    entrances.push(EntryExit {
                        lane: l,
                        major_road_name: major_road_name.clone(),
                    });
                    seen.insert(dr);
                }
            }
        }
    }
    entrances
}

fn find_exits(map: &Map, neighbourhood: &Neighbourhood, cell: &Cell) -> Vec<EntryExit> {
    let mut exits = Vec::new();
    for i in &cell.borders {
        if let Some(major_road_name) = find_major_road_name(map, neighbourhood, *i) {
            let mut seen: HashSet<DirectedRoadID> = HashSet::new();
            for l in map.get_i(*i).get_incoming_lanes(map, PathConstraints::Car) {
                let dr = map.get_l(l).get_directed_parent();
                if !seen.contains(&dr) && cell.roads.contains_key(&dr.road) {
                    exits.push(EntryExit {
                        lane: l,
                        major_road_name: major_road_name.clone(),
                    });
                    seen.insert(dr);
                }
            }
        }
    }
    exits
}

fn find_major_road_name(
    map: &Map,
    neighbourhood: &Neighbourhood,
    i: IntersectionID,
) -> Option<String> {
    let mut names = Vec::new();
    for r in &map.get_i(i).roads {
        if neighbourhood.perimeter.contains(r) {
            names.push(map.get_r(*r).get_name(None));
        }
    }
    names.sort();
    names.dedup();
    // TODO If the major road changes names or we found a corner, bail out
    if names.len() == 1 {
        names.pop()
    } else {
        None
    }
}
