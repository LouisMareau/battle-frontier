//! Every tuning number of the battle system, as in the prototype's config.js.

#[derive(Clone, Debug)]
pub struct Timing {
    /// contact moves and basic attacks: press at impact (ms either side)
    pub tap: f64,
    /// foe hits: press at impact
    pub block: f64,
    /// status moves: press at the flash
    pub cast: f64,
    /// beams: hold, release when the ring is full...
    pub charge_fill: f64,
    /// ...within this many ms after it fills
    pub charge_window: f64,
    /// multi-hit moves: time between hits
    pub rhythm_gap: f64,
    /// show the shrinking ring
    pub assist: bool,
}

#[derive(Clone, Debug)]
pub struct Spread {
    pub focus: f32,
    pub chain: f32,
    pub storm: f32,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// creatures per side on the field
    pub max_field: usize,
    pub mp_base: f32,
    pub mp_per_level: f32,
    pub basic_restore: i32,
    pub timed_restore: i32,
    /// a move's MP cost from its Gen 3 PP: the first row with pp <= max
    pub cost_by_pp: Vec<(u8, i32)>,
    pub basic_power: u16,
    pub spread_strike: Spread,
    /// moves that already hit every foe
    pub spread_wave: Spread,
    pub status_accuracy: Spread,
    pub timing: Timing,
    pub timed_damage: f32,
    pub timed_block: f32,
    pub timed_echo: f32,
    pub timed_secondary_mul: f64,
    pub timed_block_resist: f64,
    /// cumulative XP for move levels 1-5
    pub xp_levels: [u32; 5],
    pub xp_use: u32,
    pub xp_timed: u32,
    /// (level, power bonus from that level on)
    pub power_per_level: Vec<(u8, f32)>,
    pub lock_min_power: u16,
    pub lock_big_power: u16,
    pub lock_cut: f32,
    pub lock_foe_use: f64,
    pub wild_call_chance: f64,
    pub wild_call_below_hp: f32,
    pub wild_level_spread: i32,
    pub catch_only_when_alone: bool,
    pub screen_rounds: i32,
    pub weather_rounds: i32,
    pub field_per_level: i32,
    pub field_timed: i32,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_field: 6,
            mp_base: 10.0,
            mp_per_level: 0.2,
            basic_restore: 3,
            timed_restore: 1,
            cost_by_pp: vec![(5, 8), (10, 5), (15, 4), (20, 3), (30, 2), (99, 1)],
            basic_power: 35,
            spread_strike: Spread { focus: 1.0, chain: 0.75, storm: 0.5 },
            spread_wave: Spread { focus: 1.0, chain: 0.9, storm: 0.75 },
            status_accuracy: Spread { focus: 1.0, chain: 0.85, storm: 0.7 },
            timing: Timing { tap: 110.0, block: 120.0, cast: 130.0, charge_fill: 900.0, charge_window: 170.0, rhythm_gap: 360.0, assist: true },
            timed_damage: 1.25,
            timed_block: 0.5,
            timed_echo: 0.5,
            timed_secondary_mul: 2.0,
            timed_block_resist: 0.5,
            xp_levels: [0, 8, 22, 42, 70],
            xp_use: 2,
            xp_timed: 2,
            power_per_level: vec![(2, 0.1), (4, 0.2)],
            lock_min_power: 90,
            lock_big_power: 120,
            lock_cut: 0.2,
            lock_foe_use: 0.8,
            wild_call_chance: 0.3,
            wild_call_below_hp: 0.6,
            wild_level_spread: 2,
            catch_only_when_alone: false,
            screen_rounds: 5,
            weather_rounds: 5,
            field_per_level: 1,
            field_timed: 2,
        }
    }
}

/// Morning 4-10, day 10-20, night 20-4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Morning,
    Day,
    Night,
}

impl Period {
    pub fn of_hour(hour: u32) -> Period {
        match hour {
            4..=9 => Period::Morning,
            10..=19 => Period::Day,
            _ => Period::Night,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Period::Morning => "Morning",
            Period::Day => "Day",
            Period::Night => "Night",
        }
    }
}
