use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
};

use crate::id::Id;

pub(crate) struct Store {
    map: HashMap<(Id, TypeId), Box<dyn Any>>,
    live: HashSet<(Id, TypeId)>,
}

impl Store {
    pub fn new() -> Self {
        Store {
            map: HashMap::new(),
            live: HashSet::new(),
        }
    }
    pub fn get_or<T: Any + Default>(&mut self, id: &Id) -> &mut T {
        self.get_or_with(id, T::default)
    }

    pub fn get<T: Any>(&self, id: &Id) -> Option<&T> {
        let key = (id.clone(), TypeId::of::<T>());
        self.map
            .get(&key)
            .map(|val| val.downcast_ref::<T>().unwrap())
    }

    pub fn get_mut<T: Any>(&mut self, id: &Id) -> Option<&mut T> {
        let key = (id.clone(), TypeId::of::<T>());
        self.map
            .get_mut(&key)
            .map(|val| val.downcast_mut::<T>().unwrap())
    }

    pub fn get_or_with<T: Any>(&mut self, id: &Id, make: impl FnOnce() -> T) -> &mut T {
        let key = (id.clone(), TypeId::of::<T>());
        self.live.insert(key.clone());
        self.map
            .entry(key)
            .or_insert_with(|| Box::new(make()))
            .downcast_mut::<T>()
            .unwrap()
    }
    pub fn sweep(&mut self) {
        self.map.retain(|k, _| self.live.contains(k));
        self.live.clear();
    }
}
