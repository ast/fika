use std::collections::BTreeMap;

use fika_modem::Profile;

#[derive(Clone, Debug)]
pub struct HeardEntry {
    pub call: String,
    pub snr_db: f32,
    pub lane: usize,
    pub profile: Profile,
    pub last_epoch: f64,
    pub count: usize,
    pub grid: Option<String>,
}

#[derive(Default)]
pub struct HeardList {
    entries: BTreeMap<u32, HeardEntry>,
}

impl HeardList {
    pub fn update(
        &mut self,
        packed: u32,
        call: String,
        snr_db: f32,
        lane: usize,
        profile: Profile,
        epoch: f64,
    ) {
        let e = self.entries.entry(packed).or_insert(HeardEntry {
            call: call.clone(),
            snr_db,
            lane,
            profile,
            last_epoch: epoch,
            count: 0,
            grid: None,
        });
        e.call = call;
        e.snr_db = snr_db;
        e.lane = lane;
        e.profile = profile;
        e.last_epoch = epoch;
        e.count += 1;
    }

    pub fn set_grid(&mut self, packed: u32, grid: String) {
        if let Some(e) = self.entries.get_mut(&packed) {
            e.grid = Some(grid);
        }
    }

    /// Most recently heard first.
    pub fn sorted(&self) -> Vec<&HeardEntry> {
        let mut v: Vec<&HeardEntry> = self.entries.values().collect();
        v.sort_by(|a, b| b.last_epoch.total_cmp(&a.last_epoch));
        v
    }

    pub fn snr_of(&self, packed: u32) -> Option<f32> {
        self.entries.get(&packed).map(|e| e.snr_db)
    }
}
