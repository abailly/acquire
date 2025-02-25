use core::fmt;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::event::*;
use crate::side::*;
use crate::tech::*;

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct WarState {
    pub resources: u8,
    pub vp: u8,
    pub technologies: Box<Technologies>,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Phase {
    Initiative(Side),
    DrawEvents,
    CollectResources,
    ImproveTechnologies(Side),
    LaunchOffensives(Side),
    Reinforcements(Side),
    UBoot,
    Blockade,
    NewTurn,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GameState {
    pub current_turn: u8,
    pub phase: Phase,
    pub initiative: Side,
    pub winner: Option<Side>,
    pub russian_revolution: u8,
    pub lafayette: Option<u8>,
    pub nations: HashMap<Nation, NationState>,
    pub countries: HashMap<Nation, Country>,
    pub state_of_war: HashMap<Side, WarState>,
    pub end_game_this_turn: bool,
    seed: u64,
    rng: StdRng,
    events_pool: Vec<Event>,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Offensive {
    pub initiative: Side,
    pub from: Nation,
    pub to: Nation,
    pub pr: u8,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum OffensiveOutcome {
    NotEnoughResources(u8, u8),
    OperationalLevelTooLow(u8, u8),
    Hits(HitsResult),
}

impl Display for OffensiveOutcome {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            OffensiveOutcome::NotEnoughResources(pr, resources) => {
                write!(f, "Not enough resources ({} < {})", pr, resources)
            }
            OffensiveOutcome::OperationalLevelTooLow(level, required) => {
                write!(f, "Operational level too low ({} < {})", level, required)
            }
            OffensiveOutcome::Hits(result) => write!(f, "{}", result),
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum HitsResult {
    Surrenders(Nation),
    Winner(Side),
    Hits(Nation, u8),
    NationNotAtWar(Nation),
    NoResult,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum TechnologyImprovement {
    ImprovedTechnology(TechnologyType, u8),
    FailedTechnology(TechnologyType, u8),
    TechnologyNotAvailable(String, u16, u16),
    NoMoreTechnologyImprovement(TechnologyType, u8),
}

impl Display for TechnologyImprovement {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TechnologyImprovement::ImprovedTechnology(tech, pr) => {
                write!(f, "Improved {}, spent {}", tech, pr)
            }
            TechnologyImprovement::FailedTechnology(tech, pr) => {
                write!(f, "Failed to improve {}, spent {}", tech, pr)
            }
            TechnologyImprovement::TechnologyNotAvailable(tech, not_before, current) => {
                write!(
                    f,
                    "Technology {} not available before {} (year: {})",
                    tech, not_before, current
                )
            }
            TechnologyImprovement::NoMoreTechnologyImprovement(tech, level) => {
                write!(f, "No more improvement for {} at level {}", tech, level)
            }
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct ActiveEvent {
    pub event: Event,
    pub deactivation: fn(&GameState) -> bool,
}

impl Display for HitsResult {
    #[allow(unused_must_use)]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            HitsResult::Surrenders(nation) => writeln!(f, "{} surrenders", nation),
            HitsResult::Winner(side) => writeln!(f, "{} wins", side),
            HitsResult::Hits(nation, hits) => writeln!(f, "{} takes {} hits", nation, hits),
            HitsResult::NationNotAtWar(nation) => writeln!(f, "{} is not at war", nation),
            HitsResult::NoResult => writeln!(f, "No result"),
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum StateChange {
    NoChange,
    ChangeResources { side: Side, pr: i8 },
    MoreChanges(Vec<StateChange>),
}

impl StateChange {
    pub(crate) fn allies_loss(&self) -> u8 {
        match self {
            StateChange::ChangeResources { side, pr } if *pr < 0 && *side == Side::Allies => {
                -*pr as u8
            }
            StateChange::MoreChanges(changes) => changes
                .iter()
                .fold(0, |acc, change| acc + change.allies_loss()),
            _ => 0,
        }
    }

    pub(crate) fn empires_gain(&self) -> u8 {
        match self {
            StateChange::ChangeResources { side, pr } if *pr > 0 && *side == Side::Empires => {
                *pr as u8
            }
            StateChange::MoreChanges(changes) => changes
                .iter()
                .fold(0, |acc, change| acc + change.empires_gain()),
            _ => 0,
        }
    }
}

impl GameState {
    pub fn new(seed: u64) -> Self {
        let nations = INITIAL_NATION_STATE.iter().cloned().collect();
        let countries = COUNTRIES.iter().cloned().collect();
        let initial_state_of_war: HashMap<Side, WarState> = [
            (
                Side::Allies,
                WarState {
                    resources: 0,
                    vp: 0,
                    technologies: Box::new(initial_technologies()),
                },
            ),
            (
                Side::Empires,
                WarState {
                    resources: 0,
                    vp: 0,
                    technologies: Box::new(initial_technologies()),
                },
            ),
        ]
        .iter()
        .cloned()
        .collect();

        GameState {
            current_turn: 1,
            phase: Phase::DrawEvents,
            initiative: Side::Empires,
            winner: None,
            russian_revolution: 0,
            lafayette: None,
            nations,
            countries,
            state_of_war: initial_state_of_war,
            end_game_this_turn: false,
            seed,
            rng: StdRng::seed_from_u64(seed),
            events_pool: ALL_EVENTS
                .iter()
                .filter(|e| e.year == 1914)
                .cloned()
                .collect(),
        }
    }

    pub fn tally_resources(&self, pr_for_side: &Side) -> u8 {
        self.nations
            .iter()
            .fold(0, |acc, (nation, status)| match status {
                NationState::AtWar(breakdown) => match self.countries.get(nation) {
                    Some(Country {
                        side, resources, ..
                    }) if side == pr_for_side => {
                        acc + if *nation == Nation::Russia {
                            operational_level(*breakdown) * 2
                        } else {
                            *resources
                        }
                    }
                    _ => acc,
                },
                _ => acc,
            })
    }

    pub fn increase_pr(&mut self, side: Side, pr: u8) -> &mut Self {
        let st = self.state_of_war.get_mut(&side).unwrap();
        st.resources += pr;
        if st.resources > 20 {
            st.resources = 20;
        }
        self
    }

    pub fn decrease_pr(&mut self, side: Side, pr: u8) -> &mut Self {
        let st = self.state_of_war.get_mut(&side).unwrap();
        st.resources -= pr;
        self
    }

    pub fn resources_for(&self, side: &Side) -> u8 {
        self.state_of_war.get(side).unwrap().resources
    }

    pub fn roll(&mut self) -> u8 {
        self.rng.gen_range(1..=6)
    }

    pub fn current_year(&self) -> u16 {
        match self.current_turn {
            1 => 1914,
            2..=4 => 1915,
            5..=7 => 1916,
            8..=10 => 1917,
            11..=13 => 1918,
            14 => 1919,
            turn => panic!("Invalid turn {}", turn),
        }
    }

    pub fn all_nations_at_war(&self, initiative: Side) -> Vec<Nation> {
        self.nations
            .iter()
            .filter_map(|(nation, status)| match status {
                NationState::AtWar(_) => Some(*nation),
                _ => None,
            })
            .filter(|nation| self.countries.get(nation).unwrap().side == initiative)
            .collect()
    }

    pub fn artillery_bonus(&self, initiative: &Side) -> u8 {
        self.state_of_war
            .get(initiative)
            .unwrap()
            .technologies
            .artillery
    }

    pub fn attack_bonus(&self, initiative: &Side) -> u8 {
        self.state_of_war
            .get(initiative)
            .unwrap()
            .technologies
            .attack
    }

    pub fn defense_bonus(&self, initiative: &Side) -> u8 {
        self.state_of_war
            .get(initiative)
            .unwrap()
            .technologies
            .defense
    }

    pub fn surrenders(&mut self, to: &Nation) -> HitsResult {
        let side = self.countries.get(to).unwrap().side.other();
        self.state_of_war.get_mut(&side).unwrap().vp += self.countries.get(to).unwrap().vp;
        self.nations.insert(*to, NationState::AtPeace);
        if self.roll() < self.state_of_war.get(&side).unwrap().vp {
            self.winner = Some(side);
            HitsResult::Winner(side)
        } else {
            HitsResult::Surrenders(*to)
        }
    }

    pub(crate) fn draw_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..3 {
            if self.events_pool.is_empty() {
                break;
            }
            let idx = self.rng.gen_range(0..self.events_pool.len());
            let event = self.events_pool.remove(idx);
            events.push(event);
        }
        events
    }

    pub(crate) fn can_draw_event(&mut self, event: &Event) -> bool {
        self.events_pool.contains(event)
    }

    pub(crate) fn add_event(&mut self, event: Event) {
        self.events_pool.push(event)
    }

    pub(crate) fn operational_level(&self, nation: &Nation) -> u8 {
        self.nations.get(nation).unwrap().operational_level()
    }

    pub(crate) fn breakdown_level(&self, nation: &Nation) -> u8 {
        self.nations.get(nation).unwrap().breakdown_level()
    }

    pub fn new_year(&mut self, current_turn_year: u16, next_year: u16) {
        self.events_pool.retain(|event| {
            event.not_after.is_none() || event.not_after.unwrap() > current_turn_year
        });
        self.events_pool.extend(
            ALL_EVENTS
                .iter()
                .filter(|event| event.year == next_year)
                .cloned(),
        );
    }

    /// List technologies available to the given side in the current turn
    pub(crate) fn available_technologies(&self, side: &Side) -> Vec<Technology> {
        match side {
            Side::Allies => self.available_technologies_for(&ALLIES_TECHNOLOGIES),
            Side::Empires => self.available_technologies_for(&EMPIRE_TECHNOLOGIES),
        }
    }

    fn available_technologies_for(
        &self,
        technologies: &[[Option<Technology>; 4]; 4],
    ) -> Vec<Technology> {
        let tech_map: HashMap<TechnologyType, u8> = self
            .state_of_war
            .get(&Side::Empires)
            .unwrap()
            .technologies
            .as_ref()
            .into();
        technologies
            .iter()
            .flatten()
            .filter_map(|s| *s)
            .filter(|tech| {
                tech.date <= self.current_year() && tech.level == tech_map[&tech.category] + 1
            })
            .collect()
    }

    /// List enemy nations neighbouring the given nation
    pub(crate) fn neighbours(&self, source: &Nation) -> Vec<&Nation> {
        let mut neighbours = Vec::new();
        for n in Nation::values() {
            if source.adjacent_to(n) && self.is_at_war(n) {
                neighbours.push(n);
            }
        }
        neighbours
    }

    /// Evaluate the value of the given state, yielding a number -1 and +1 where
    /// positive values are better for the Allies and negative values are better for the Empires.
    pub fn valuation(&self) -> f64 {
        let allies = self.state_of_war.get(&Side::Allies).unwrap();
        let empires = self.state_of_war.get(&Side::Empires).unwrap();
        let allies_resources = allies.resources as f64;
        let empires_resources = empires.resources as f64;
        let allies_technologies = allies.technologies.values().into_iter().sum::<u8>() as f64;
        let empires_technologies = empires.technologies.values().into_iter().sum::<u8>() as f64;
        let (allies_breakdowns, empires_breakdowns): (f64, f64) = self.nations.iter().fold(
            (0.0, 0.0),
            |(acc_a, acc_e), (nation, status)| match status {
                NationState::AtWar(level) => {
                    if nation.is_allies() {
                        (acc_a + *level as f64, acc_e)
                    } else {
                        (acc_a, acc_e + *level as f64)
                    }
                }
                _ => (acc_a, acc_e),
            },
        );
        let allies_victory_points = allies.vp as f64;
        let empires_victory_points = empires.vp as f64;
        let allies_total = allies_resources
            + allies_technologies * 3.0
            + allies_breakdowns * 5.0
            + allies_victory_points * 4.0;
        let empires_total = empires_resources
            + empires_technologies * 3.0
            + empires_breakdowns * 5.0
            + empires_victory_points * 4.0;
        (allies_total - empires_total) / (allies_total + empires_total)
    }

    fn is_at_war(&self, n: &Nation) -> bool {
        matches!(self.nations.get(n), Some(NationState::AtWar(_)))
    }

    pub(crate) fn game_ends(&self) -> bool {
        self.end_game_this_turn || self.current_turn >= 15 || self.winner.is_some()
    }

    pub(crate) fn winner(&self) -> Side {
        match self.winner {
            Some(side) => side,
            None => {
                let empires_pv = self.state_of_war.get(&Side::Empires).unwrap().vp;
                let allies_pv = self.state_of_war.get(&Side::Allies).unwrap().vp;
                if empires_pv >= allies_pv {
                    Side::Empires
                } else {
                    Side::Allies
                }
            }
        }
    }

    pub(crate) fn apply_change(&mut self, change: &StateChange) -> &mut Self {
        match change {
            StateChange::NoChange => {}
            StateChange::ChangeResources { side, pr } => {
                if *pr >= 0 {
                    self.increase_pr(*side, *pr as u8);
                } else {
                    self.decrease_pr(*side, -*pr as u8);
                }
            }
            StateChange::MoreChanges(changes) => {
                changes.iter().for_each(|change| {
                    self.apply_change(change);
                });
            }
        }
        self
    }

    pub(crate) fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }

    pub(crate) fn side_to_play(&self) -> Option<Side> {
        match self.phase {
            Phase::Initiative(side) => Some(side),
            Phase::ImproveTechnologies(side) => Some(side),
            Phase::LaunchOffensives(side) => Some(side),
            Phase::Reinforcements(side) => Some(side),
            Phase::DrawEvents => None,
            Phase::CollectResources => None,
            Phase::UBoot => Some(Side::Empires),
            Phase::Blockade => Some(Side::Allies),
            Phase::NewTurn => None,
        }
    }
}

impl Display for GameState {
    /// TODO: take care of writeln! result
    #[allow(unused_must_use)]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "Turn: {} ({})", self.current_turn, self.current_year());
        writeln!(f, "Initiative: {}", self.initiative);
        if let Some(winner) = self.winner {
            writeln!(f, "Winner: {}", winner);
        } else {
            writeln!(f, "Game value: {}", self.valuation());
        }
        writeln!(f, "Russian Revolution: {}", self.russian_revolution);
        writeln!(f, "Breakdown:");
        for (nation, status) in self.nations.iter() {
            writeln!(f, "\t{}: {}", nation, status);
        }
        writeln!(f, "State of War:");
        for (side, war_state) in self.state_of_war.iter() {
            writeln!(f, "\t{}:", side);
            writeln!(f, "\t\tResources: {}", war_state.resources);
            writeln!(f, "\t\tVP: {}", war_state.vp);
            writeln!(f, "\t\tTechnologies: {}", war_state.technologies);
        }
        Ok(())
    }
}

#[cfg(test)]
mod game_state_tests {

    use super::HitsResult::*;
    use crate::{
        fixtures::EngineBuilder, GameState, Nation::*, NationState::*, Side::*, StateChange,
        ZERO_TECHNOLOGIES,
    };

    #[test]
    fn nation_surrenders_when_brought_to_0_then_increase_vp_of_other_side() {
        let mut engine = EngineBuilder::new(14) // die roll = 6
            .with_nation(France, AtWar(4))
            .build();
        let result = engine.apply_hits(&France, 4);

        assert_eq!(Surrenders(France), result);
        assert_eq!(6, engine.state.state_of_war.get(&Empires).unwrap().vp);
        assert_eq!(AtPeace, engine.state.nations.get(&France).unwrap().clone());
    }

    #[test]
    fn side_wins_and_game_ends_if_die_roll_lower_than_vp_given_nation_surrenders() {
        let mut engine = EngineBuilder::new(11) // die roll = 2
            .with_nation(France, AtWar(4))
            .build();
        let result = engine.apply_hits(&France, 4);

        assert_eq!(Empires, engine.state.winner());
        assert_eq!(Winner(Empires), result);
        assert!(engine.game_ends());
    }

    #[test]
    fn empires_is_winning_given_they_have_more_vps() {
        let mut engine = EngineBuilder::new(11) // die roll = 2
            .with_nation(Italy, AtWar(4))
            .build();

        engine.apply_hits(&Italy, 4);

        assert_eq!(Empires, engine.state.winner());
    }

    #[test]
    fn empires_is_winning_given_they_have_equal_vps() {
        let mut engine = EngineBuilder::new(11) // die roll = 2
            .with_nation(Italy, AtWar(4))
            .with_nation(OttomanEmpire, AtWar(4))
            .build();

        engine.apply_hits(&Italy, 4);
        engine.apply_hits(&OttomanEmpire, 4);

        assert_eq!(Empires, engine.state.winner());
    }

    #[test]
    fn allies_is_winning_given_they_have_more_vps() {
        let mut engine = EngineBuilder::new(11) // die roll = 2
            .with_nation(Italy, AtWar(4))
            .with_nation(AustriaHungary, AtWar(4))
            .build();

        engine.apply_hits(&Italy, 4);
        engine.apply_hits(&AustriaHungary, 4);

        assert_eq!(Allies, engine.state.winner());
    }

    #[test]
    fn breakdown_returns_hits_lost() {
        let mut engine = EngineBuilder::new(11).with_nation(France, AtWar(4)).build();

        assert_eq!(Hits(France, 2), engine.apply_hits(&France, 2));
    }

    #[test]
    fn breakdown_does_not_inflict_hits_given_nation_is_not_at_war() {
        let mut engine = EngineBuilder::new(11).build();

        assert_eq!(NationNotAtWar(Italy), engine.apply_hits(&Italy, 2));
    }

    #[test]
    fn only_technologies_for_1914_are_available_at_start() {
        let engine = EngineBuilder::new(11).build();

        let allies_techs = engine.state.available_technologies(&Allies);
        assert!(allies_techs.iter().all(|tech| tech.date == 1914));

        let empires_techs = engine.state.available_technologies(&Empires);
        assert!(empires_techs.iter().all(|tech| tech.date == 1914));
    }

    #[test]
    fn only_technologies_beyond_current_level_are_available() {
        let engine = EngineBuilder::new(11)
            .with_technologies(
                Empires,
                crate::Technologies {
                    defense: 1,
                    ..ZERO_TECHNOLOGIES
                },
            )
            .with_technologies(
                Allies,
                crate::Technologies {
                    attack: 1,
                    ..ZERO_TECHNOLOGIES
                },
            )
            .on_turn(3)
            .build();

        let allies_techs = engine.state.available_technologies(&Allies);

        assert_eq!(
            vec!["Combat Gas", "Trench warfare", "Heavy artillery", "Reco"],
            allies_techs
                .iter()
                .map(|tech| tech.name.into())
                .collect::<Vec<String>>()
        );

        let empires_techs = engine.state.available_technologies(&Empires);

        assert_eq!(
            vec!["Combat Gas", "Trench warfare", "Heavy artillery", "Reco"],
            empires_techs
                .iter()
                .map(|tech| tech.name.into())
                .collect::<Vec<String>>()
        );
    }

    #[test]
    fn can_list_all_neighbouring_nations_one_can_attack() {
        let engine = EngineBuilder::new(11).build();

        assert_eq!(vec![&Germany], engine.state.neighbours(&France));
        assert_eq!(vec![&France, &Russia], engine.state.neighbours(&Germany));
    }

    #[test]
    fn no_change_does_not_modify_state() {
        let mut state = GameState::new(12);
        state.increase_pr(Allies, state.tally_resources(&Allies));

        state.apply_change(&StateChange::NoChange);

        assert_eq!(14, state.resources_for(&Allies));
    }

    #[test]
    fn applying_change_modifies_state() {
        let mut state = GameState::new(12);
        state.increase_pr(Allies, state.tally_resources(&Allies));

        state.apply_change(&StateChange::ChangeResources {
            side: Allies,
            pr: -4,
        });

        assert_eq!(10, state.resources_for(&Allies));
    }

    #[test]
    fn applying_more_changes_modifies_state_for_each_individual_change() {
        let mut state = GameState::new(12);
        state.increase_pr(Allies, state.tally_resources(&Allies));
        state.increase_pr(Empires, state.tally_resources(&Empires));

        let reduce_pr = StateChange::ChangeResources {
            side: Allies,
            pr: -4,
        };
        let add_pr = StateChange::ChangeResources {
            side: Empires,
            pr: 2,
        };
        state.apply_change(&StateChange::MoreChanges(vec![reduce_pr, add_pr]));

        assert_eq!(10, state.resources_for(&Allies));
        assert_eq!(11, state.resources_for(&Empires));
    }
}
