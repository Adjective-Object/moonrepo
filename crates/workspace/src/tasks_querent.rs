use crate::build_data::{ProjectBuildData, TaskBuildData};
use moon_common::Id;
use moon_config::{ProjectDependencyConfig, ProjectDependsOn};
use moon_task::{Target, TaskOptions};
use moon_task_builder::TasksQuerent;
use rustc_hash::{FxHashMap, FxHashSet};

/// Precompute transitive dependencies for all projects in the workspace.
/// Returns a map from project ID to a list of all its transitive dependencies.
pub fn compute_transitive_deps(
    project_data: &FxHashMap<Id, ProjectBuildData>,
) -> FxHashMap<Id, Vec<Id>> {
    let mut transitive_deps: FxHashMap<Id, Vec<Id>> = FxHashMap::default();

    // First, build a map of direct dependencies for each project
    let direct_deps: FxHashMap<&Id, Vec<&Id>> = project_data
        .iter()
        .map(|(id, data)| {
            let deps: Vec<&Id> = data
                .config
                .as_ref()
                .map(|config| {
                    config
                        .depends_on
                        .iter()
                        .filter_map(|dep| {
                            let dep_id = match dep {
                                ProjectDependsOn::String(id) => id,
                                ProjectDependsOn::Object(ProjectDependencyConfig {
                                    id, ..
                                }) => id,
                            };
                            // Only include if the dependency exists
                            if project_data.contains_key(dep_id) {
                                Some(dep_id)
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            (id, deps)
        })
        .collect();

    // For each project, compute its transitive closure
    for project_id in project_data.keys() {
        let mut visited = FxHashSet::default();
        let mut result = Vec::new();
        let mut frontier: Vec<&Id> = direct_deps
            .get(project_id)
            .map(|deps| deps.clone())
            .unwrap_or_default();

        visited.insert(project_id);

        while let Some(dep_id) = frontier.pop() {
            if !visited.insert(dep_id) {
                continue;
            }
            result.push(dep_id.clone());

            // Add this dependency's dependencies to the frontier
            if let Some(deps) = direct_deps.get(dep_id) {
                for d in deps {
                    if !visited.contains(d) {
                        frontier.push(d);
                    }
                }
            }
        }

        transitive_deps.insert(project_id.clone(), result);
    }

    transitive_deps
}

pub struct WorkspaceBuilderTasksQuerent<'builder> {
    pub project_data: &'builder FxHashMap<Id, ProjectBuildData>,
    pub all_project_ids: &'builder Option<Vec<Id>>,
    pub projects_by_tag: &'builder FxHashMap<Id, Vec<Id>>,
    pub task_data: &'builder FxHashMap<Target, TaskBuildData>,
    pub transitive_deps: &'builder Option<FxHashMap<Id, Vec<Id>>>,
}

impl<'builder> TasksQuerent for WorkspaceBuilderTasksQuerent<'builder> {
    type IdsCollection<'a>
        = std::slice::Iter<'a, Id>
    where
        Self: 'a;

    fn query_projects_by_tag(&self, tag: &str) -> miette::Result<std::slice::Iter<'_, Id>> {
        Ok(self
            .projects_by_tag
            .get(tag)
            .map(|list| list.iter())
            .unwrap_or_default())
    }

    fn query_tasks(
        &self,
        project_ids: Vec<&Id>,
        task_id: &Id,
    ) -> miette::Result<Vec<(&Target, &TaskOptions)>> {
        // May be an alias!
        let project_ids = project_ids
            .iter()
            .map(|id| ProjectBuildData::resolve_id(id, self.project_data))
            .collect::<Vec<_>>();

        let results = self
            .task_data
            .iter()
            .filter_map(|(target, data)| {
                let project_id = target.get_project_id().ok()?;

                if &target.task_id == task_id && project_ids.contains(project_id) {
                    Some((target, &data.options))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(results)
    }

    fn query_transitive_deps(&self, project_id: &Id) -> miette::Result<std::slice::Iter<'_, Id>> {
        self.transitive_deps
            .as_ref()
            .and_then(|td| td.get(project_id).map(|deps| deps.iter()))
            .ok_or_else(|| {
                miette::miette!(
                    "Transitive dependencies for project {} were not precomputed",
                    project_id
                )
            })
    }

    fn query_all(&self) -> miette::Result<std::slice::Iter<'_, Id>> {
        self.all_project_ids
            .as_ref()
            .map(|ids| ids.iter())
            .ok_or_else(|| miette::miette!("List of all dependencies was not precomputed",))
    }
}
