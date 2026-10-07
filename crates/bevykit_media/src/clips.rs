//! Animation clips discovered from a glTF scene.
//!
//! When a glTF scene instance is ready, its named clips are collected into an
//! [`AnimationLibrary`] on the scene root, together with the [`AnimationPlayer`] entity and a
//! generated [`AnimationGraph`] with one node per clip. The player receives the graph and
//! [`AnimationTransitions`], so clips can be played by name through
//! [`Animations`](crate::controller::Animations).

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::{AnimationClip, AnimationPlayer, transition::AnimationTransitions};
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};

use crate::controller::AnimationPlayback;

/// A clip in an [`AnimationLibrary`].
#[derive(Clone, Debug)]
pub struct LibraryClip {
    /// The clip's node in the generated graph.
    pub node: AnimationNodeIndex,
    /// The clip asset.
    pub clip: Handle<AnimationClip>,
}

/// The named clips of a scene instance and the player that animates them.
#[derive(Component, Clone, Debug)]
pub struct AnimationLibrary {
    player: Entity,
    graph: Handle<AnimationGraph>,
    clips: HashMap<String, LibraryClip>,
}

impl AnimationLibrary {
    /// Returns the entity with the [`AnimationPlayer`].
    pub fn player(&self) -> Entity {
        self.player
    }

    /// Returns the generated graph.
    pub fn graph(&self) -> &Handle<AnimationGraph> {
        &self.graph
    }

    /// Returns a clip by name.
    pub fn clip(&self, name: &str) -> Option<&LibraryClip> {
        self.clips.get(name)
    }

    /// Iterates the clip names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.clips.keys().map(String::as_str)
    }
}

pub(crate) fn build_animation_library(
    ready: On<WorldInstanceReady>,
    roots: Query<&WorldAssetRoot>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let root = ready.entity;
    let Ok(scene) = roots.get(root) else {
        return;
    };
    let Some(player) = children
        .iter_descendants(root)
        .find(|entity| players.contains(*entity))
    else {
        return;
    };
    let Some(gltf) = scene
        .0
        .path()
        .and_then(|path| asset_server.get_handle::<Gltf>(path.without_label()))
        .and_then(|handle| gltfs.get(&handle))
    else {
        return;
    };

    let (graph, nodes) = AnimationGraph::from_clips(gltf.named_animations.values().cloned());
    let clips = gltf
        .named_animations
        .iter()
        .zip(nodes)
        .map(|((name, clip), node)| {
            let clip = LibraryClip {
                node,
                clip: clip.clone(),
            };
            (name.to_string(), clip)
        })
        .collect();
    let graph = graphs.add(graph);
    commands
        .entity(player)
        .insert((AnimationGraphHandle(graph.clone()), AnimationTransitions::new()));
    commands.entity(root).insert((
        AnimationLibrary {
            player,
            graph,
            clips,
        },
        AnimationPlayback::default(),
    ));
}
