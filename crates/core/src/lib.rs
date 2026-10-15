//! The game's rules, with no rendering and no platform code: the battle system
//! (Sea of Stars-style timing, MP, move levels, locks, rounds ordered by Speed,
//! battles that grow up to 6v6), creatures and the data they come from.
//!
//! Ported from F:\Emulation\BattlePrototype\src\engine (JavaScript), same rules and
//! numbers; the tests in tests/ are that prototype's tests.

pub mod ai;
pub mod battle;
pub mod config;
pub mod dex;
pub mod mon;
pub mod moves;
