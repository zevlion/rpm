use std::collections::HashMap;
use crate::ipc::ProcessInfo;

pub struct Engine {
    pub processes: HashMap<usize, ProcessInfo>,
    pub next_id: usize,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 0,
        }
    }
}