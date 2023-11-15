use std::f32::consts::PI;
use bevy::prelude::*;
use rand::{distributions::WeightedIndex, prelude::Distribution};

use crate::{psychics::{Position, Soul, Trace, PsychicSettings}, nn::Net, axiom::Axiom, map::{Map, Species, build_map, xy_idx}};

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SimulationSettings{max_turn_number: 100, current_turn: 100, current_generation: 0});
        app.add_systems(Update, simulate_generation);
        app.add_systems(Update, evolve_generation);
        app.register_type::<SimulationSettings>();
    }
}

pub const PLAY_AREA_WIDTH: u32 = 45;
pub const PLAY_AREA_HEIGHT: u32 = 45;

#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
pub struct SimulationSettings {
    pub max_turn_number: usize,
    pub current_turn: usize,
    pub current_generation: usize
}

fn simulate_generation( // Trying hard to make this concurrent with time_passes. Not sure if it will work. 10th November 2023
    // In order to make effects and spells happen: make a vector of (position, effect). Then, at the start of next turn, make them all happen. 12th November 2023
    mut config: ResMut<SimulationSettings>,
    mut psychics: Query<(&mut Position, &mut Soul, &mut Trace, &Species), With<Soul>>,
    mut hylics: Query<(&mut Position, &mut Trace, &Species), Without<Soul>>,
    mut map: ResMut<Map>,
){    
    if config.current_turn == config.max_turn_number{
        return;
    }
    assert!(config.current_turn < config.max_turn_number);
    for turn in 0..config.max_turn_number+1{
        let mut beacon_of_light: (u32, u32) = (0,0);
        for (mut position, mut trace, species) in hylics.iter_mut(){
            let action = Axiom::Move { dx: 0, dy: 0 };
            (position.x, position.y) = process_motion(position.x, position.y, action, &map.tiles);
            trace.positions.push((position.x, position.y));
            match species{
                Species::Beacon => beacon_of_light = (position.x, position.y),
                _ => ()
            }
        }
        for (mut position, mut soul, mut trace, species) in psychics.iter_mut(){
            soul.senses_input = locate_quadrant(position.x, position.y, beacon_of_light.0, beacon_of_light.1);
            soul.senses_input.append(&mut find_adjacent_collisions((position.x, position.y), &map.tiles));
            soul.senses_input.append(&mut vec![10./(9.+((position.x as i32 - beacon_of_light.0 as i32).abs() + (position.y as i32 - beacon_of_light.1 as i32).abs()) as f64)]);
            soul.decision_outputs = soul.nn.decide(&soul.senses_input);
            let index_of_biggest = soul.decision_outputs.iter().enumerate().fold((0, 0.0), |max, (ind, &val)| if val > max.1 {(ind, val)} else {max});
            let final_decision = soul.action_choices[index_of_biggest.0];
            if !soul.actions_chosen.contains(&final_decision.act_motion()){ soul.actions_chosen.push(final_decision.act_motion())};
            let idx = map.xy_idx(position.x, position.y);
            map.tiles[idx] = Species::Nothing; // left tile becomes empty
            (position.x, position.y) = process_motion(position.x, position.y, final_decision, &map.tiles);
            let idx = map.xy_idx(position.x, position.y);
            map.tiles[idx] = species.clone(); // entered tile becomes full
            trace.positions.push((position.x, position.y));
        }
        config.current_turn = turn;
    }
}

pub fn process_x(new_pos: i32) -> i32 {
    match new_pos >= PLAY_AREA_WIDTH as i32{
        true => PLAY_AREA_WIDTH as i32-1,
        false => match new_pos < 0 {
            true => 0,
            false => new_pos
        }
    }
}

pub fn process_y(new_pos: i32) -> i32 {
    match new_pos >= PLAY_AREA_HEIGHT as i32{
        true => PLAY_AREA_HEIGHT as i32-1,
        false => match new_pos < 0 {
            true => 0,
            false => new_pos
        }
    }
}

pub fn find_adjacent_collisions(
    pos: (u32, u32),
    collision_map: &Vec<Species>
) -> Vec<f64>{
    let mut output = Vec::with_capacity(4);
    let mut search = Vec::with_capacity(4);
    for i in [(0,1), (1,0), (-1, 0), (0,-1)]{
        search.push(i);
    }
    for i in search{
        if target_is_empty(((pos.0 as i32+i.0) as u32, (pos.1 as i32+i.1) as u32), collision_map){
            output.push(1.);
        } else {output.push(0.)};
    }
    output

}


pub fn target_is_empty(
    new_pos: (u32, u32),
    collision_map: &Vec<Species>,
) -> bool {
    let idx = xy_idx(new_pos.0, new_pos.1);
    if collision_map[idx] == Species::Nothing{
        true
    }
    else { false }
}

fn process_motion(
    cur_x: u32,
    cur_y: u32,
    action: Axiom,
    collision_map: &Vec<Species>,
) -> (u32, u32){
    let (dx, dy) = action.act_motion();
    let new_coords = (process_x(cur_x as i32 + dx) as u32, process_y(cur_y as i32 + dy) as u32);
    if target_is_empty(new_coords, collision_map) { //
        new_coords
    } else { (cur_x, cur_y) }   
}

fn locate_quadrant( // Move this to a Senses file later
    ori_x: u32,
    ori_y: u32,
    dest_x: u32,
    dest_y: u32,
) -> Vec<f64> {
    let dx = dest_x as i32-ori_x as i32;
    let dy = dest_y as i32-ori_y as i32;
    let mut theta: f32;
    match dx == 0{
        true => match dy > 0 {
            true => theta = 90.,
            false => theta = 270.,
        }
        false => theta = ((dy) as f32).atan2(dx as f32) * (180./PI),
    }
    match theta < 0.{
        true => theta += 360.,
        false=> ()
    }
    let result = theta as u32;
    let angles = [270, 90, 180, 0];
    let mut output = [0., 0., 0., 0.];
    for (i, a) in angles.iter().enumerate(){
        let mut sense = - (0.55 * (result/100) as f32 - (a/180) as f32).abs() + 1.;
        if sense < 0. {sense = 0.}
        output[i] = sense as f64;
    }
    output.to_vec()
}

fn evolve_generation(
    mut config: ResMut<SimulationSettings>,
    mut psychics: Query<(&mut Position, &mut Soul, &mut Trace, &Species), With<Soul>>, // Consider making this the same query with Has<Soul>
    psy_settings: Res<PsychicSettings>,
    mut hylics: Query<(&mut Position, &mut Trace, &Species), Without<Soul>>,
    mut map: ResMut<Map>,

){
    if config.current_turn < config.max_turn_number{
        return;
    }
    (map.tiles, map.catalogue, map.locations) = build_map(map.population.clone());
    let mut beacon_of_light: (u32, u32) = (0,0); // Very gory when more Hylics will get added.
    for (mut pos, mut trace, species) in hylics.iter_mut(){
        trace.shipped_positions = trace.positions.clone();
        trace.positions = Vec::with_capacity(config.max_turn_number);
        let index = map.catalogue.iter().position(|r| r == species).unwrap();
        if map.locations[index].is_empty(){
            trace.positions.push((0, 0));
            // Super gory. Since we're always stuck with too many walls, some of them can't find a position and get tucked in a stack in the corner instead. Fix this.
            break;
        }
        let Some((x,y)) = map.locations[index].pop() else { panic!("Locations assigment did not find an XY pair.") };
        (pos.x, pos.y) = (x, y);
        pos.starting_position = (x,y);
        trace.positions.push((x, y));
        match species{
            Species::Beacon => beacon_of_light = (pos.x, pos.y),
            _ => ()
        }
    }
    let mut all_souls: Vec<Net> = Vec::with_capacity(psy_settings.number_at_start as usize); 
    let mut all_fitnesses: Vec<f32> = Vec::with_capacity(psy_settings.number_at_start as usize);
    let mut best_fit = (0., 0);
    for (mut pos, mut soul, mut trace, species) in psychics.iter_mut(){
        let mut final_fitness = if (pos.x as i32 - beacon_of_light.0 as i32).abs() < 2 && (pos.y as i32 - beacon_of_light.1 as i32).abs() < 2{
            100.
        } else if (pos.x as i32 - beacon_of_light.0 as i32).abs() < 5 && (pos.y as i32 - beacon_of_light.1 as i32).abs() < 5{
            50.
        } else if (pos.x as i32 - beacon_of_light.0 as i32).abs() < 10 && (pos.y as i32 - beacon_of_light.1 as i32).abs() < 10{
            10.
        } else {
            1.
        };
        //dbg!(soul.actions_chosen.clone());
        if soul.actions_chosen.len() > 2{

            final_fitness += 1000.;
        }

        soul.actions_chosen = Vec::new();
        //30.-((pos.x as i32 - beacon_of_light.0 as i32).abs() + (pos.y as i32 - beacon_of_light.1 as i32).abs()) as f32;
        if pos.x == 44 || pos.y == 44 || pos.x == 0 || pos.x == 0{
            final_fitness *= 0.5;
        }
        if (pos.x, pos.y) == pos.starting_position{
            final_fitness = 0.3;
        }
        soul.fitness = final_fitness;

        let index = map.catalogue.iter().position(|r| r == species).unwrap();
        let Some((x,y)) = map.locations[index].pop() else { panic!("Locations assigment did not find an XY pair.") };
        (pos.x, pos.y) = (x, y);
        pos.starting_position = (x,y);
        trace.shipped_positions = trace.positions.clone();
        trace.positions = Vec::with_capacity(config.max_turn_number);
        trace.positions.push((x, y));
        match species{
            Species::Beacon => beacon_of_light = (pos.x, pos.y),
            _ => ()
        }

        all_souls.push(soul.nn.clone());
        all_fitnesses.push(final_fitness);
        if final_fitness > best_fit.0{
            best_fit = (final_fitness, all_fitnesses.len()-1);
        }
    }
    //dbg!(all_fitnesses.clone());
    let (_max_fitness, gene_pool) = create_gene_pool(all_fitnesses);
    let mut rng = rand::thread_rng();
    for (mut _position, mut soul, mut _trace, _species) in psychics.iter_mut(){
        let soul_idx = gene_pool.sample(&mut rng);
        let mut rand_soul = all_souls[soul_idx].clone(); // soul_idx
        rand_soul.mutate();
        soul.nn = rand_soul;
    }
    config.current_turn = 0 ;
    config.current_generation += 1;
}

fn create_gene_pool(values: Vec<f32>) -> (f32, WeightedIndex<f32>) {
    let mut max_fitness = 0.0;
    let mut weights = Vec::new();

    for v in values.iter() {
        if *v > max_fitness {
            max_fitness = *v;
        }
        weights.push(*v);
    }

    (
        max_fitness,
        WeightedIndex::new(&weights).expect("Failed to generate gene pool"),
    )
}