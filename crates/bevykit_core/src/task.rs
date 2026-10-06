//! Asynchronous work bound to the ECS and, optionally, to a [`Scope`](crate::scope::Scope).
//!
//! Each pending task is stored on its own entity. When the task finishes, its completion is
//! applied to the world as a command. A task spawned inside a scope is owned by it: closing the
//! scope despawns the task entity, which drops and cancels the future. A late result therefore
//! can never be applied to a scope that has already closed.
//!
//! ```ignore
//! fn load_metadata(mut tasks: KitTasks, scope: Single<Entity, With<LevelScope>>) {
//!     tasks.spawn_in(*scope, async move {
//!         let metadata = fetch_metadata().await;
//!         move |world: &mut World| {
//!             world.insert_resource(metadata);
//!         }
//!     });
//! }
//! ```

use std::future::Future;

use bevy::ecs::error::CommandOutput;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, IoTaskPool, Task};

use crate::scope::OwnedBy;

type Completion = Box<dyn FnOnce(&mut World) + Send + 'static>;

/// A pending task. Despawning the entity cancels the task.
#[derive(Component)]
pub struct PendingTask {
    task: Task<Completion>,
    label: &'static str,
}

impl PendingTask {
    /// Returns the diagnostic label of the task.
    pub fn label(&self) -> &'static str {
        self.label
    }
}

/// Selects the thread pool used to run a task.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum TaskPoolKind {
    /// CPU-bound work, run on the async compute pool.
    #[default]
    Compute,
    /// IO-bound work, run on the IO pool.
    Io,
}

/// System parameter for spawning tasks whose results are applied to the world.
#[derive(SystemParam)]
pub struct KitTasks<'w, 's> {
    commands: Commands<'w, 's>,
}

impl KitTasks<'_, '_> {
    /// Spawns a task owned by `scope`. The command returned by the future is applied when it
    /// completes, unless the scope has closed first.
    pub fn spawn_in<F, C>(&mut self, scope: Entity, future: F) -> Entity
    where
        F: Future<Output = C> + Send + 'static,
        C: Command + Send + 'static,
    {
        self.spawn_with(Some(scope), TaskPoolKind::Compute, "task", future)
    }

    /// Spawns an unscoped task whose resulting command is applied when it completes.
    pub fn spawn<F, C>(&mut self, future: F) -> Entity
    where
        F: Future<Output = C> + Send + 'static,
        C: Command + Send + 'static,
    {
        self.spawn_with(None, TaskPoolKind::Compute, "task", future)
    }

    /// Spawns a task owned by `scope` and passes its output to `then` on completion.
    pub fn spawn_in_then<F, T, H>(&mut self, scope: Entity, future: F, then: H) -> Entity
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
        H: FnOnce(T, &mut World) + Send + 'static,
    {
        self.spawn_with(Some(scope), TaskPoolKind::Compute, "task", async move {
            let output = future.await;
            move |world: &mut World| then(output, world)
        })
    }

    /// Spawns a task with full control over ownership, thread pool, and diagnostic label.
    pub fn spawn_with<F, C>(
        &mut self,
        scope: Option<Entity>,
        pool: TaskPoolKind,
        label: &'static str,
        future: F,
    ) -> Entity
    where
        F: Future<Output = C> + Send + 'static,
        C: Command + Send + 'static,
    {
        let wrapped = async move {
            let command = future.await;
            Box::new(move |world: &mut World| {
                if let Some(error) = command.apply(world).to_err() {
                    error!("Task `{label}` completion failed: {error}");
                }
            }) as Completion
        };
        let task = match pool {
            TaskPoolKind::Compute => AsyncComputeTaskPool::get().spawn(wrapped),
            TaskPoolKind::Io => IoTaskPool::get().spawn(wrapped),
        };
        let mut entity = self.commands.spawn(PendingTask { task, label });
        if let Some(scope) = scope {
            entity.insert(OwnedBy(scope));
        }
        entity.id()
    }

    /// Cancels a pending task. Cancelling a finished or unknown task is a no-op.
    pub fn cancel(&mut self, task: Entity) {
        if let Ok(mut entity) = self.commands.get_entity(task) {
            entity.try_despawn();
        }
    }
}

/// Polls pending tasks and applies the completions of finished ones.
pub fn poll_pending_tasks(mut commands: Commands, mut tasks: Query<(Entity, &mut PendingTask)>) {
    for (entity, mut pending) in &mut tasks {
        if let Some(completion) = check_ready(&mut pending.task) {
            commands.entity(entity).despawn();
            commands.queue(completion);
        }
    }
}
