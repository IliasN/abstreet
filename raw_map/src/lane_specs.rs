use osm2lanes::road::Designated;
use osm2lanes::tag::TagsWrite;

use abstutil::Tags;
use geom::Distance;

use crate::{Direction, DrivingSide, LaneSpec, LaneType, MapConfig};

pub fn get_lane_specs_ltr(orig_tags: &Tags, cfg: &MapConfig) -> Vec<LaneSpec> {
    // Special cases first
    if orig_tags.is_any("railway", vec!["light_rail", "rail"]) {
        return vec![LaneSpec {
            lt: LaneType::LightRail,
            dir: Direction::Fwd,
            width: LaneSpec::typical_lane_widths(LaneType::LightRail, orig_tags)[0].0,
        }];
    }

    let tags = transform_tags(orig_tags, cfg);
    let locale = osm2lanes::locale::Config::new()
        .driving_side(match cfg.driving_side {
            DrivingSide::Right => osm2lanes::locale::DrivingSide::Right,
            DrivingSide::Left => osm2lanes::locale::DrivingSide::Left,
        })
        .build();
    let mut config = osm2lanes::transform::TagsToLanesConfig::default();
    config.error_on_warnings = false;
    config.include_separators = true;

    //println!("{:?}", orig_tags.get("abst:osm_way_id"));
    match osm2lanes::transform::tags_to_lanes(&tags, &locale, &config) {
        Ok(output) => {
            let mut result = output
                .road
                .lanes
                .into_iter()
                .map(|lane| transform_lane(lane, &locale))
                .flatten()
                .collect::<Vec<_>>();

            // No shoulders on unwalkable roads
            if orig_tags.is_any(
                crate::osm::HIGHWAY,
                vec!["motorway", "motorway_link", "construction"],
            ) || orig_tags.is("foot", "no")
                || orig_tags.is("access", "no")
                || orig_tags.is("motorroad", "yes")
            {
                result.retain(|lane| lane.lt != LaneType::Shoulder);
            }

            if output.road.highway.is_construction() {
                // Remove sidewalks and make everything else a construction lane
                result.retain(|lane| !lane.lt.is_walkable());
                for lane in &mut result {
                    lane.lt = LaneType::Construction;
                }
            }

            // If there's no driving lane, ignore any assumptions about parking
            // (https://www.openstreetmap.org/way/6449188 is an example)
            if result.iter().all(|lane| lane.lt != LaneType::Driving) {
                result.retain(|lane| lane.lt != LaneType::Parking);
            }

            // Use our own widths for the moment
            for lane in &mut result {
                lane.width = LaneSpec::typical_lane_widths(lane.lt, &orig_tags)[0].0;
            }

            // Fix direction on outer lanes
            for (idx, lane) in result.iter_mut().enumerate() {
                if lane.lt == LaneType::Sidewalk || lane.lt == LaneType::Shoulder {
                    if idx == 0 {
                        lane.dir = if cfg.driving_side == DrivingSide::Right {
                            Direction::Back
                        } else {
                            Direction::Fwd
                        };
                    } else {
                        // Assume last
                        lane.dir = if cfg.driving_side == DrivingSide::Right {
                            Direction::Fwd
                        } else {
                            Direction::Back
                        };
                    }
                }
            }

            result
        }
        Err(err) => {
            error!("Broke on {:?}: {}", orig_tags, err);
            vec![LaneSpec {
                lt: LaneType::Driving,
                dir: Direction::Fwd,
                width: Distance::meters(1.0),
            }]
        }
    }
}

fn transform_tags(tags: &Tags, cfg: &MapConfig) -> osm2lanes::tag::Tags {
    let mut tags = tags.clone();

    // Patch around some common issues
    if tags.is("sidewalk", "none") {
        tags.insert("sidewalk", "no");
    }
    if tags.is("oneway", "reversible") {
        tags.insert("oneway", "yes");
    }
    if tags.is("highway", "living_street") {
        tags.insert("highway", "residential");
    }

    if tags.is("sidewalk", "separate") && cfg.inferred_sidewalks {
        // Make blind guesses
        let value = if tags.is("oneway", "yes") {
            if cfg.driving_side == DrivingSide::Right {
                "right"
            } else {
                "left"
            }
        } else {
            "both"
        };
        tags.insert("sidewalk", value);
    }

    // Multiple bus schemas
    if tags.has_any(vec!["bus:lanes:forward", "bus:lanes:backward"])
        && tags.has_any(vec!["lanes:bus:forward", "lanes:bus:backward"])
    {
        // Arbitrarily pick one!
        tags.remove("lanes:bus:forward");
        tags.remove("lanes:bus:backward");
    }

    // Super common in Bristol
    /*if tags.is(if cfg.driving_side == DrivingSide::Right {"busway:left" } else { "busway:right" }, "lane") && !(tags.is("oneway", "yes") || tags.is("oneway:bus", "yes")) {
        tags.insert("oneway:bus", "yes");
    }*/

    let mut result = osm2lanes::tag::Tags::default();
    for (k, v) in tags.inner() {
        result.checked_insert(k.to_string(), v).unwrap();
    }
    result
}

fn transform_lane(
    lane: osm2lanes::road::Lane,
    locale: &osm2lanes::locale::Locale,
) -> Vec<LaneSpec> {
    use osm2lanes::road::Lane;

    let mut lt;
    let dir;
    match lane {
        Lane::Travel {
            direction,
            designated,
            ..
        } => {
            lt = match designated {
                Designated::Foot => LaneType::Sidewalk,
                Designated::Motor => LaneType::Driving,
                Designated::Bicycle => LaneType::Biking,
                Designated::Bus => LaneType::Bus,
            };
            match direction {
                Some(direction) => match direction {
                    osm2lanes::road::Direction::Forward => {
                        dir = Direction::Fwd;
                    }
                    osm2lanes::road::Direction::Backward => {
                        dir = Direction::Back;
                    }
                    osm2lanes::road::Direction::Both => {
                        match designated {
                            Designated::Motor => {
                                lt = LaneType::SharedLeftTurn;
                                dir = Direction::Fwd;
                            }
                            Designated::Bicycle => {
                                // Rewrite one bidirectional cycletrack into two lanes
                                let width = Distance::meters(lane.width(locale).val());
                                // TODO driving side and also side of the road???
                                let (dir1, dir2) = (Direction::Back, Direction::Fwd);
                                return vec![
                                    LaneSpec {
                                        lt: LaneType::Biking,
                                        dir: dir1,
                                        width,
                                    },
                                    LaneSpec {
                                        lt: LaneType::Biking,
                                        dir: dir2,
                                        width,
                                    },
                                ];
                            }
                            x => todo!("dir=both, designated={:?}", x),
                        }
                    }
                },
                // Fix later
                None => {
                    dir = Direction::Fwd;
                }
            };
        }
        Lane::Shoulder { .. } => {
            lt = LaneType::Shoulder;
            // Fix later
            dir = Direction::Fwd;
        }
        Lane::Separator { .. } => {
            // TODO Barriers
            return Vec::new();
        }
        Lane::Parking {
            direction,
            designated: Designated::Motor,
            ..
        } => {
            lt = LaneType::Parking;
            dir = match direction {
                osm2lanes::road::Direction::Forward => Direction::Fwd,
                osm2lanes::road::Direction::Backward => Direction::Back,
                osm2lanes::road::Direction::Both => todo!("dir = both for parking"),
            }
        }
        _ => todo!("handle {:?}", lane),
    }
    let width = Distance::meters(lane.width(locale).val());
    vec![LaneSpec { lt, dir, width }]
}
