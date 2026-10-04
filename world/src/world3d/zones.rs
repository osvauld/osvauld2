use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZonePhase3d {
    Enter,
    Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneEvent3d {
    pub phase: ZonePhase3d,
    pub id: String,
    pub who: String,
    pub tick: u64,
}

impl World3d {
    pub fn drain_zone_events(&mut self) -> Vec<ZoneEvent3d> {
        self.events.drain(..).collect()
    }

    pub(super) fn push_zone_event(&mut self, event: ZoneEvent3d) {
        if self.events.len() < 4096 {
            self.events.push_back(event);
        } else {
            self.dropped_zone_events = self.dropped_zone_events.saturating_add(1);
        }
    }

    pub(super) fn update_zones(&mut self) {
        let bodies: HashMap<_, _> = self
            .ids
            .iter()
            .map(|(id, e)| {
                let body = self.ecs.get::<Body>(*e).unwrap();
                (body.handle, (id, &body.spec))
            })
            .collect();
        let mut next = std::collections::BTreeSet::new();
        for (a, b) in self.physics.sensor_pairs() {
            let (Some((a_id, a_spec)), Some((b_id, b_spec))) = (bodies.get(&a), bodies.get(&b))
            else {
                continue;
            };
            if a_spec.sensor && b_spec.dynamic {
                next.insert(((*a_id).clone(), (*b_id).clone()));
            }
            if b_spec.sensor && a_spec.dynamic {
                next.insert(((*b_id).clone(), (*a_id).clone()));
            }
        }
        let mut events = Vec::new();
        for (id, who) in self.zones.difference(&next) {
            events.push(ZoneEvent3d {
                phase: ZonePhase3d::Leave,
                id: id.clone(),
                who: who.clone(),
                tick: self.tick(),
            });
        }
        for (id, who) in next.difference(&self.zones) {
            events.push(ZoneEvent3d {
                phase: ZonePhase3d::Enter,
                id: id.clone(),
                who: who.clone(),
                tick: self.tick(),
            });
        }
        self.zones = next;
        for event in events {
            self.push_zone_event(event);
        }
    }

    pub(super) fn prune_zones(&mut self) {
        let removed: Vec<_> = self
            .zones
            .iter()
            .filter(|(id, who)| !self.ids.contains_key(id) || !self.ids.contains_key(who))
            .cloned()
            .collect();
        for (id, who) in removed {
            self.zones.remove(&(id.clone(), who.clone()));
            if self.ids.contains_key(&who) {
                self.push_zone_event(ZoneEvent3d {
                    phase: ZonePhase3d::Leave,
                    id,
                    who,
                    tick: self.tick(),
                });
            }
        }
    }
}
