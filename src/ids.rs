//! Deterministic 4-character id factory (FNV-1a seed, mulberry32).
use crate::assets::{seed_from, ALPHABET};
use std::collections::HashSet;

pub struct IdFactory {
    state: u32,
    used: HashSet<String>,
}

impl IdFactory {
    pub fn new(seed_text: &str) -> Self {
        let seed = seed_from(seed_text);
        IdFactory { state: if seed == 0 { 0x9e3779b9 } else { seed }, used: HashSet::new() }
    }

    fn random(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let s = self.state;
        let mut t = (s ^ (s >> 15)).wrapping_mul(1 | s);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }

    pub fn next(&mut self) -> String {
        loop {
            let mut id = String::new();
            for _ in 0..4 {
                let i = (self.random() * ALPHABET.len() as f64).floor() as usize;
                id.push(ALPHABET[i] as char);
            }
            if !self.used.contains(&id) {
                self.used.insert(id.clone());
                return id;
            }
        }
    }

    pub fn reserve(&mut self, id: &str) {
        self.used.insert(id.to_string());
    }

    pub fn has(&self, id: &str) -> bool {
        self.used.contains(id)
    }
}
