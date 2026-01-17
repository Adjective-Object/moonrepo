use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use moon_common::Id;
use moon_config::{ProjectConfig, ProjectDependencyConfig, ProjectDependsOn};
use moon_task::{Target, TaskOptions};
use moon_task_builder::TasksQuerent;
use moon_workspace::{
    ProjectBuildData, TaskBuildData, WorkspaceBuilderTasksQuerent, compute_transitive_deps,
};
use rustc_hash::FxHashMap;

/// Synthetic data generator for benchmarks.
/// Creates a linear chain of project dependencies: p0 <- p1 <- p2 <- ... <- pN
/// and a few high-connectivity "hub" projects that many packages depend on.
struct SyntheticWorkspace {
    project_data: FxHashMap<Id, ProjectBuildData>,
    all_project_ids: Option<Vec<Id>>,
    projects_by_tag: FxHashMap<Id, Vec<Id>>,
    task_data: FxHashMap<Target, TaskBuildData>,
}

impl SyntheticWorkspace {
    fn new(num_projects: usize, num_hub_projects: usize) -> Self {
        let mut project_data = FxHashMap::default();
        let mut task_data = FxHashMap::default();
        let projects_by_tag = FxHashMap::default();

        // Create hub projects first (many projects will depend on these)
        for hub_idx in 0..num_hub_projects {
            let hub_id = Id::raw(format!("hub{hub_idx}"));
            project_data.insert(
                hub_id.clone(),
                ProjectBuildData {
                    config: Some(ProjectConfig {
                        depends_on: vec![], // Hub projects have no dependencies
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );

            // Add a build task for hub
            let target = Target::new(&hub_id, "build").unwrap();
            task_data.insert(
                target,
                TaskBuildData {
                    options: TaskOptions::default(),
                    ..Default::default()
                },
            );
        }

        // Create linear chain of projects: p0 <- p1 <- p2 <- ...
        // Each project depends on the previous one AND all hub projects
        for i in 0..num_projects {
            let project_id = Id::raw(format!("p{i}"));
            let mut depends_on = Vec::new();

            // Each project depends on the previous project (creating a chain)
            if i > 0 {
                depends_on.push(ProjectDependsOn::Object(ProjectDependencyConfig {
                    id: Id::raw(format!("p{}", i - 1)),
                    ..Default::default()
                }));
            }

            // Each project also depends on all hub projects
            for hub_idx in 0..num_hub_projects {
                depends_on.push(ProjectDependsOn::Object(ProjectDependencyConfig {
                    id: Id::raw(format!("hub{hub_idx}")),
                    ..Default::default()
                }));
            }

            project_data.insert(
                project_id.clone(),
                ProjectBuildData {
                    config: Some(ProjectConfig {
                        depends_on,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );

            // Add a build task for each project
            let target = Target::new(&project_id, "build").unwrap();
            task_data.insert(
                target,
                TaskBuildData {
                    options: TaskOptions::default(),
                    ..Default::default()
                },
            );
        }

        Self {
            all_project_ids: Some(project_data.keys().cloned().collect()),
            project_data,
            projects_by_tag,
            task_data,
        }
    }

    /// Create a querent WITH precomputed transitive deps (new approach)
    fn create_querent_with_cache<'a>(
        &'a self,
        cache: &'a FxHashMap<Id, Vec<Id>>,
    ) -> WorkspaceBuilderTasksQuerent<'a> {
        WorkspaceBuilderTasksQuerent {
            project_data: &self.project_data,
            all_project_ids: self.all_project_ids.as_ref(),
            projects_by_tag: &self.projects_by_tag,
            task_data: &self.task_data,
            transitive_deps: Some(cache),
        }
    }
}

fn bench_transitive_deps(c: &mut Criterion) {
    let mut group = c.benchmark_group("transitive_deps");

    // Test with different workspace sizes
    for num_projects in [50, 100, 200, 500] {
        let num_hubs = 5;
        let workspace = SyntheticWorkspace::new(num_projects, num_hubs);

        // Benchmark NEW approach: precompute once, then lookup
        group.bench_with_input(
            BenchmarkId::new("new_precomputed", num_projects),
            &workspace,
            |b, ws| {
                b.iter(|| {
                    // Precompute all transitive deps once
                    let cache = compute_transitive_deps(&ws.project_data);

                    // Then lookup for each project
                    let querent = ws.create_querent_with_cache(&cache);
                    for i in 0..num_projects {
                        let project_id = Id::raw(format!("p{i}"));
                        let _ = querent.query_transitive_deps(&project_id);
                    }
                });
            },
        );

        // Benchmark just the precomputation step
        group.bench_with_input(
            BenchmarkId::new("precompute_only", num_projects),
            &workspace,
            |b, ws| {
                b.iter(|| compute_transitive_deps(&ws.project_data));
            },
        );

        // Benchmark just the lookups (after precomputation)
        let cache = compute_transitive_deps(&workspace.project_data);
        group.bench_with_input(
            BenchmarkId::new("lookup_only", num_projects),
            &(&workspace, &cache),
            |b, (ws, cache)| {
                b.iter(|| {
                    let querent = ws.create_querent_with_cache(cache);
                    for i in 0..num_projects {
                        let project_id = Id::raw(format!("p{i}"));
                        let _ = querent.query_transitive_deps(&project_id);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_transitive_deps);
criterion_main!(benches);
