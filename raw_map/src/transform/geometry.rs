use abstutil::Timer;
use geom::{Distance, PolyLine};

use crate::{IntersectionType, RawMap};

pub fn finalize_geometry(map: &mut RawMap, timer: &mut Timer) {
    // Remove some roads
    // TODO Can we do this earlier?
    map.roads.retain(|id, road| {
        if id.i1 == id.i2 {
            warn!("Skipping loop {}", id);
            false
        } else if PolyLine::new(road.center_points.clone()).is_err() {
            warn!("Skipping broken geom {}", id);
            false
        } else {
            true
        }
    });

    // Populate some derived fields for Road
    for (id, road) in &mut map.roads {
        let mut lane_specs_ltr = crate::lane_specs::get_lane_specs_ltr(&road.osm_tags, &map.config);
        for l in &mut lane_specs_ltr {
            l.width *= road.scale_width;
        }
        // This unwrap is safe apparently
        // TODO Move untrimmed_road_geometry here -- actually have to do avoid borrow
        /*let (trimmed_center_pts, total_width) = map.untrimmed_road_geometry(*id).unwrap();

        road.trimmed_center_pts = trimmed_center_pts;
        road.half_width = total_width / 2.0;
        road.lane_specs_ltr = lane_specs_ltr;*/
    }

    /*timer.start_iter("find each intersection polygon", map.intersections.len());
    for i in map.intersections.values_mut() {
        timer.next();
        match crate::intersection_polygon(
            i.id,
            i.roads.clone(),
            &mut m.roads,
            &raw.intersections[&i.id].trim_roads_for_merging,
        ) {
            Ok((poly, _)) => {
                i.polygon = poly;
            }
            Err(err) => {
                error!("Can't make intersection geometry for {}: {}", i.id, err);

                // Don't trim lines back at all
                let r = &m.roads[i.roads.iter().next().unwrap()];
                let pt = if r.src_i == i.id {
                    r.trimmed_center_pts.first_pt()
                } else {
                    r.trimmed_center_pts.last_pt()
                };
                i.polygon = Circle::new(pt, Distance::meters(3.0)).to_polygon();

                // Also don't attempt to make Movements later!
                i.intersection_type = IntersectionType::StopSign;
            }
        }
    }*/

    // Some roads near borders get completely squished. Stretch them out here. Attempting to do
    // this in the convert_osm layer doesn't work, because predicting how much roads will be
    // trimmed is impossible.
    /*let min_len = Distance::meters(5.0);
    for i in map.intersections.values_mut() {
        if i.intersection_type != IntersectionType::Border {
            continue;
        }
        let r = map.roads.get_mut(i.roads.iter().next().unwrap()).unwrap();
        if r.trimmed_center_pts.length() >= min_len {
            continue;
        }
        if r.dst_i == i.id {
            r.trimmed_center_pts = r.trimmed_center_pts.extend_to_length(min_len);
        } else {
            r.trimmed_center_pts = r
                .trimmed_center_pts
                .reversed()
                .extend_to_length(min_len)
                .reversed();
        }
        i.polygon = crate::intersection_polygon(
            i.id,
            i.roads.clone(),
            &mut m.roads,
            &raw.intersections[&i.id].trim_roads_for_merging,
        )
        .unwrap()
        .0;
        info!(
            "Shifted border {} out a bit to make the road a reasonable length",
            i.id
        );
    }*/
}
