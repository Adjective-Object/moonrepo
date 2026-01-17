//! Benchmark for evaluating task scheduling against a real workspace.
//!
//! This benchmark loads the client-web workspace and computes the full run schedule
//! for the `*:vitest-migrate` target pattern without actually running tasks.
//!
//! Run with: cargo bench --bench task_schedule

use criterion::{Criterion, criterion_group, criterion_main};
use moon_action_graph::{ActionGraphBuilder, ActionGraphBuilderOptions, RunRequirements};
use moon_task::TargetLocator;
use moon_test_utils2::WorkspaceMocker;
use moon_workspace_graph::WorkspaceGraph;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

const CLIENT_WEB_PATH: &str = env!("HOME");

fn get_client_web_path() -> PathBuf {
    PathBuf::from(CLIENT_WEB_PATH).join("client-web")
}

/// Container for setting up action graph building against client-web workspace
struct ClientWebActionGraph {
    mocker: WorkspaceMocker,
}

impl ClientWebActionGraph {
    fn new() -> Self {
        let root = get_client_web_path();

        let mocker = WorkspaceMocker::new(&root)
            .load_default_configs()
            .with_all_toolchains()
            .with_default_projects()
            .with_global_envs();

        Self { mocker }
    }

    async fn create_workspace_graph(&self) -> Arc<WorkspaceGraph> {
        Arc::new(self.mocker.mock_workspace_graph().await)
    }

    async fn create_builder(&self, workspace_graph: Arc<WorkspaceGraph>) -> ActionGraphBuilder<'_> {
        let config = &self.mocker.workspace_config.pipeline;

        ActionGraphBuilder::new(
            Arc::new(self.mocker.mock_app_context()),
            workspace_graph,
            ActionGraphBuilderOptions {
                install_dependencies: config.install_dependencies.clone(),
                setup_environment: true.into(),
                setup_toolchains: true.into(),
                sync_projects: config.sync_projects.clone(),
                sync_project_dependencies: config.sync_project_dependencies,
                sync_workspace: config.sync_workspace,
            },
        )
        .unwrap()
    }

    async fn create_builder_minimal(
        &self,
        workspace_graph: Arc<WorkspaceGraph>,
    ) -> ActionGraphBuilder<'_> {
        // Minimal options to focus on task graph building only
        ActionGraphBuilder::new(
            Arc::new(self.mocker.mock_app_context()),
            workspace_graph,
            ActionGraphBuilderOptions::new(false),
        )
        .unwrap()
    }
}

/// Benchmark loading the workspace graph for client-web
fn bench_load_workspace_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_web_workspace");

    // Increase sample size timeout for this potentially slow operation
    group.sample_size(10);

    let container = ClientWebActionGraph::new();
    let rt = Runtime::new().unwrap();

    group.bench_function("load_workspace_graph", |b| {
        b.iter(|| rt.block_on(async { container.create_workspace_graph().await }));
    });

    group.finish();
}

/// Benchmark building the action graph and computing the run schedule
/// for *:vitest-migrate target pattern
fn bench_vitest_migrate_schedule(c: &mut Criterion) {
    let mut group = c.benchmark_group("vitest_migrate_schedule");

    // Use smaller sample size due to expensive operations
    group.sample_size(10);

    let container = ClientWebActionGraph::new();
    let rt = Runtime::new().unwrap();

    // Pre-load the workspace graph once (this is expensive)
    let workspace_graph = rt.block_on(container.create_workspace_graph());

    // Benchmark: Build action graph with full options (includes sync, install deps, etc)
    group.bench_function("full_action_graph", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut builder = container.create_builder(workspace_graph.clone()).await;

                let locator = TargetLocator::parse("*:vitest-migrate").unwrap();
                let requirements = RunRequirements::default();

                builder
                    .run_task_by_target_locator(&locator, &requirements)
                    .await
                    .unwrap();

                let (action_context, action_graph) = builder.build();

                // Compute topological sort (the run schedule)
                let schedule = action_graph.sort_topological().unwrap();

                (action_context, action_graph, schedule)
            })
        });
    });

    // Benchmark: Build action graph with minimal options (no sync, no install deps)
    group.bench_function("minimal_action_graph", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut builder = container
                    .create_builder_minimal(workspace_graph.clone())
                    .await;

                let locator = TargetLocator::parse("*:vitest-migrate").unwrap();
                let requirements = RunRequirements::default();

                builder
                    .run_task_by_target_locator(&locator, &requirements)
                    .await
                    .unwrap();

                let (action_context, action_graph) = builder.build();

                // Compute topological sort (the run schedule)
                let schedule = action_graph.sort_topological().unwrap();

                (action_context, action_graph, schedule)
            })
        });
    });

    // Benchmark: Just the topological sort (schedule computation)
    group.bench_function("topological_sort_only", |b| {
        // Build the graph once outside the benchmark
        let (_, action_graph) = rt.block_on(async {
            let mut builder = container.create_builder(workspace_graph.clone()).await;

            let locator = TargetLocator::parse("*:vitest-migrate").unwrap();
            let requirements = RunRequirements::default();

            builder
                .run_task_by_target_locator(&locator, &requirements)
                .await
                .unwrap();

            builder.build()
        });

        b.iter(|| {
            // Just measure the sort/scheduling time
            action_graph.sort_topological().unwrap()
        });
    });

    // Benchmark: Priority grouping (used for batching actions)
    group.bench_function("priority_grouping", |b| {
        let (_, action_graph) = rt.block_on(async {
            let mut builder = container.create_builder(workspace_graph.clone()).await;

            let locator = TargetLocator::parse("*:vitest-migrate").unwrap();
            let requirements = RunRequirements::default();

            builder
                .run_task_by_target_locator(&locator, &requirements)
                .await
                .unwrap();

            builder.build()
        });

        let topo = action_graph.sort_topological().unwrap();

        b.iter(|| action_graph.group_priorities(topo.clone()));
    });

    group.finish();
}

/// Print schedule information (not a benchmark, but useful for analysis)
fn bench_print_schedule_info(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule_info");
    group.sample_size(10);

    let container = ClientWebActionGraph::new();
    let rt = Runtime::new().unwrap();

    let workspace_graph = rt.block_on(container.create_workspace_graph());

    // Run once to print stats
    let (action_context, action_graph, schedule) = rt.block_on(async {
        let mut builder = container.create_builder(workspace_graph.clone()).await;

        let locator = TargetLocator::parse("*:vitest-migrate").unwrap();
        let requirements = RunRequirements::default();

        builder
            .run_task_by_target_locator(&locator, &requirements)
            .await
            .unwrap();

        let (action_context, action_graph) = builder.build();
        let schedule = action_graph.sort_topological().unwrap();

        (action_context, action_graph, schedule)
    });

    println!("\n=== Schedule Information for *:vitest-migrate ===");
    println!("Total actions in graph: {}", action_graph.get_node_count());
    println!("Schedule length: {}", schedule.len());
    println!("Primary targets: {}", action_context.primary_targets.len());

    // Group by priority
    let priorities = action_graph.group_priorities(schedule.clone());
    for (priority, indices) in &priorities {
        let priority_name = match priority {
            0 => "critical",
            1 => "high",
            2 => "normal",
            3 => "low",
            _ => "other",
        };
        println!(
            "  Priority {} ({}): {} actions",
            priority,
            priority_name,
            indices.len()
        );
    }

    // Print first few and last few actions
    println!("\nFirst 10 actions in schedule:");
    for (i, index) in schedule.iter().take(10).enumerate() {
        if let Some(node) = action_graph.get_node_from_index(index) {
            println!("  {}: {}", i, node.label());
        }
    }

    if schedule.len() > 20 {
        println!("\n... ({} actions omitted) ...", schedule.len() - 20);
    }

    println!("\nLast 10 actions in schedule:");
    for (i, index) in schedule.iter().rev().take(10).rev().enumerate() {
        if let Some(node) = action_graph.get_node_from_index(index) {
            println!("  {}: {}", schedule.len() - 10 + i, node.label());
        }
    }

    // Benchmark just to satisfy criterion
    group.bench_function("noop", |b| b.iter(|| 1 + 1));

    group.finish();
}

criterion_group!(
    benches,
    bench_load_workspace_graph,
    bench_vitest_migrate_schedule,
    bench_print_schedule_info
);
criterion_main!(benches);
